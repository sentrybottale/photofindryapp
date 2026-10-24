//! Windows handle-based file identity and private state. No Unix metadata is
//! synthesized. Unsupported identity/security information is an error.
//!
//! Parent directories are opened without delete sharing and retained for the
//! lifetime of each file. Every path component is inspected without traversing
//! reparse points. Private directories have a protected current-user-only DACL;
//! current-user-owned leaves may inherit those same restricted ACEs. Existing
//! ACLs are never silently repaired.
//! These mechanisms do not promise protection against another process running as
//! the same user changing an already-open file's bytes/security descriptor.

use std::{
    ffi::{OsStr, OsString, c_void},
    fs as stdfs,
    io::{self, Read, Seek, SeekFrom, Write},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, RawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr::{self, null, null_mut},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_LOCK_VIOLATION, GENERIC_READ, GENERIC_WRITE,
        HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT,
        },
        CopySid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetLengthSid,
        GetSecurityDescriptorControl, GetTokenInformation, IsValidSid, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::*,
    System::{
        IO::OVERLAPPED,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

/// Real Windows file identity. FILE_ID_128 is retained in full and in native
/// byte order. Times are signed FILETIME ticks (100 ns since 1601-01-01 UTC).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub volume: u64,
    pub file_id: [u8; 16],
    pub size: u64,
    pub created_100ns: i64,
    pub modified_100ns: i64,
    pub changed_100ns: i64,
    pub links: u32,
    pub attributes: u32,
}

impl Identity {
    /// Versioned provider identity. Never shorten this to a fictitious Unix inode.
    pub fn source_key(&self) -> String {
        let file = self
            .file_id
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        format!(
            "win32:{:016x}:{file}:{}:{}:{}",
            self.volume, self.size, self.modified_100ns, self.changed_100ns
        )
    }
    pub fn is_directory(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }
    pub fn requires_recall(&self) -> bool {
        self.attributes
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
                | FILE_ATTRIBUTE_RECALL_ON_OPEN)
            != 0
    }
}

/// An owned non-inheritable handle plus the directory handles that pin its path.
/// Read/Write/Seek are ordinary native file I/O; identity is queried explicitly.
#[derive(Debug)]
pub struct File {
    inner: stdfs::File,
    parents: Vec<stdfs::File>,
}

impl File {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        open_read_no_reparse(path.as_ref())
    }
    pub fn options() -> OpenOptions {
        OpenOptions::new()
    }
    pub fn as_std(&self) -> &stdfs::File {
        &self.inner
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata {
            identity: identity(self)?,
            inner: self.inner.metadata()?,
        })
    }
    pub fn sync_all(&self) -> io::Result<()> {
        self.inner.sync_all()
    }
    pub fn sync_data(&self) -> io::Result<()> {
        self.inner.sync_data()
    }
    pub fn set_len(&self, size: u64) -> io::Result<()> {
        self.inner.set_len(size)
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: self.inner.try_clone()?,
            parents: self
                .parents
                .iter()
                .map(stdfs::File::try_clone)
                .collect::<io::Result<_>>()?,
        })
    }
}
impl AsRawHandle for File {
    fn as_raw_handle(&self) -> RawHandle {
        self.inner.as_raw_handle()
    }
}
impl Read for File {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}
impl Read for &File {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(bytes)
    }
}
impl Write for File {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl Write for &File {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        (&self.inner).write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}
impl Seek for File {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
impl Seek for &File {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        (&self.inner).seek(from)
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn last() -> io::Error {
    io::Error::last_os_error()
}
fn bool_result(value: i32) -> io::Result<()> {
    if value == 0 { Err(last()) } else { Ok(()) }
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut out = value.encode_wide().collect::<Vec<_>>();
    if out.contains(&0) {
        return Err(invalid("A native path contains NUL."));
    }
    out.push(0);
    Ok(out)
}

/// Reject DOS devices/streams and path normalization aliases before native open.
/// The initial provider accepts local NTFS only. Device namespaces, network
/// drives, remote UNC sources and relative paths are not qualified.
fn checked_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("An absolute native filesystem path is required."));
    }
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) =>
        {
            match p.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
                _ => unreachable!(),
            }
        }
        Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)) =>
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "The initial Windows provider requires local NTFS; network sources are not qualified.",
            ));
        }
        _ => return Err(invalid("Device namespace paths are not accepted.")),
    };
    // A literal extended path avoids MAX_PATH/manifest dependence. Construct it
    // only after validating each ordinary component; never append a raw path to
    // a device namespace or let verbatim slash/traversal rules change its meaning.
    let mut normalized = PathBuf::from(format!(
        "\\\\?\\{}:\\",
        char::from(drive.to_ascii_uppercase())
    ));
    for part in path.components() {
        match part {
            Component::ParentDir | Component::CurDir => {
                return Err(invalid(
                    "Native paths must not contain traversal components.",
                ));
            }
            Component::Normal(name) => {
                let units = name.encode_wide().collect::<Vec<_>>();
                if units.is_empty()
                    || units
                        .iter()
                        .any(|c| [0, b':' as u16, b'/' as u16, b'\\' as u16].contains(c))
                    || matches!(units.last(), Some(32 | 46))
                {
                    return Err(invalid(
                        "Alternate streams and normalized path aliases are not accepted.",
                    ));
                }
                let upper = name
                    .to_string_lossy()
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if matches!(
                    upper.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                ) || ["COM", "LPT"].iter().any(|p| {
                    upper.strip_prefix(p).is_some_and(|s| {
                        matches!(
                            s,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
                }) {
                    return Err(invalid("DOS device names are not accepted as files."));
                }
                normalized.push(name);
            }
            _ => {}
        }
    }
    let root = normalized
        .ancestors()
        .find(|p| p.is_absolute() && p.parent().is_none())
        .ok_or_else(|| invalid("A native drive root is required."))?;
    let root = wide(root.as_os_str())?;
    // DRIVE_REMOVABLE=2 and DRIVE_FIXED=3 are the documented local drive types.
    if !matches!(unsafe { GetDriveTypeW(root.as_ptr()) }, 2 | 3) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Only a local fixed/removable NTFS drive is qualified.",
        ));
    }
    if wide(normalized.as_os_str())?.len() > 32_768 {
        return Err(invalid(
            "A native path exceeds the Windows extended-path bound.",
        ));
    }
    Ok(normalized)
}

