//! Small native operating-system mechanisms shared by the bridge and helpers.
//! This crate contains no model policy, saved-photo schema or inference logic.
use std::{io, path::{Path, PathBuf}};

pub mod fs;

#[cfg(windows)]
pub mod process;

/// Bundled executables never resolve through PATH or a working directory.
pub fn executable(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    #[cfg(windows)]
    { if path.extension().is_none() { return path.with_extension("exe"); } }
    path
}

pub fn secure_random(bytes: &mut [u8]) -> io::Result<()> {
    #[cfg(unix)]
    { use std::io::Read; std::fs::File::open("/dev/urandom")?.read_exact(bytes) }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
        let count = u32::try_from(bytes.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput,"Random request exceeds its native bound."))?;
        let result = unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), count, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
        if result == 0 { Ok(()) } else { Err(io::Error::other("The operating system could not generate a secure random value.")) }
    }
}

#[cfg(windows)]
pub fn available_space(path:&Path)->io::Result<u64>{
    use std::os::windows::ffi::OsStrExt;
    let mut path=path.as_os_str().encode_wide().collect::<Vec<_>>();
    if path.contains(&0){return Err(io::Error::new(io::ErrorKind::InvalidInput,"Invalid storage path."));}path.push(0);
    let mut available=0u64;
    if unsafe{windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(path.as_ptr(),&mut available,std::ptr::null_mut(),std::ptr::null_mut())}==0{return Err(io::Error::last_os_error());}
    Ok(available)
}
