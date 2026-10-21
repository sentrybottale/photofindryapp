//! Consume a fresh implicit native access bookmark in the process doing I/O.
//! This generic platform adapter does not create authority from a path, persist
//! bookmarks, choose source identity, or claim to override macOS privacy policy.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::path::{Component, Path, PathBuf};

pub const MAX_BOOKMARK_BYTES: usize = 96 * 1024;
pub const MAX_BOOKMARK_BASE64_BYTES: usize = 128 * 1024;
const MAX_PATH_BYTES: usize = 16_384;

fn path_ok(path: &Path) -> bool {
    path.is_absolute()
        && path
            .to_str()
            .is_some_and(|s| !s.contains('\0') && s.len() <= MAX_PATH_BYTES)
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
}

/// One owned native URL lifetime. Cloning an `Arc<ReaderLease>` keeps an active
/// reader's lease alive while a bounded registry evicts its inactive cache entry.
/// Debug output intentionally contains neither the path nor access capability.
pub struct ReaderLease {
    path: PathBuf,
    #[cfg(target_os = "macos")]
    url: mac::Object,
    #[cfg(target_os = "macos")]
    started: bool,
    #[cfg(target_os = "macos")]
    _owner_guard: Option<std::sync::Arc<std::fs::File>>,
}

impl std::fmt::Debug for ReaderLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReaderLease(<private native access>)")
    }
}