fn native_open(
    path: &Path,
    access: u32,
    share: u32,
    creation: u32,
    security: *const SECURITY_ATTRIBUTES,
) -> io::Result<stdfs::File> {
    let path = wide(path.as_os_str())?;
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            security,
            creation,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_NO_RECALL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last());
    }
    // CreateFile receives no inheritable SECURITY_ATTRIBUTES and FromRawHandle
    // immediately gives exactly one owner to every successful native handle.
    Ok(unsafe { stdfs::File::from_raw_handle(handle) })
}

fn info<T>(handle: HANDLE, class: FILE_INFO_BY_HANDLE_CLASS) -> io::Result<T> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    bool_result(unsafe {
        GetFileInformationByHandleEx(
            handle,
            class,
            value.as_mut_ptr().cast(),
            u32::try_from(size_of::<T>())
                .map_err(|_| invalid("Native metadata buffer is too large."))?,
        )
    })?;
    Ok(unsafe { value.assume_init() })
}

fn identity_handle(handle: HANDLE) -> io::Result<Identity> {
    if unsafe { GetFileType(handle) } != FILE_TYPE_DISK {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "A native disk filesystem object is required.",
        ));
    }
    let mut filesystem = [0u16; 32];
    bool_result(unsafe {
        GetVolumeInformationByHandleW(
            handle,
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    })?;
    let end = filesystem
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| invalid("The native filesystem name exceeds its bound."))?;
    if &filesystem[..end] != ['N' as u16, 'T' as u16, 'F' as u16, 'S' as u16] {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "The initial Windows provider requires NTFS identity and security semantics.",
        ));
    }
    let id: FILE_ID_INFO = info(handle, FileIdInfo)?;
    let basic: FILE_BASIC_INFO = info(handle, FileBasicInfo)?;
    let standard: FILE_STANDARD_INFO = info(handle, FileStandardInfo)?;
    if basic.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(denied("Reparse points are not accepted."));
    }
    if basic.FileAttributes
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
            | FILE_ATTRIBUTE_RECALL_ON_OPEN)
        != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Cloud/offline hydration is not qualified by the initial Windows provider.",
        ));
    }
    if standard.DeletePending || standard.EndOfFile < 0 || standard.NumberOfLinks == 0 {
        return Err(denied(
            "The opened filesystem object is being removed or lacks a usable identity.",
        ));
    }
    Ok(Identity {
        volume: id.VolumeSerialNumber,
        file_id: id.FileId.Identifier,
        size: standard.EndOfFile as u64,
        created_100ns: basic.CreationTime,
        modified_100ns: basic.LastWriteTime,
        changed_100ns: basic.ChangeTime,
        links: standard.NumberOfLinks,
        attributes: basic.FileAttributes,
    })
}

pub fn identity<F: AsRawHandle + ?Sized>(file: &F) -> io::Result<Identity> {
    identity_handle(file.as_raw_handle())
}

