//! Native current-account and desktop operations. No shell/PATH interpreter.
use std::{
    ffi::c_void,
    mem::size_of,
    path::{Path, PathBuf},
};
use windows_api::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{HSTRING, PWSTR, w},
};
struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn sid_for_process(process: HANDLE) -> Result<String, String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token)
            .map_err(|_| "Cannot inspect the Windows account token.")?;
        let _token = OwnedHandle(token);
        let mut length = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut length);
        if length == 0 || length > 65536 {
            return Err("The Windows account token exceeds its bound.".into());
        }
        let mut storage =
            vec![0usize; (length as usize + size_of::<usize>() - 1) / size_of::<usize>()];
        GetTokenInformation(
            token,
            TokenUser,
            Some(storage.as_mut_ptr().cast::<c_void>()),
            length,
            &mut length,
        )
        .map_err(|_| "Cannot read the Windows account token.")?;
        let user = &*(storage.as_ptr().cast::<TOKEN_USER>());
        let mut value = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut value)
            .map_err(|_| "Cannot identify the Windows account.")?;
        let result = value
            .to_string()
            .map_err(|_| "The Windows account identifier is invalid.".to_owned());
        let _ = LocalFree(Some(HLOCAL(value.0.cast())));
        result
    }
}
pub fn current_sid() -> Result<String, String> {
    unsafe { sid_for_process(GetCurrentProcess()) }
}
pub fn same_user(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let _process = OwnedHandle(process);
        sid_for_process(process)
            .ok()
            .zip(current_sid().ok())
            .is_some_and(|(a, b)| a == b)
    }
}
/// The descriptor is explicitly current-SID-only. Windows defaults would grant
/// Everyone/anonymous read access to a named pipe and are not acceptable here.
pub struct PipeSecurity {
    descriptor: windows_api::Win32::Security::PSECURITY_DESCRIPTOR,
}
impl PipeSecurity {
    pub fn new() -> Result<Self, String> {
        let sid = current_sid()?;
        let text = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
        let mut descriptor = windows_api::Win32::Security::PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &text,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .map_err(|_| "Cannot protect the local Windows entry.")?;
        }
        Ok(Self { descriptor })
    }
    pub fn attributes(&self) -> windows_api::Win32::Security::SECURITY_ATTRIBUTES {
        windows_api::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: size_of::<windows_api::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: false.into(),
        }
    }
}
impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}
pub fn open_browser(url: &str) -> Result<(), String> {
    if !url.starts_with("http://127.0.0.1:") || url.len() > 512 || url.contains(['\0', '\r', '\n'])
    {
        return Err("Only the verified local browser entry may be opened.".into());
    }
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("Windows could not open the default browser. The companion remains running; use open --no-browser for its local URL.".into());
    }
    Ok(())
}
pub fn choose(kind: &str) -> Result<Option<PathBuf>, String> {
    use windows_api::Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            FOS_DONTADDTORECENT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
            FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
        },
    };
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|_| "Windows could not initialize the file chooser.")?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _apartment = Apartment;
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "The Windows file chooser is unavailable.")?;
        let mut options = dialog
            .GetOptions()
            .map_err(|_| "The Windows file chooser could not be configured.")?
            | FOS_FORCEFILESYSTEM
            | FOS_FILEMUSTEXIST
            | FOS_PATHMUSTEXIST
            | FOS_NOCHANGEDIR
            | FOS_DONTADDTORECENT;
        if ["folder", "sourceFolder"].contains(&kind) {
            options |= FOS_PICKFOLDERS;
        }
        dialog
            .SetOptions(options)
            .map_err(|_| "The Windows file chooser could not be configured.")?;
        dialog
            .SetTitle(w!("Choose for PhotoFindry"))
            .map_err(|_| "The file chooser title could not be set.")?;
        if let Err(error) = dialog.Show(None) {
            if error.code().0 == 0x800704C7u32 as i32 {
                return Ok(None);
            }
            return Err("The Windows file chooser did not complete. Saved work is kept.".into());
        }
        let item = dialog
            .GetResult()
            .map_err(|_| "The file chooser did not return a location.")?;
        let location = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| "Choose an ordinary filesystem location.")?;
        let selected = location
            .to_string()
            .map_err(|_| "The selected Windows path is not valid Unicode.".to_owned());
        CoTaskMemFree(Some(location.0.cast()));
        let selected = selected?;
        let path = Path::new(&selected);
        if !path.is_absolute() || selected.len() > 32768 {
            return Err("Choose an absolute local filesystem location.".into());
        }
        Ok(Some(path.to_owned()))
    }
}