impl ReaderLease {
    pub fn open(reader_bookmark: &str, expected_path: &Path) -> Result<Self, String> {
        if !path_ok(expected_path)
            || reader_bookmark.is_empty()
            || reader_bookmark.len() > MAX_BOOKMARK_BASE64_BYTES
        {
            return Err("The native access grant is invalid or exceeds its bound.".into());
        }
        let bytes = STANDARD
            .decode(reader_bookmark)
            .map_err(|_| "The native access grant is invalid.")?;
        if bytes.is_empty() || bytes.len() > MAX_BOOKMARK_BYTES {
            return Err("The native access grant is invalid or exceeds its bound.".into());
        }
        #[cfg(target_os = "macos")]
        {
            mac::resolve(&bytes, expected_path)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = bytes;
            Err("Native macOS access grants are unavailable on this platform.".into())
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(target_os = "macos")]
impl Drop for ReaderLease {
    fn drop(&mut self) {
        // Resolution disables implicit automatic scope entry, so this acquired
        // scope has exactly one matching stop before URL/owner-guard release.
        if self.started {
            unsafe { mac::CFURLStopAccessingSecurityScopedResource(self.url.0) };
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{
        ffi::{c_char, c_void},
        ptr,
    };
    type Ref = *const c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: Ref);
        fn CFDataCreate(allocator: Ref, bytes: *const u8, length: isize) -> Ref;
        fn CFURLCreateByResolvingBookmarkData(
            allocator: Ref,
            bookmark: Ref,
            options: usize,
            relative: Ref,
            properties: Ref,
            stale: *mut u8,
            error: *mut Ref,
        ) -> Ref;
        fn CFURLCopyScheme(url: Ref) -> Ref;
        fn CFURLGetFileSystemRepresentation(
            url: Ref,
            resolve: u8,
            bytes: *mut u8,
            length: isize,
        ) -> u8;
        fn CFStringGetCString(value: Ref, buffer: *mut c_char, length: isize, encoding: u32) -> u8;
        fn CFURLStartAccessingSecurityScopedResource(url: Ref) -> u8;
        pub(super) fn CFURLStopAccessingSecurityScopedResource(url: Ref);
        #[cfg(test)]
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: Ref,
            bytes: *const u8,
            length: isize,
            directory: u8,
        ) -> Ref;
        #[cfg(test)]
        fn CFURLCreateBookmarkData(
            allocator: Ref,
            url: Ref,
            options: usize,
            properties: Ref,
            relative: Ref,
            error: *mut Ref,
        ) -> Ref;
        #[cfg(test)]
        fn CFDataGetLength(data: Ref) -> isize;
        #[cfg(test)]
        fn CFDataGetBytePtr(data: Ref) -> *const u8;
    }
    pub(super) struct Object(pub(super) Ref);
    // These are immutable retained CFData/CFURL/CFString values. Scope entry
    // happens once before publication; scope exit/release happens once on Drop.
    // Access policy and registry mutation are owned by the callers, not CFURL.
    unsafe impl Send for Object {}
    unsafe impl Sync for Object {}
    impl Object {
        fn take(value: Ref) -> Result<Self, String> {
            if value.is_null() {
                Err("The native access grant could not be resolved.".into())
            } else {
                Ok(Self(value))
            }
        }
    }
    impl Drop for Object {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) };
        }
    }
    fn release_error(error: Ref) {
        // Do not stringify CFError: it can contain a private path/bookmark.
        if !error.is_null() {
            unsafe { CFRelease(error) };
        }
    }
    fn owner_guard() -> Result<Option<std::sync::Arc<std::fs::File>>, String> {
        let executable = std::fs::canonicalize(
            std::env::current_exe().map_err(|_| "Cannot identify the native reader.")?,
        )
        .map_err(|_| "Cannot identify the native reader.")?;
        owner_guard_for_executable(&executable)
    }
    pub(super) fn owner_guard_for_executable(
        executable: &Path,
    ) -> Result<Option<std::sync::Arc<std::fs::File>>, String> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        use std::sync::{Arc, Mutex, OnceLock, Weak};
        static GUARDS: OnceLock<Mutex<std::collections::BTreeMap<PathBuf, Weak<std::fs::File>>>> =
            OnceLock::new();
        let Some(parent) = executable.parent() else {
            return Ok(None);
        };
        let role = match (
            parent.file_name().and_then(|s| s.to_str()),
            executable.file_name().and_then(|s| s.to_str()),
        ) {
            (Some("bin"), Some("photofindry")) | (Some("runtime"), Some("PhotoWorker")) => true,
            _ => false,
        };
        if !role {
            return Ok(None);
        }
        let Some(package) = parent
            .parent()
            .filter(|p| p.file_name().is_some_and(|s| s == "package"))
        else {
            return Ok(None);
        };
        let Some(version) = package.parent() else {
            return Ok(None);
        };
        let Some(versions) = version
            .parent()
            .filter(|p| p.file_name().is_some_and(|s| s == "versions"))
        else {
            return Ok(None);
        };
        let root = versions
            .parent()
            .ok_or("Cannot identify the retained access owner.")?;
        let owner = root.join("source-access");
        for directory in [root.to_path_buf(), owner.clone()] {
            let m = std::fs::symlink_metadata(&directory)
                .map_err(|_| "The retained access owner is unavailable.")?;
            if !m.is_dir()
                || m.file_type().is_symlink()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o022 != 0
            {
                return Err("The retained access owner has an unsafe location.".into());
            }
        }
        let path = owner.join("owner.lock");
        let before = std::fs::symlink_metadata(&path)
            .map_err(|_| "The retained access owner is unavailable.")?;
        let mut guards = GUARDS
            .get_or_init(|| Mutex::new(Default::default()))
            .lock()
            .map_err(|_| "Cannot acquire the retained access owner lease.")?;
        guards.retain(|_, guard| guard.strong_count() != 0);
        if let Some(file) = guards.get(&path).and_then(Weak::upgrade) {
            let held = file
                .metadata()
                .map_err(|_| "Cannot inspect the retained access owner.")?;
            if before.file_type().is_symlink()
                || !before.is_file()
                || before.nlink() != 1
                || before.uid() != unsafe { libc::geteuid() }
                || before.mode() & 0o077 != 0
                || before.dev() != held.dev()
                || before.ino() != held.ino()
            {
                return Err("The retained access owner changed while acquiring its lease.".into());
            }
            return Ok(Some(file));
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| "The retained access owner is unavailable.")?;
        let after = file
            .metadata()
            .map_err(|_| "Cannot inspect the retained access owner.")?;
        if !after.is_file()
            || after.nlink() != 1
            || after.uid() != unsafe { libc::geteuid() }
            || after.mode() & 0o077 != 0
            || before.dev() != after.dev()
            || before.ino() != after.ino()
        {
            return Err("The retained access owner changed while acquiring its lease.".into());
        }
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
            return Err("The retained access owner is being removed.".into());
        }
        let file = Arc::new(file);
        guards.insert(path, Arc::downgrade(&file));
        Ok(Some(file))
    }
    pub(super) fn resolve(bytes: &[u8], expected: &Path) -> Result<ReaderLease, String> {
        let guard = owner_guard()?;
        let data = Object::take(unsafe {
            CFDataCreate(ptr::null(), bytes.as_ptr(), bytes.len() as isize)
        })?;
        let mut stale = 0u8;
        let mut error = ptr::null();
        // An implicit bookmark transfers access from the native owner. Do not
        // pretend it is an app-scoped bookmark owned by this reader. Resolution
        // must not prompt UI or mount a disconnected volume in the background.
        let url = unsafe {
            CFURLCreateByResolvingBookmarkData(
                ptr::null(),
                data.0,
                (1 << 8) | (1 << 9) | (1 << 15),
                ptr::null(),
                ptr::null(),
                &mut stale,
                &mut error,
            )
        };
        release_error(error);
        let url = Object::take(url)?;
        // Foundation may mark a fresh implicit transfer stale when resolving it
        // in a different process/sandbox. This is a location refresh hint, not
        // an authority verdict. Explicit scope activation below must succeed,
        // and its resolved canonical path must match the current owner response.
        // Only the retained owner renews the persistent bookmark.
        let scheme = Object::take(unsafe { CFURLCopyScheme(url.0) })?;
        let mut scheme_bytes = [0u8; 16];
        if unsafe {
            CFStringGetCString(
                scheme.0,
                scheme_bytes.as_mut_ptr().cast(),
                scheme_bytes.len() as isize,
                0x0800_0100,
            )
        } == 0
            || &scheme_bytes[..5] != b"file\0"
        {
            return Err("The native access grant does not identify a local file URL.".into());
        }
        let started = unsafe { CFURLStartAccessingSecurityScopedResource(url.0) } != 0;
        if !started {
            return Err("macOS could not activate the native access grant.".into());
        }
        let mut lease = ReaderLease {
            path: PathBuf::new(),
            url,
            started,
            _owner_guard: guard,
        };
        let mut path = vec![0u8; MAX_PATH_BYTES + 1];
        if unsafe {
            CFURLGetFileSystemRepresentation(lease.url.0, 1, path.as_mut_ptr(), path.len() as isize)
        } == 0
        {
            return Err("The native access grant has no bounded local location.".into());
        }
        let length = path
            .iter()
            .position(|b| *b == 0)
            .ok_or("The native access grant has no bounded local location.")?;
        let resolved = PathBuf::from(
            std::str::from_utf8(&path[..length])
                .map_err(|_| "The native access location cannot be represented safely.")?,
        );
        if !path_ok(&resolved) {
            return Err("The native access location is invalid.".into());
        }
        // Compare only after consumption. Canonicalization may itself require
        // the grant. It also handles Foundation's /var vs /private/var aliases.
        let actual = std::fs::canonicalize(&resolved)
            .map_err(|_| "The granted location is currently unavailable.")?;
        let expected = std::fs::canonicalize(expected)
            .map_err(|_| "The selected location is currently unavailable.")?;
        if actual != expected {
            return Err("The native access grant identifies a different location.".into());
        }
        lease.path = actual;
        Ok(lease)
    }

    #[cfg(test)]
    pub(super) fn temporary_bookmark(path: &Path) -> String {
        use std::os::unix::ffi::OsStrExt;
        let bytes = path.as_os_str().as_bytes();
        let url = Object::take(unsafe {
            CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                bytes.as_ptr(),
                bytes.len() as isize,
                path.is_dir() as u8,
            )
        })
        .unwrap();
        let mut error = ptr::null();
        let data = unsafe {
            CFURLCreateBookmarkData(ptr::null(), url.0, 0, ptr::null(), ptr::null(), &mut error)
        };
        release_error(error);
        let data = Object::take(data).unwrap();
        let length = unsafe { CFDataGetLength(data.0) };
        assert!(length > 0 && length as usize <= MAX_BOOKMARK_BYTES);
        STANDARD.encode(unsafe {
            std::slice::from_raw_parts(CFDataGetBytePtr(data.0), length as usize)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_oversized_grants_fail_without_echoing_the_capability() {
        for value in [
            "not a bookmark".to_owned(),
            "A".repeat(MAX_BOOKMARK_BASE64_BYTES + 1),
            STANDARD.encode([0u8; 8]),
        ] {
            let error = ReaderLease::open(&value, Path::new("/private")).unwrap_err();
            assert!(!error.contains(&value));
        }
        assert!(ReaderLease::open("eA==", Path::new("relative")).is_err());
        assert!(ReaderLease::open("eA==", Path::new("/private/../etc")).is_err());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn managed_readers_share_one_descriptor_and_hold_removal_until_last_drop() {
        use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};
        let root = std::env::temp_dir().join(format!(
            "pf-owner-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let owner = root.join("source-access");
        std::fs::create_dir_all(&owner).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = owner.join("owner.lock");
        std::fs::write(&path, b"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let exe = root.join("versions/124/package/bin/photofindry");
        let first = mac::owner_guard_for_executable(&exe).unwrap().unwrap();
        let guards: Vec<_> = (0..1024)
            .map(|_| mac::owner_guard_for_executable(&exe).unwrap().unwrap())
            .collect();
        assert!(guards.iter().all(|g| std::sync::Arc::ptr_eq(g, &first)));
        let remover = std::fs::File::open(&path).unwrap();
        assert_ne!(
            unsafe { libc::flock(remover.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        drop(first);
        drop(guards);
        assert_eq!(
            unsafe { libc::flock(remover.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(mac::owner_guard_for_executable(&exe).is_err());
        drop(remover);
        let held = mac::owner_guard_for_executable(&exe).unwrap().unwrap();
        std::fs::rename(&path, owner.join("old.lock")).unwrap();
        std::fs::write(&path, b"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(mac::owner_guard_for_executable(&exe).is_err());
        drop(held);
        std::fs::remove_file(&path).unwrap();
        assert!(mac::owner_guard_for_executable(&exe).is_err());
        std::os::unix::fs::symlink(owner.join("old.lock"), &path).unwrap();
        assert!(mac::owner_guard_for_executable(&exe).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(mac::owner_guard_for_executable(&exe).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn actual_corefoundation_roundtrip_is_path_bound_and_shared_only_by_owned_leases() {
        let root = std::env::temp_dir().join(format!(
            "pf-native-access-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original.txt");
        std::fs::write(&file, b"unchanged fixture bytes").unwrap();
        let other = root.join("other");
        std::fs::create_dir(&other).unwrap();
        let bookmark = mac::temporary_bookmark(&file);
        let lease = std::sync::Arc::new(ReaderLease::open(&bookmark, &file).unwrap());
        assert_eq!(lease.path(), std::fs::canonicalize(&file).unwrap());
        assert!(!format!("{lease:?}").contains("original.txt"));
        assert!(ReaderLease::open(&bookmark, &other).is_err());
        let reader = std::sync::Arc::clone(&lease);
        drop(lease);
        assert_eq!(
            std::thread::spawn(move || {
                let bytes = std::fs::read(reader.path()).unwrap();
                drop(reader);
                bytes
            })
            .join()
            .unwrap(),
            b"unchanged fixture bytes"
        );
        std::fs::remove_dir_all(root).unwrap();
        // This exercises real CF APIs/lifetimes, not protected-folder TCC or
        // sandbox inheritance. Those require their separate native trial.
    }
}