/// Full native identity plus standard descriptive metadata from one held handle.
/// There are deliberately no Unix dev/ino/uid/mode compatibility methods.
#[derive(Clone, Debug)]
pub struct Metadata {
    identity: Identity,
    inner: stdfs::Metadata,
}
impl Metadata {
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn len(&self) -> u64 {
        self.identity.size
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn is_dir(&self) -> bool {
        self.identity.is_directory()
    }
    pub fn is_file(&self) -> bool {
        !self.is_dir() && self.inner.is_file()
    }
    pub fn file_type(&self) -> stdfs::FileType {
        self.inner.file_type()
    }
    pub fn permissions(&self) -> stdfs::Permissions {
        self.inner.permissions()
    }
    pub fn modified(&self) -> io::Result<SystemTime> {
        filetime(self.identity.modified_100ns)
    }
    pub fn created(&self) -> io::Result<SystemTime> {
        filetime(self.identity.created_100ns)
    }
    pub fn accessed(&self) -> io::Result<SystemTime> {
        self.inner.accessed()
    }
}
fn filetime(ticks: i64) -> io::Result<SystemTime> {
    const EPOCH_TICKS: i128 = 116_444_736_000_000_000;
    let delta = i128::from(ticks) - EPOCH_TICKS;
    let nanos = delta
        .unsigned_abs()
        .checked_mul(100)
        .ok_or_else(|| invalid("Native file time is out of range."))?;
    let seconds = u64::try_from(nanos / 1_000_000_000)
        .map_err(|_| invalid("Native file time is out of range."))?;
    let duration = Duration::new(seconds, (nanos % 1_000_000_000) as u32);
    (if delta >= 0 {
        UNIX_EPOCH.checked_add(duration)
    } else {
        UNIX_EPOCH.checked_sub(duration)
    })
    .ok_or_else(|| invalid("Native file time is outside the platform's SystemTime range."))
}

/// Standard named open options plus explicit privacy at creation. Existing
/// private files are validated before truncation; no chmod-like repair is used.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    private: bool,
}
impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn read(&mut self, value: bool) -> &mut Self {
        self.read = value;
        self
    }
    pub fn write(&mut self, value: bool) -> &mut Self {
        self.write = value;
        self
    }
    pub fn append(&mut self, value: bool) -> &mut Self {
        self.append = value;
        self
    }
    pub fn truncate(&mut self, value: bool) -> &mut Self {
        self.truncate = value;
        self
    }
    pub fn create(&mut self, value: bool) -> &mut Self {
        self.create = value;
        self
    }
    pub fn create_new(&mut self, value: bool) -> &mut Self {
        self.create_new = value;
        self
    }
    pub fn private(&mut self, value: bool) -> &mut Self {
        self.private = value;
        self
    }
    pub fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        if !self.read && !self.write && !self.append {
            return Err(invalid(
                "At least one native file access option is required.",
            ));
        }
        if (self.create || self.create_new || self.truncate) && !self.write && !self.append {
            return Err(invalid("Creation/truncation requires write access."));
        }
        if self.truncate && self.append && !self.create_new {
            return Err(invalid("Append and truncation cannot be combined."));
        }
        let path = checked_path(path.as_ref())?;
        let held = if self.private {
            let parent = private_dir(
                path.parent()
                    .ok_or_else(|| invalid("Private file parent is missing."))?,
                false,
            )?;
            let File { inner, mut parents } = parent;
            parents.push(inner);
            parents
        } else {
            parents(&path)?
        };
        let security = if self.private {
            Some(PrivateSecurity::new(false)?)
        } else {
            None
        };
        let attributes = security.as_ref().map(PrivateSecurity::attributes);
        let access = (if self.read { GENERIC_READ } else { 0 })
            | (if self.append {
                FILE_APPEND_DATA
            } else if self.write {
                GENERIC_WRITE
            } else {
                0
            })
            | FILE_READ_ATTRIBUTES
            | if self.private { READ_CONTROL } else { 0 };
        // CREATE_ALWAYS is incompatible with OPEN_REPARSE_POINT. Open first and
        // validate the actual handle before any requested destructive truncation.
        let disposition = if self.create_new {
            CREATE_NEW
        } else if self.create {
            OPEN_ALWAYS
        } else {
            OPEN_EXISTING
        };
        let share = if !self.write && !self.append {
            FILE_SHARE_READ
        } else {
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        };
        let inner = native_open(
            &path,
            access,
            share,
            disposition,
            attributes
                .as_ref()
                .map_or(null(), |a| a as *const SECURITY_ATTRIBUTES),
        )?;
        let current = identity_handle(inner.as_raw_handle())?;
        if current.is_directory() {
            return Err(invalid("A regular file is required."));
        }
        if let Some(security) = &security {
            verify_private(&inner, &security.user, false)?;
        }
        let file = File {
            inner,
            parents: held,
        };
        if self.truncate && !self.create_new {
            file.set_len(0)?;
        }
        Ok(file)
    }
}

pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    open_metadata_no_reparse(path.as_ref())?.metadata()
}
/// Reparse objects are refused, rather than yielding metadata usable as a source
/// identity. Callers that need to display links must use a separate platform UI.
pub fn symlink_metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    metadata(path)
}

pub struct ReadDir {
    inner: stdfs::ReadDir,
    parent: Arc<File>,
}
pub struct DirEntry {
    inner: stdfs::DirEntry,
    _parent: Arc<File>,
}
impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|entry| {
            entry.map(|inner| DirEntry {
                inner,
                _parent: Arc::clone(&self.parent),
            })
        })
    }
}
impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.inner.path()
    }
    pub fn file_name(&self) -> OsString {
        self.inner.file_name()
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        metadata(self.path())
    }
    pub fn file_type(&self) -> io::Result<stdfs::FileType> {
        Ok(self.metadata()?.file_type())
    }
}
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
    let path = checked_path(path.as_ref())?;
    let parent = open_metadata_no_reparse(&path)?;
    if !identity(&parent)?.is_directory() {
        return Err(invalid("A directory is required."));
    }
    Ok(ReadDir {
        inner: stdfs::read_dir(&path)?,
        parent: Arc::new(parent),
    })
}

#[derive(Clone, Debug, Default)]
pub struct DirBuilder {
    recursive: bool,
    private: bool,
}
impl DirBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn recursive(&mut self, value: bool) -> &mut Self {
        self.recursive = value;
        self
    }
    pub fn private(&mut self, value: bool) -> &mut Self {
        self.private = value;
        self
    }
    pub fn create(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = checked_path(path.as_ref())?;
        let mut chain = if self.recursive {
            path.ancestors()
                .filter(|p| p.is_absolute())
                .map(Path::to_owned)
                .collect::<Vec<_>>()
        } else {
            vec![path.clone()]
        };
        chain.reverse();
        for current in chain {
            if self.recursive && current != path {
                match open_metadata_no_reparse(&current) {
                    Ok(file) if identity(&file)?.is_directory() => continue,
                    Ok(_) => return Err(invalid("A recursive directory parent is a file.")),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            if self.private {
                if self.recursive {
                    drop(private_dir(&current, true)?);
                } else {
                    // DirBuilder::create is exclusive like std::fs::create_dir;
                    // private_dir(create=true) is the separate idempotent ensure.
                    let _parents = parents(&current)?;
                    let security = PrivateSecurity::new(true)?;
                    let text = wide(current.as_os_str())?;
                    bool_result(unsafe {
                        CreateDirectoryW(text.as_ptr(), &security.attributes())
                    })?;
                    drop(private_dir(&current, false)?);
                }
            } else {
                let _parents = parents(&current)?;
                let text = wide(current.as_os_str())?;
                if unsafe { CreateDirectoryW(text.as_ptr(), null()) } == 0 {
                    let error = last();
                    if !(self.recursive
                        && error.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32))
                    {
                        return Err(error);
                    }
                }
                if !metadata(&current)?.is_dir() {
                    return Err(invalid("The created native path is not a directory."));
                }
            }
        }
        Ok(())
    }
}

fn parents(path: &Path) -> io::Result<Vec<stdfs::File>> {
    let Some(parent) = path.parent() else {
        return Ok(Vec::new());
    };
    let mut chain = parent.ancestors().collect::<Vec<_>>();
    chain.reverse();
    let mut held = Vec::new();
    for parent in chain {
        // A bare drive prefix is not a directory path; the next rooted component
        // (C:\) is the actual first parent opened.
        if !parent.is_absolute() {
            continue;
        }
        // Traversal access makes no-delete sharing effective. Attribute-only
        // handles do not pin names. Permit directory writes so SQLite and native
        // child publication can create/update entries beneath the held parent.
        let file = native_open(
            parent,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
        )?;
        if !identity_handle(file.as_raw_handle())?.is_directory() {
            return Err(denied("A native path parent is not a directory."));
        }
        held.push(file);
    }
    Ok(held)
}

/// Open source pixels without following any reparse component. Writers and
/// renames are excluded while this read handle is held. Callers still recheck
/// identity after decode and before committing derived bytes.
pub fn open_read_no_reparse(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    let parents = parents(&path)?;
    let inner = native_open(&path, GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING, null())?;
    if identity_handle(inner.as_raw_handle())?.is_directory() {
        return Err(invalid("A regular file is required."));
    }
    Ok(File { inner, parents })
}

/// Directory/file identity probe with attribute access only; it does not read or
/// hydrate source image bytes. Missing native identity information fails closed.
pub fn open_metadata_no_reparse(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    let parents = parents(&path)?;
    let mut inner = native_open(
        &path,
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
    )?;
    if identity_handle(inner.as_raw_handle())?.is_directory() {
        // Attribute-only opens do not participate in Windows delete sharing.
        // Hold traversal access to actually pin the directory against rename.
        // Ordinary file attribute queries must remain compatible with SQLite's
        // concurrently-held read/write database/WAL handles.
        inner = native_open(
            &path,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
        )?;
        identity_handle(inner.as_raw_handle())?;
    }
    Ok(File { inner, parents })
}

/// Check actual selected-directory listing/traversal authority without fetching
/// a single entry or touching any original bytes. Keep the directory name pinned.
pub fn open_read_directory_no_reparse(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    let parents = parents(&path)?;
    let inner = native_open(
        &path,
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
    )?;
    if !identity_handle(inner.as_raw_handle())?.is_directory() {
        return Err(invalid("An ordinary directory is required."));
    }
    Ok(File { inner, parents })
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct UserSid {
    words: Vec<usize>,
}
impl UserSid {
    fn get() -> io::Result<Self> {
        let mut token = null_mut();
        bool_result(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
        let token = Token(token);
        let mut bytes = 0;
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut bytes);
        }
        if bytes < size_of::<TOKEN_USER>() as u32 || bytes > 65_536 {
            return Err(denied("Cannot query the current-user SID."));
        }
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        bool_result(unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            )
        })?;
        let source = unsafe { (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid };
        if unsafe { IsValidSid(source) } == 0 {
            return Err(denied("The current-user SID is invalid."));
        }
        let length = unsafe { GetLengthSid(source) };
        let mut words = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        bool_result(unsafe { CopySid(length, words.as_mut_ptr().cast(), source) })?;
        Ok(Self { words })
    }
    fn as_ptr(&self) -> PSID {
        self.words.as_ptr().cast_mut().cast()
    }
    fn text(&self) -> io::Result<String> {
        let mut value = null_mut();
        bool_result(unsafe { ConvertSidToStringSidW(self.as_ptr(), &mut value) })?;
        let allocation = LocalAllocation(value.cast());
        let mut length = 0;
        while unsafe { *value.add(length) } != 0 {
            length += 1;
            if length > 256 {
                return Err(denied("The current-user SID text exceeds its bound."));
            }
        }
        let result = String::from_utf16(unsafe { std::slice::from_raw_parts(value, length) })
            .map_err(|_| denied("The current-user SID text is invalid."))?;
        drop(allocation);
        Ok(result)
    }
}

struct PrivateSecurity {
    allocation: LocalAllocation,
    user: UserSid,
}
impl PrivateSecurity {
    fn new(directory: bool) -> io::Result<Self> {
        let user = UserSid::get()?;
        let sid = user.text()?;
        let sddl = format!(
            "O:{sid}D:P(A;{};FA;;;{sid})",
            if directory { "OICI" } else { "" }
        );
        let text = wide(OsStr::new(&sddl))?;
        let mut descriptor = null_mut();
        bool_result(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        })?;
        Ok(Self {
            allocation: LocalAllocation(descriptor),
            user,
        })
    }
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.allocation.0,
            bInheritHandle: 0,
        }
    }
}

fn verify_private(file: &stdfs::File, user: &UserSid, directory: bool) -> io::Result<()> {
    let identity = identity_handle(file.as_raw_handle())?;
    if identity.is_directory() != directory || (!directory && identity.links != 1) {
        return Err(denied(
            "Private state must have the expected object type and one file link.",
        ));
    }
    let (mut owner, mut acl, mut descriptor): (PSID, *mut ACL, PSECURITY_DESCRIPTOR) =
        (null_mut(), null_mut(), null_mut());
    let code = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let _allocation = LocalAllocation(descriptor);
    if owner.is_null() || unsafe { EqualSid(owner, user.as_ptr()) } == 0 || acl.is_null() {
        return Err(denied(
            "Private state is not owned and restricted to the current user.",
        ));
    }
    let (mut control, mut revision) = (0, 0);
    bool_result(unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) })?;
    // Every file caller already holds a validated protected private parent.
    // SQLite creates WAL/SHM children by inheriting that parent's owner-only
    // ACL; the leaf's actual ACEs remain just as restricted, even though Windows
    // does not set DACL_PROTECTED on inherited children. Directories themselves
    // must retain the protected boundary. No ACL is repaired or broadened here.
    if directory && control & SE_DACL_PROTECTED == 0 {
        return Err(denied(
            "Private state has an inheritable or unprotected DACL.",
        ));
    }
    let mut sizes: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
    bool_result(unsafe {
        GetAclInformation(
            acl,
            (&mut sizes as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    })?;
    if sizes.AceCount == 0 || sizes.AceCount > 64 {
        return Err(denied(
            "Private state has no usable bounded current-user DACL.",
        ));
    }
    for index in 0..sizes.AceCount {
        let mut ace = null_mut();
        bool_result(unsafe { GetAce(acl, index, &mut ace) })?;
        if ace.is_null() {
            return Err(denied("Private state returned a missing access-control entry."));
        }
        // ACCESS_ALLOWED_ACE_TYPE is the native numeric0. Object/callback/deny
        // ACEs are deliberately unsupported rather than loosely interpreted.
        let header = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
        if header.AceType != 0 || (header.AceSize as usize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(denied(
                "Private state has an unsupported access-control entry.",
            ));
        }
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = ptr::addr_of!(allowed.SidStart).cast_mut().cast::<c_void>();
        if unsafe { IsValidSid(sid) } == 0 || unsafe { EqualSid(sid, user.as_ptr()) } == 0 {
            return Err(denied("Private state grants access to another principal."));
        }
        let sid_length = unsafe { GetLengthSid(sid) } as usize;
        if offset_of!(ACCESS_ALLOWED_ACE, SidStart) + sid_length > allowed.Header.AceSize as usize {
            return Err(denied("Private state contains a malformed SID entry."));
        }
    }
    Ok(())
}

/// Open/create one private directory. Its parent must exist. Existing permissions
/// are checked and retained, never changed. Keep the returned handle alive while
/// using a path whose directory identity must remain pinned.
pub fn private_dir(path: &Path, create: bool) -> io::Result<File> {
    let path = checked_path(path)?;
    let parents = parents(&path)?;
    let security = PrivateSecurity::new(true)?;
    if create {
        let text = wide(path.as_os_str())?;
        if unsafe { CreateDirectoryW(text.as_ptr(), &security.attributes()) } == 0 {
            let error = last();
            if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
                return Err(error);
            }
        }
    }
    let inner = native_open(
        &path,
        FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
    )?;
    verify_private(&inner, &security.user, true)?;
    Ok(File { inner, parents })
}

fn private_file_inner(path: &Path, create_new: bool, rename: bool) -> io::Result<File> {
    let path = checked_path(path)?;
    let parent = private_dir(
        path.parent()
            .ok_or_else(|| invalid("Private file parent is missing."))?,
        false,
    )?;
    let security = PrivateSecurity::new(false)?;
    let inner = native_open(
        &path,
        GENERIC_READ | GENERIC_WRITE | READ_CONTROL | if rename { DELETE } else { 0 },
        FILE_SHARE_READ | FILE_SHARE_WRITE | if rename { FILE_SHARE_DELETE } else { 0 },
        if create_new {
            CREATE_NEW
        } else {
            OPEN_EXISTING
        },
        &security.attributes(),
    )?;
    verify_private(&inner, &security.user, false)?;
    let File {
        inner: parent,
        mut parents,
    } = parent;
    parents.push(parent);
    Ok(File { inner, parents })
}

/// true creates a new private file exclusively; false opens an existing private
/// file for read/write. This never truncates existing data or repairs unsafe ACLs.
pub fn private_file(path: &Path, create_new: bool) -> io::Result<File> {
    private_file_inner(path, create_new, false)
}

/// Verify an already-open leaf handle without changing its ACL. An inherited
/// owner-only leaf is admitted only beneath a validated protected private parent.
pub fn verify_private_file(file: &File) -> io::Result<()> {
    let path = final_path(file)?;
    let _parent = private_dir(
        path.parent()
            .ok_or_else(|| invalid("A private file parent is required."))?,
        false,
    )?;
    verify_private(&file.inner, &UserSid::get()?, false)
}

fn delete_held(file: &File) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    bool_result(unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    })
}

fn checked_publication(
    source: &Path,
    target: &Path,
    replace: bool,
) -> io::Result<(PathBuf, PathBuf, File)> {
    let source = checked_path(source)?;
    let target = checked_path(target)?;
    if source == target || source.parent() != target.parent() {
        return Err(invalid(
            "Native publication requires distinct same-directory source/target paths.",
        ));
    }
    let parent = private_dir(
        target
            .parent()
            .ok_or_else(|| invalid("A publication parent is required."))?,
        false,
    )?;
    if replace {
        match private_file(&target, false) {
            Ok(file) => drop(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    } else {
        match metadata(&target) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "A publication destination already exists.",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok((source, target, parent))
}

fn publish_held(file: &File, source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    file.sync_all()?;
    let source = wide(source.as_os_str())?;
    let target = wide(target.as_os_str())?;
    // No COPY_ALLOWED flag: this must remain a same-volume native rename.
    // WRITE_THROUGH is the documented Windows move publication mechanism;
    // this does not pretend that flushing an arbitrary directory is POSIX fsync.
    bool_result(unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_WRITE_THROUGH
                | if replace {
                    MOVEFILE_REPLACE_EXISTING
                } else {
                    0
                },
        )
    })
}

fn atomic_publish_impl(source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    let (source, target, _parent) = checked_publication(source, target, replace)?;
    let security = PrivateSecurity::new(false)?;
    let held = parents(&source)?;
    // Prevent any existing/concurrent data writer while inspecting/flushing the
    // staged file; DELETE sharing allows the owned Windows rename itself.
    let inner = native_open(
        &source,
        GENERIC_READ | GENERIC_WRITE | READ_CONTROL | DELETE,
        FILE_SHARE_READ | FILE_SHARE_DELETE,
        OPEN_EXISTING,
        null(),
    )?;
    verify_private(&inner, &security.user, false)?;
    let file = File {
        inner,
        parents: held,
    };
    publish_held(&file, &source, &target, replace)
}

/// Publish an already-closed private staged file using native same-directory
/// NTFS replacement and write-through move. All directory names stay pinned and
/// all native errors are returned. Cooperating writers must hold ExclusiveLock.
/// Hardware/power-loss persistence still depends on the storage honoring flushes;
/// actual Windows restart/fault trials remain separate qualification.
pub fn atomic_publish(source: &Path, target: &Path) -> io::Result<()> {
    atomic_publish_impl(source, target, true)
}
/// Same publication mechanism, but an existing destination is never overwritten.
pub fn atomic_publish_new(source: &Path, target: &Path) -> io::Result<()> {
    atomic_publish_impl(source, target, false)
}

/// This API deliberately refuses a POSIX directory-fsync claim on Windows.
/// Windows publication callers must use atomic_publish/atomic_publish_new.
pub fn flush_directory(_directory: &File) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Use native write-through atomic publication; POSIX directory fsync is not provided on Windows.",
    ))
}

/// Flush new bytes and publish with the native write-through replacement path.
/// Existing files are validated before any temporary file is created.
pub fn atomic_write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let path = checked_path(path)?;
    path.file_name()
        .ok_or_else(|| invalid("A private filename is required."))?;
    let parent_path = path
        .parent()
        .ok_or_else(|| invalid("A private file parent is required."))?;
    let _parent = private_dir(parent_path, false)?;
    match private_file(&path, false) {
        Ok(file) => drop(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut random = [0u8; 16];
    crate::secure_random(&mut random)?;
    let suffix = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let temporary = parent_path.join(format!(".photofindry-{suffix}.tmp"));
    let mut file = private_file_inner(&temporary, true, true)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        delete_held(&file)?;
        return Err(error);
    }
    // On a publication error retain the staged file for owned recovery rather
    // than deleting a handle whose rename may have been partially reported.
    publish_held(&file, &temporary, &path, true)
}

/// Remove only an explicitly admitted managed path, by the validated open file
/// handle. The caller supplies managed-root authority and the expected full
/// identity; this function does not decide whether arbitrary originals are cache.
pub fn remove_managed_file(root: &File, relative: &Path, expected: &Identity) -> io::Result<()> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(invalid(
            "A managed deletion requires a relative child path without traversal.",
        ));
    }
    let user = UserSid::get()?;
    verify_private(&root.inner, &user, true)?;
    let root_path = final_path(root)?;
    let target = checked_path(&root_path.join(relative))?;
    let _parent = private_dir(
        target
            .parent()
            .ok_or_else(|| invalid("A managed file parent is required."))?,
        false,
    )?;
    let held = parents(&target)?;
    let inner = native_open(
        &target,
        DELETE | FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ,
        OPEN_EXISTING,
        null(),
    )?;
    verify_private(&inner, &user, false)?;
    let file = File {
        inner,
        parents: held,
    };
    if &identity(&file)? != expected {
        return Err(denied(
            "The managed file changed before deletion; it is retained.",
        ));
    }
    delete_held(&file)
}

fn final_path(file: &File) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let needed = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            null_mut(),
            0,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if needed == 0 {
        return Err(last());
    }
    if needed > 32_768 {
        return Err(invalid("A managed root path exceeds its native bound."));
    }
    let mut buffer = vec![0u16; needed as usize + 1];
    let written = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if written == 0 {
        return Err(last());
    }
    if written >= buffer.len() as u32 {
        return Err(denied(
            "The managed root path changed while being resolved.",
        ));
    }
    checked_path(Path::new(&OsString::from_wide(&buffer[..written as usize])))
}

/// Nonblocking OS-owned lock. Its file is not unlinked on release: deleting lock
/// names creates separate lock domains. Windows also releases it on process exit.
pub struct ExclusiveLock {
    file: File,
}
impl ExclusiveLock {
    pub fn acquire(path: &Path) -> io::Result<Self> {
        let file = match private_file(path, true) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                private_file(path, false)?
            }
            Err(error) => return Err(error),
        };
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        if unsafe {
            LockFileEx(
                file.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut overlapped,
            )
        } == 0
        {
            let error = last();
            return Err(
                if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
                    io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "Private state is already locked.",
                    )
                } else {
                    error
                },
            );
        }
        Ok(Self { file })
    }
    pub fn identity(&self) -> io::Result<Identity> {
        identity(&self.file)
    }
}
impl Drop for ExclusiveLock {
    fn drop(&mut self) {
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        unsafe {
            UnlockFileEx(self.file.as_raw_handle(), 0, 1, 0, &mut overlapped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut random = [0u8; 16];
            crate::secure_random(&mut random).unwrap();
            let name = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let path = std::env::temp_dir().join(format!("photofindry-native-fs-{name}"));
            drop(private_dir(&path, true).unwrap());
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = stdfs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn native_identity_retains_full_file_id_and_real_change_time() {
        let fixture = Fixture::new();
        let path = fixture.0.join("data");
        let mut file = private_file(&path, true).unwrap();
        file.write_all(b"first").unwrap();
        file.sync_all().unwrap();
        let first = identity(&file).unwrap();
        assert_eq!(first.size, 5);
        assert_eq!(first.links, 1);
        assert!(!first.is_directory());
        assert_eq!(first, identity(&file.try_clone().unwrap()).unwrap());
        let other = private_file(&fixture.0.join("other"), true).unwrap();
        assert_ne!(first.file_id, identity(&other).unwrap().file_id);
        assert!(first.created_100ns > 0 && first.changed_100ns >= first.created_100ns);
        let key = first.source_key();
        assert!(key.starts_with("win32:"));
        assert_eq!(key.split(':').nth(2).unwrap().len(), 32);
    }
    #[test]
    fn lock_is_exclusive_and_release_does_not_unlink_identity() {
        let fixture = Fixture::new();
        let path = fixture.0.join("entry.lock");
        let lock = ExclusiveLock::acquire(&path).unwrap();
        let first = lock.identity().unwrap();
        assert_eq!(
            ExclusiveLock::acquire(&path).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(lock);
        let next = ExclusiveLock::acquire(&path).unwrap();
        assert_eq!(first.file_id, next.identity().unwrap().file_id);
    }
    #[test]
    fn atomic_replace_is_private_and_never_truncates_a_hardlinked_target() {
        let fixture = Fixture::new();
        let path = fixture.0.join("state.json");
        atomic_write_private(&path, b"old").unwrap();
        let before = identity(&open_read_no_reparse(&path).unwrap()).unwrap();
        atomic_write_private(&path, b"new data").unwrap();
        assert_eq!(stdfs::read(&path).unwrap(), b"new data");
        let after = identity(&private_file(&path, false).unwrap()).unwrap();
        assert_ne!(before.file_id, after.file_id);
        stdfs::hard_link(&path, fixture.0.join("alias")).unwrap();
        assert!(atomic_write_private(&path, b"bad").is_err());
        assert_eq!(stdfs::read(&path).unwrap(), b"new data");
        assert_eq!(stdfs::read_dir(&fixture.0).unwrap().count(), 2);
    }
    #[test]
    fn publication_new_refuses_existing_destination_and_keeps_staged_bytes() {
        let fixture = Fixture::new();
        let stage = fixture.0.join("stage");
        let target = fixture.0.join("target");
        let mut file = private_file(&stage, true).unwrap();
        file.write_all(b"new").unwrap();
        drop(file);
        let mut file = private_file(&target, true).unwrap();
        file.write_all(b"old").unwrap();
        drop(file);
        assert_eq!(
            atomic_publish_new(&stage, &target).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(stdfs::read(&stage).unwrap(), b"new");
        assert_eq!(stdfs::read(&target).unwrap(), b"old");
        let fresh = fixture.0.join("fresh");
        atomic_publish_new(&stage, &fresh).unwrap();
        assert!(!stage.exists());
        assert_eq!(stdfs::read(&fresh).unwrap(), b"new");
    }
    #[test]
    fn inherited_owner_only_sqlite_style_leaf_is_accepted_without_acl_repair() {
        let fixture = Fixture::new();
        let path = fixture.0.join("catalog.sqlite-wal");
        // SQLite's CRT creates an ordinary child that inherits the validated
        // protected parent's only-current-user ACE, without leaf PROTECTED.
        stdfs::write(&path, b"retained journal").unwrap();
        let security = || {
            let file = native_open(&path, READ_CONTROL, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, null()).unwrap();
            let mut descriptor = null_mut();
            let code = unsafe { GetSecurityInfo(file.as_raw_handle(), SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, null_mut(), null_mut(),
                null_mut(), null_mut(), &mut descriptor) };
            assert_eq!(code, 0);
            let _allocation = LocalAllocation(descriptor);
            let length = unsafe { windows_sys::Win32::Security::GetSecurityDescriptorLength(descriptor) };
            unsafe { std::slice::from_raw_parts(descriptor.cast::<u8>(), length as usize).to_vec() }
        };
        let acl_before = security();
        let before = identity(&open_read_no_reparse(&path).unwrap()).unwrap();
        let file = private_file(&path, false).unwrap();
        let after = identity(&file).unwrap();
        // NTFS may finalize ChangeTime during a write-capable reopen. Compare
        // the actual descriptor, file binding, content timestamp and bytes to
        // prove we did not repair ACLs or replace/change the retained journal.
        assert_eq!(before.volume, after.volume);
        assert_eq!(before.file_id, after.file_id);
        assert_eq!(before.size, after.size);
        assert_eq!(before.modified_100ns, after.modified_100ns);
        assert_eq!(acl_before, security());
        drop(file);
        assert_eq!(stdfs::read(&path).unwrap(), b"retained journal");
    }
    #[test]
    fn broad_leaf_acl_is_rejected_before_private_truncation() {
        let fixture = Fixture::new();
        let path = fixture.0.join("broad");
        let user = UserSid::get().unwrap();
        let sid = user.text().unwrap();
        let text = wide(OsStr::new(&format!(
            "O:{sid}D:P(A;;FA;;;WD)(A;;FA;;;{sid})"
        )))
        .unwrap();
        let mut descriptor = null_mut();
        bool_result(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        })
        .unwrap();
        let _descriptor = LocalAllocation(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let mut file = native_open(
            &path,
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            CREATE_NEW,
            &attributes,
        )
        .unwrap();
        file.write_all(b"keep original bytes").unwrap();
        drop(file);
        assert!(
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .private(true)
                .open(&path)
                .is_err()
        );
        assert_eq!(stdfs::read(&path).unwrap(), b"keep original bytes");
    }
    #[test]
    fn managed_delete_keeps_changed_and_hardlinked_files() {
        let fixture = Fixture::new();
        let root = private_dir(&fixture.0, false).unwrap();
        let path = fixture.0.join("cache");
        atomic_write_private(&path, b"old").unwrap();
        let old = identity(&open_read_no_reparse(&path).unwrap()).unwrap();
        atomic_write_private(&path, b"different").unwrap();
        assert!(remove_managed_file(&root, Path::new("cache"), &old).is_err());
        assert_eq!(stdfs::read(&path).unwrap(), b"different");
        let current = identity(&open_read_no_reparse(&path).unwrap()).unwrap();
        stdfs::hard_link(&path, fixture.0.join("original-alias")).unwrap();
        assert!(remove_managed_file(&root, Path::new("cache"), &current).is_err());
        stdfs::remove_file(fixture.0.join("original-alias")).unwrap();
        let current = identity(&open_read_no_reparse(&path).unwrap()).unwrap();
        remove_managed_file(&root, Path::new("cache"), &current).unwrap();
        assert!(!path.exists());
        assert!(remove_managed_file(&root, Path::new("../outside"), &current).is_err());
    }
    #[test]
    fn held_directory_prevents_native_rename_during_metadata_and_enumeration() {
        let fixture = Fixture::new();
        let child = fixture.0.join("child");
        drop(private_dir(&child, true).unwrap());
        let guard = open_metadata_no_reparse(&child).unwrap();
        let before = identity(&guard).unwrap();
        assert!(stdfs::rename(&child, fixture.0.join("moved")).is_err());
        assert_eq!(identity(&guard).unwrap().file_id, before.file_id);
        drop(guard);
        stdfs::rename(&child, fixture.0.join("moved")).unwrap();
    }
    #[test]
    fn normalized_aliases_and_streams_are_rejected_before_open() {
        for text in [
            r"C:\state\..\data",
            r"C:\state\data:stream",
            r"C:\state\data.",
            r"C:\state\CON.txt",
            r"\\.\PhysicalDrive0",
            r"relative\data",
        ] {
            assert!(checked_path(Path::new(text)).is_err(), "{text}");
        }
    }
    #[test]
    fn reparse_leaf_and_parent_are_rejected_when_native_symlinks_are_available() {
        let fixture = Fixture::new();
        let original = fixture.0.join("original");
        drop(private_file(&original, true).unwrap());
        let link = fixture.0.join("link");
        match std::os::windows::fs::symlink_file(&original, &link) {
            Ok(()) => assert!(open_read_no_reparse(&link).is_err()),
            Err(error) => assert_eq!(
                error.raw_os_error(),
                Some(1314),
                "Only the native symlink privilege denial may limit this test."
            ),
        }
    }
}
