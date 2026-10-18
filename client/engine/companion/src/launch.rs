//! Local launch authority. The public URL contains no paths, capabilities or actions.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{fd::AsRawFd, unix::{fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt}, process::CommandExt}},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{UnixListener, UnixStream}, sync::oneshot, task::JoinHandle};

#[cfg(test)]
const LAUNCH_URL: &str = "photofindry-local://open";
pub(super) const REGISTRATION: &str = "registration.json";
const SOCKET: &str = "launch.sock";
const MAX_REGISTRATION: u64 = 16 * 1024;
const FRAME_LIMIT: usize = 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(3);
type Result<T> = std::result::Result<T, String>;

#[cfg(test)]
fn validate_launch_url(value: &str) -> Result<()> {
    if value == LAUNCH_URL { Ok(()) } else { Err("Only the PhotoFindry Open link is supported.".into()) }
}
pub fn archive_id(value:&str)->Result<String> {
    if value.len()!=36 || !value.bytes().enumerate().all(|(index,byte)| {
        if [8,13,18,23].contains(&index) {byte==b'-'} else {byte.is_ascii_hexdigit()}
    }) {return Err("Choose a complete PhotoFindry archive UUID.".into());}
    Ok(value.to_ascii_lowercase())
}

fn uid() -> u32 { unsafe { libc::geteuid() } }

/// Enter this same account's user bootstrap before starting an AppKit helper
/// or a managed companion. A GUI parent's task-specific bootstrap can disappear
/// while its detached companion is still running, causing _RegisterApplication
/// to abort before a native chooser is even created. No UID or path comes from
/// a browser request; callers supply only their fixed, validated executable.
pub fn desktop_command(executable: &Path) -> tokio::process::Command {
    let mut command=tokio::process::Command::new("/bin/launchctl");
    command.arg("asuser").arg(uid().to_string()).arg(executable);
    command
}

/// Resolve the OS account home rather than trusting a website/launch environment.
pub fn default_directory() -> Result<PathBuf> {
    let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16 * 1024];
    let status = unsafe { libc::getpwuid_r(uid(), record.as_mut_ptr(), buffer.as_mut_ptr().cast(), buffer.len(), &mut result) };
    if status != 0 || result.is_null() { return Err("Cannot locate this macOS account's home directory.".into()); }
    let record = unsafe { record.assume_init() };
    if record.pw_dir.is_null() { return Err("This macOS account has no home directory.".into()); }
    let home = unsafe { std::ffi::CStr::from_ptr(record.pw_dir) }.to_str()
        .map_err(|_| "The macOS account home path is not supported.")?;
    Ok(PathBuf::from(home).join("Library/Application Support/PhotoRecall/companion-launch"))
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Registration {
    pub(super) version: u32,
    pub data_dir: PathBuf,
    pub runtime: PathBuf,
    pub executable: PathBuf,
}
impl Registration {
    pub fn matches(&self, data: &Path, runtime: &Path, executable: &Path) -> bool {
        self.data_dir == data && self.runtime == runtime && self.executable == executable
    }
}

pub(super) fn canonical(path: &Path, directory: bool) -> Result<PathBuf> {
    if !path.is_absolute() { return Err("Local launch paths must be absolute.".into()); }
    let metadata = fs::symlink_metadata(path).map_err(|_| "A registered local launch path is unavailable.")?;
    if metadata.file_type().is_symlink() || if directory { !metadata.is_dir() } else { !metadata.is_file() } {
        return Err("Local launch paths must be ordinary files or directories.".into());
    }
    if !directory && metadata.mode() & 0o111 == 0 { return Err("The registered companion is not executable.".into()); }
    let resolved = fs::canonicalize(path).map_err(|_| "Cannot resolve a local launch path.".to_owned())?;
    for parent in resolved.ancestors() {
        let metadata = fs::symlink_metadata(parent).map_err(|_| "Cannot inspect registered path ownership.")?;
        trusted_owner(parent, &metadata)?;
    }
    Ok(resolved)
}

/// The capability-consuming opener is a fixed trusted sibling, never a path
/// supplied in a public URL or returned by the loopback server.
pub fn browser_opener(executable: &Path) -> Result<PathBuf> {
    let executable = canonical(executable, false)?;
    let parent = executable.parent().ok_or("Cannot locate the installed browser opener.")?;
    canonical(&parent.join("CompanionLauncher"), false)
}

/// macOS deliberately makes this system directory writable by administrators.
/// Trust only that exact root/admin-owned directory, never writable app contents
/// or arbitrary root-owned group-writable directories. Administrators already
/// control system application installation; ordinary accounts remain excluded.
fn system_applications_directory(path: &Path, directory: bool, owner: u32, group: u32, mode: u32) -> bool {
    cfg!(target_os = "macos") && path == Path::new("/Applications")
        && directory && owner == 0 && group == 80 && mode & 0o002 == 0
}

pub(super) fn trusted_owner(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    let trusted_shared_temporary = metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
    let applications = system_applications_directory(path, metadata.is_dir(), metadata.uid(), metadata.gid(), metadata.mode());
    if (metadata.uid() != 0 && metadata.uid() != uid())
        || (metadata.mode() & 0o022 != 0 && !trusted_shared_temporary && !applications) {
        return Err("Local launch paths must not be writable by another account.".into());
    }
    Ok(())
}

pub(super) fn private_directory(path: &Path, create: bool) -> Result<File> {
    if !path.is_absolute() { return Err("Local launch state requires an absolute directory.".into()); }
    // Parents may be ordinary shared system directories, but never symlinks.
    // The final state directory is strictly private to this OS account.
    let mut current = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::RootDir | Component::Normal(_) => current.push(component.as_os_str()),
            _ => return Err("Use a normalized local launch state directory.".into()),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => { trusted_owner(&current, &metadata)?; },
            Ok(_) => return Err("Local launch state must not traverse symlinks or special files.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().mode(0o700).create(&current).map_err(|_| "Cannot create private local launch state.")?;
            },
            Err(_) => return Err("PhotoFindry has not been registered for this account. Open its installed launcher first.".into()),
        }
    }
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_DIRECTORY)
        .open(path).map_err(|_| "Cannot open private local launch state.")?;
    let metadata = file.metadata().map_err(|_| "Cannot inspect local launch state.")?;
    if metadata.uid() != uid() || metadata.mode() & 0o077 != 0 {
        return Err("Local launch state must be owned by this account with mode 0700.".into());
    }
    Ok(file)
}

pub(super) fn same_directory(path: &Path, directory: &File) -> Result<()> {
    let current = fs::symlink_metadata(path).map_err(|_| "Local launch state disappeared.")?;
    let opened = directory.metadata().map_err(|_| "Cannot recheck local launch state.")?;
    if !current.is_dir() || current.file_type().is_symlink() || current.dev() != opened.dev() || current.ino() != opened.ino() {
        return Err("Local launch state changed during the operation.".into());
    }
    Ok(())
}

pub(super) fn private_file(path: &Path, create: bool) -> Result<File> {
    let file = OpenOptions::new().read(true).write(create).create(create).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path).map_err(|_| "Cannot open private local launch metadata.")?;
    let metadata = file.metadata().map_err(|_| "Cannot inspect private local launch metadata.")?;
    if !metadata.is_file() || metadata.uid() != uid() || metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
        return Err("Local launch metadata must be an owner-only regular file.".into());
    }
    Ok(file)
}

pub(super) fn try_lock(directory: &Path, name: &str) -> Result<Option<File>> {
    let lock = private_file(&directory.join(name), true)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { return Ok(Some(lock)); }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock { Ok(None) } else { Err("Cannot lock local launch state.".into()) }
}

fn validate_registration(record: &Registration) -> Result<()> {
    if record.version != 1 || canonical(&record.data_dir, true)? != record.data_dir
        || canonical(&record.runtime, true)? != record.runtime || canonical(&record.executable, false)? != record.executable {
        return Err("Local launch registration is incompatible or its paths changed.".into());
    }
    canonical(&record.runtime.join("PhotoWorker"), false)?;
    canonical(&record.runtime.join("PhotoEngine"), false)?;
    Ok(())
}

/// Explicit local CLI registration only. This never admits or migrates a library.
pub fn register(directory: &Path, data: &Path, runtime: &Path, executable: &Path) -> Result<Registration> {
    let owner = private_directory(directory, true)?;
    let _startup = try_lock(directory, "startup.lock")?.ok_or("Another local launch or uninstall is in progress.")?;
    ensure_available(directory)?;
    let _lock = try_lock(directory, "registration.lock")?.ok_or("Another local registration is in progress.")?;
    register_locked(directory, data, runtime, executable, &owner)
}
fn register_locked(directory: &Path, data: &Path, runtime: &Path, executable: &Path, owner: &File) -> Result<Registration> {
    let record = Registration { version: 1, data_dir: canonical(data, true)?, runtime: canonical(runtime, true)?, executable: canonical(executable, false)? };
    validate_registration(&record)?;
    super::handover::validate_admission(&record.data_dir)?;
    // Do not silently change the library behind an active broker.
    let _broker_lock = try_lock(directory, "broker.lock")?.ok_or("Stop the running registered companion before changing its registration.")?;
    let mut entropy = [0u8; 16];
    getrandom::fill(&mut entropy).map_err(|_| "Cannot prepare local launch registration.")?;
    let suffix: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = directory.join(format!(".registration-{suffix}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&temporary)
            .map_err(|_| "Cannot stage local launch registration.")?;
        serde_json::to_writer(&mut file, &record).map_err(|_| "Cannot encode local launch registration.")?;
        file.write_all(b"\n").and_then(|_| file.sync_all()).map_err(|_| "Cannot flush local launch registration.")?;
        same_directory(directory, &owner)?;
        match fs::symlink_metadata(directory.join(REGISTRATION)) {
            Ok(_) => { private_file(&directory.join(REGISTRATION), false)?; },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err("Cannot inspect previous local launch registration.".into()),
        }
        fs::rename(&temporary, directory.join(REGISTRATION)).map_err(|_| "Cannot publish local launch registration.")?;
        owner.sync_all().map_err(|_| "Cannot flush local launch state.")?;
        Ok(record)
    })();
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}

pub fn read_registration(directory: &Path) -> Result<Registration> {
    let owner = private_directory(directory, false)?;
    let file = private_file(&directory.join(REGISTRATION), false)?;
    if file.metadata().map_err(|_| "Cannot inspect local launch registration.")?.len() > MAX_REGISTRATION {
        return Err("Local launch registration is too large.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_REGISTRATION + 1).read_to_end(&mut bytes).map_err(|_| "Cannot read local launch registration.")?;
    if bytes.len() as u64 > MAX_REGISTRATION { return Err("Local launch registration is too large.".into()); }
    let record: Registration = serde_json::from_slice(&bytes).map_err(|_| "Local launch registration is invalid.")?;
    same_directory(directory, &owner)?;
    validate_registration(&record)?;
    Ok(record)
}

/// A pending uninstall is a durable launch barrier; registration cannot revive
/// the package after a crash between revocation and its recoverable Trash move.
pub fn ensure_available(directory: &Path) -> Result<()> {
    if let Some(value) = read_uninstall_receipt(directory)? {
        if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {return Err("Local uninstall recovery state is incompatible.".into());}
        if value.get("status").and_then(serde_json::Value::as_str) != Some("complete") {
            return Err("PhotoFindry uninstall is pending. Finish it using this package's local uninstall command; your library is preserved.".into());
        }
    }
    Ok(())
}
pub(super) fn read_uninstall_receipt(directory: &Path) -> Result<Option<serde_json::Value>> {
    let path = directory.join("uninstall.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot inspect local uninstall recovery state.".into()),
        Ok(_) => {},
    }
    let mut bytes = Vec::new();
    private_file(&path, false)?.take(MAX_REGISTRATION + 1).read_to_end(&mut bytes).map_err(|_| "Cannot read local uninstall recovery state.")?;
    if bytes.len() as u64 > MAX_REGISTRATION {return Err("Local uninstall recovery state is too large.".into());}
    serde_json::from_slice(&bytes).map(Some).map_err(|_| "Local uninstall recovery state is invalid.".into())
}
pub(super) fn write_uninstall_receipt(directory: &Path, value: &impl Serialize) -> Result<()> {
    let owner = private_directory(directory, false)?;
    let path = directory.join("uninstall.json");
    if path.symlink_metadata().is_ok() { private_file(&path, false)?; }
    let mut entropy = [0u8; 8]; getrandom::fill(&mut entropy).map_err(|_| "Cannot stage uninstall recovery state.")?;
    let suffix: String = entropy.iter().map(|byte|format!("{byte:02x}")).collect();
    let temporary = directory.join(format!(".uninstall-{suffix}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary).map_err(|_| "Cannot create uninstall recovery state.")?;
        serde_json::to_writer(&mut file, value).map_err(|_| "Cannot encode uninstall recovery state.")?;
        file.write_all(b"\n").and_then(|_|file.sync_all()).map_err(|_| "Cannot flush uninstall recovery state.")?;
        same_directory(directory, &owner)?;
        fs::rename(&temporary, &path).map_err(|_| "Cannot publish uninstall recovery state.")?;
        owner.sync_all().map_err(|_| "Cannot flush uninstall recovery state.".to_owned())
    })();
    if result.is_err() {let _ = fs::remove_file(temporary);}
    result
}
pub(super) struct LifecycleGuard {
    pub owner: File,
    _startup: File,
    _registration: File,
}
impl LifecycleGuard {
    pub fn acquire(directory: &Path) -> Result<Self> {
        let owner = private_directory(directory, false)?;
        let startup = try_lock(directory, "startup.lock")?.ok_or("Another local launch or uninstall is in progress.")?;
        let registration = try_lock(directory, "registration.lock")?.ok_or("Another local registration is in progress.")?;
        same_directory(directory, &owner)?;
        Ok(Self {owner, _startup:startup, _registration:registration})
    }
}
impl LifecycleGuard {
    pub fn register(&self, directory:&Path, data:&Path, runtime:&Path, executable:&Path)->Result<Registration> {
        same_directory(directory, &self.owner)?;
        register_locked(directory,data,runtime,executable,&self.owner)
    }
}
pub(super) async fn request_stop(directory: &Path) -> Result<bool> {
    request_shutdown(directory, false).await
}
pub(super) async fn request_update_stop(directory: &Path) -> Result<bool> {
    request_shutdown(directory, true).await
}
async fn request_shutdown(directory: &Path, update: bool) -> Result<bool> {
    let path = socket_path(directory)?;
    if socket_metadata(&path)?.is_none() { return Ok(false); }
    let operation = async {
        let mut stream = match UnixStream::connect(&path).await {
            Ok(stream) => stream,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => return Ok(false),
            Err(_) => return Err("Cannot connect to the registered companion for uninstall.".into()),
        };
        if !same_user(&stream) { return Err("The local launch broker belongs to another account.".into()); }
        let command=if update {b"{\"version\":1,\"operation\":\"prepare-update\"}\n".as_slice()} else {b"{\"version\":1,\"operation\":\"stop\"}\n".as_slice()};
        stream.write_all(command).await.map_err(|_| "Cannot request local shutdown.")?;
        stream.shutdown().await.map_err(|_| "Cannot finish local shutdown request.")?;
        let bytes = read_frame(&mut stream).await?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "This running companion does not support safe uninstall. Quit it, then retry.")?;
        if value != serde_json::json!({"stopping":true}) {
            if value.get("stopping") == Some(&serde_json::Value::Bool(false)) {
                return Err(value.get("error").and_then(serde_json::Value::as_str).filter(|s|s.len()<=512).unwrap_or("The registered companion is busy. Wait for its transfer to finish, then retry uninstall.").to_owned());
            }
            return Err("The registered companion did not acknowledge shutdown.".into());
        }
        Ok(true)
    };
    tokio::time::timeout(IO_TIMEOUT, operation).await.map_err(|_| "The registered companion did not respond to shutdown. Quit it, then retry.")?
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchTarget {
    pub port: u16,
    pub ticket: String,
    #[serde(default,skip_serializing_if="Option::is_none",rename="archiveId")]
    pub archive_id:Option<String>,
}
impl LaunchTarget {
    pub fn url(&self) -> Result<String> {
        if self.port == 0 || self.ticket.len() != 64 || !self.ticket.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
            return Err("The local launch response is invalid.".into());
        }
        let navigation=self.archive_id.as_ref().map(|id|archive_id(id).map(|id|format!("&archive={id}"))).transpose()?.unwrap_or_default();
        Ok(format!("http://127.0.0.1:{}/#bootstrap={}{}", self.port, self.ticket,navigation))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { version: u32, operation: String }

fn same_user(stream: &UnixStream) -> bool {
    let (mut peer_uid, mut peer_gid) = (u32::MAX, u32::MAX);
    unsafe { libc::getpeereid(stream.as_raw_fd(), &mut peer_uid, &mut peer_gid) == 0 && peer_uid == uid() }
}

async fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.take((FRAME_LIMIT + 1) as u64).read_to_end(&mut bytes).await.map_err(|_| "Cannot read the local launch request.")?;
    if bytes.is_empty() || bytes.len() > FRAME_LIMIT { return Err("The local launch message exceeds its limit.".into()); }
    Ok(bytes)
}

fn socket_path(directory: &Path) -> Result<PathBuf> {
    let path = directory.join(SOCKET);
    use std::os::unix::ffi::OsStrExt;
    if path.as_os_str().as_bytes().len() >= 104 { return Err("The private local launch directory path is too long for a macOS socket.".into()); }
    Ok(path)
}

fn socket_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() && metadata.uid() == uid() && metadata.mode() & 0o077 == 0 => Ok(Some(metadata)),
        Ok(_) => Err("An unexpected file occupies the local launch socket.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot inspect the local launch socket.".into()),
    }
}

pub struct Broker {
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl Broker {
    #[cfg(test)]
    pub fn start<F>(directory: &Path, registration: &Registration, port: u16, issue: F) -> Result<Self>
    where F: Fn() -> Result<String> + Send + Sync + 'static {
        Self::start_with_shutdown(directory, registration, port, issue, || Ok(()))
    }
    #[cfg(test)]
    pub fn start_with_shutdown<F, S>(directory: &Path, registration: &Registration, port: u16, issue: F, shutdown: S) -> Result<Self>
    where F: Fn() -> Result<String> + Send + Sync + 'static, S: Fn() -> Result<()> + Send + Sync + 'static {
        Self::start_with_shutdown_and_update(directory,registration,port,issue,shutdown,|| async {Err("This companion requires a manual quit before updating. Quit PhotoFindry, then retry.".into())})
    }
    pub fn start_with_shutdown_and_update<F,S,U,UF>(directory: &Path, registration: &Registration, port: u16, issue: F, shutdown: S, update: U) -> Result<Self>
    where F: Fn() -> Result<String> + Send + Sync + 'static, S: Fn() -> Result<()> + Send + Sync + 'static,
        U: Fn() -> UF + Send + Sync + 'static, UF: std::future::Future<Output=Result<()>> + Send + 'static {
        ensure_available(directory)?;
        if port == 0 { return Err("The local browser listener is not ready.".into()); }
        let owner = private_directory(directory, false)?;
        if read_registration(directory)? != *registration { return Err("The local launch registration changed.".into()); }
        let lock = try_lock(directory, "broker.lock")?.ok_or("The registered companion already has a launch broker.")?;
        let path = socket_path(directory)?;
        // The held ownership lock proves another valid broker cannot use this path.
        if socket_metadata(&path)?.is_some() { fs::remove_file(&path).map_err(|_| "Cannot remove a stale local launch socket.")?; }
        same_directory(directory, &owner)?;
        let listener = UnixListener::bind(&path).map_err(|_| "Cannot bind the private local launch socket.")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|_| "Cannot protect the local launch socket.")?;
        let identity = socket_metadata(&path)?.ok_or("The private local launch socket disappeared.")?;
        let (stop, mut stopped) = oneshot::channel();
        let issue = Arc::new(issue);
        let shutdown = Arc::new(shutdown);
        let update = Arc::new(update);
        let _directory = directory.to_path_buf();
        let task = tokio::spawn(async move {
            let (_owner, _lock) = (owner, lock);
            // Sequential bounded requests prevent launch storms from spawning tasks.
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    accepted = listener.accept() => {
                        let Ok((mut stream, _)) = accepted else { break; };
                        if !same_user(&stream) { continue; }
                        let operation = async {
                            let bytes = read_frame(&mut stream).await?;
                            let request: Request = serde_json::from_slice(&bytes).map_err(|_| "Invalid local launch request.".to_owned())?;
                            if request.version != 1 { return Err("Unsupported local launch request.".into()); }
                            if request.operation == "health" {
                                let bytes=b"{\"ok\":true,\"ready\":true}\n";
                                stream.write_all(bytes).await.map_err(|_|"Cannot report local readiness".to_owned())?;
                                return Ok::<(),String>(());
                            }
                            if request.operation == "stop" || request.operation == "prepare-update" {
                                // Once the same-user request is accepted, complete its
                                // owned idle/drain decision even if the socket times out.
                                let stopped = if request.operation == "prepare-update" {
                                    let update=update.clone();
                                    tokio::spawn(async move { update().await }).await
                                        .map_err(|_|"The update readiness check stopped unexpectedly.".to_owned())?
                                } else { shutdown() };
                                let result = match stopped {
                                    Ok(()) => serde_json::json!({"stopping":true}),
                                    Err(error) => serde_json::json!({"stopping":false,"error":error.chars().take(512).collect::<String>()}),
                                };
                                let bytes = serde_json::to_vec(&result).map_err(|_| "Cannot encode local shutdown response.".to_owned())?;
                                stream.write_all(&bytes).await.map_err(|_| "Cannot acknowledge local shutdown.".to_owned())?;
                                stream.shutdown().await.map_err(|_| "Cannot finish local shutdown response.".to_owned())?;
                                return Ok(());
                            }
                            if request.operation != "open" { return Err("Unsupported local launch request.".into()); }
                            ensure_available(&_directory)?;
                            // Never replay a previously delivered one-use ticket.
                            // Authority bounds outstanding tickets; native launcher
                            // coalesces URL events while its open process is active.
                            let target = LaunchTarget { port, ticket: issue()?, archive_id:None };
                            target.url()?;
                            let bytes = serde_json::to_vec(&target).map_err(|_| "Cannot encode local launch response.".to_owned())?;
                            stream.write_all(&bytes).await.map_err(|_| "Cannot deliver local launch response.".to_owned())?;
                            stream.shutdown().await.map_err(|_| "Cannot finish local launch response.".to_owned())?;
                            Ok::<(), String>(())
                        };
                        tokio::select! { _ = &mut stopped => break, _ = tokio::time::timeout(IO_TIMEOUT, operation) => {} }
                    }
                }
            }
            drop(listener);
            if socket_metadata(&path).ok().flatten().is_some_and(|metadata| metadata.dev() == identity.dev() && metadata.ino() == identity.ino()) {
                let _ = fs::remove_file(path);
            }
        });
        Ok(Self { stop: Some(stop), task: Some(task) })
    }
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        if let Some(task) = self.task.take() { let _ = task.await; }
    }
}
impl Drop for Broker {
    fn drop(&mut self) { if let Some(stop) = self.stop.take() { let _ = stop.send(()); } }
}

pub(super) async fn request_health(directory:&Path)->Result<bool> {
    let path=socket_path(directory)?;if socket_metadata(&path)?.is_none(){return Ok(false)};
    tokio::time::timeout(IO_TIMEOUT,async {
        let mut stream=match UnixStream::connect(&path).await {Ok(s)=>s,Err(error) if matches!(error.kind(),std::io::ErrorKind::NotFound|std::io::ErrorKind::ConnectionRefused)=>return Ok(false),Err(_)=>return Err("Cannot contact local companion".into())};
        stream.write_all(b"{\"version\":1,\"operation\":\"health\"}\n").await.map_err(|_|"Cannot request local readiness")?;
        stream.shutdown().await.map_err(|_|"Cannot finish local readiness request")?;
        let bytes=read_frame(&mut stream).await?;
        Ok(serde_json::from_slice::<serde_json::Value>(&bytes).ok()==Some(serde_json::json!({"ok":true,"ready":true})))
    }).await.map_err(|_|"Local readiness timed out".to_owned())?
}
async fn request_open(directory: &Path) -> Result<Option<LaunchTarget>> {
    let path = socket_path(directory)?;
    if socket_metadata(&path)?.is_none() { return Ok(None); }
    let operation = async {
        let mut stream = match UnixStream::connect(&path).await {
            Ok(stream) => stream,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => return Ok(None),
            Err(_) => return Err("Cannot connect to the private local launch broker.".into()),
        };
        if !same_user(&stream) { return Err("The local launch broker belongs to another account.".into()); }
        stream.write_all(b"{\"version\":1,\"operation\":\"open\"}\n").await.map_err(|_| "Cannot request local launch.")?;
        stream.shutdown().await.map_err(|_| "Cannot finish local launch request.")?;
        let bytes = read_frame(&mut stream).await?;
        let target: LaunchTarget = serde_json::from_slice(&bytes).map_err(|_| "Invalid local launch response.")?;
        target.url()?;
        Ok(Some(target))
    };
    tokio::time::timeout(IO_TIMEOUT, operation).await.map_err(|_| "The local launch broker did not respond in time.")?
}

async fn stop_owned_startup(child: &mut tokio::process::Child) {
    if child.try_wait().ok().flatten().is_some() { return; }
    if let Some(pid) = child.id() { unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); } }
    if tokio::time::timeout(Duration::from_secs(5), child.wait()).await.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

/// Native/CLI-only entry. Registered paths are revalidated before any process starts.
pub async fn open_registered(directory: &Path) -> Result<LaunchTarget> {
    if let Ok(record)=read_registration(directory) {
        if let Some(root)=super::lifecycle::installed_root(&record.executable) {super::lifecycle::recover(&root,false).await?;}
    }
    let _owner = private_directory(directory, false)?;
    let lock_deadline = tokio::time::Instant::now() + Duration::from_secs(65);
    let _startup = loop {
        if let Some(lock) = try_lock(directory, "startup.lock")? { break lock; }
        if tokio::time::Instant::now() >= lock_deadline { return Err("Another PhotoFindry launch is still in progress.".into()); }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    ensure_available(directory)?;
    let record = read_registration(directory)?;
    if let Some(target) = request_open(directory).await? { return Ok(target); }
    super::handover::validate_admission(&record.data_dir)?;
    if let Some(root)=super::lifecycle::installed_root(&record.executable) {
        super::lifecycle::start_registered(&root).await?;
        let deadline=tokio::time::Instant::now()+Duration::from_secs(75);
        loop {if let Some(target)=request_open(directory).await?{return Ok(target)}
            if tokio::time::Instant::now()>deadline{return Err("The background companion could not open. Run lifecycle recover; your library is kept.".into())}
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let mut command = desktop_command(&record.executable);
    command.args(["serve", "--data-dir"]).arg(&record.data_dir).arg("--runtime").arg(&record.runtime)
        .arg("--managed").arg("--launch-state").arg(directory)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command.as_std_mut().process_group(0);
    let mut child = command.spawn().map_err(|_| "Cannot start the registered local companion.")?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        match request_open(directory).await {
            Ok(Some(target)) => return Ok(target),
            Ok(None) => {},
            Err(error) => { stop_owned_startup(&mut child).await; return Err(error); },
        }
        if child.try_wait().map_err(|_| "Cannot inspect local companion startup.")?.is_some() {
            return Err("PhotoFindry could not open the registered library. Use its local launcher to review library recovery or a running older app.".into());
        }
        if tokio::time::Instant::now() >= deadline {
            stop_owned_startup(&mut child).await;
            return Err("The local library did not become ready within one minute.".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_applications_trust_is_limited_to_macos_root_admin_directory() {
        let applications = Path::new("/Applications");
        assert_eq!(system_applications_directory(applications, true, 0, 80, 0o775), cfg!(target_os = "macos"));
        for (path, directory, owner, group, mode) in [
            ("/Applications", true, 0, 80, 0o777),
            ("/Applications", true, 0, 20, 0o775),
            ("/Applications", true, 501, 80, 0o775),
            ("/Applications", false, 0, 80, 0o775),
            ("/Applications/PhotoFindry.app", true, 0, 80, 0o775),
            ("/Users/Shared/Applications", true, 0, 80, 0o775),
        ] {
            assert!(!system_applications_directory(Path::new(path), directory, owner, group, mode), "{path}");
        }
    }
    #[tokio::test]
    async fn desktop_launch_preserves_owned_pid_and_startup_cancellation() {
        use tokio::io::{AsyncBufReadExt,BufReader};
        let mut command=desktop_command(Path::new("/bin/sh"));
        command.args(["-c","printf '%s\\n' \"$$\"; exec /bin/sleep 20"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        command.as_std_mut().process_group(0);
        let mut child=command.spawn().unwrap();
        let expected=child.id().unwrap();
        let mut line=String::new();
        tokio::time::timeout(Duration::from_secs(5),BufReader::new(child.stdout.take().unwrap()).read_line(&mut line)).await.unwrap().unwrap();
        assert_eq!(line.trim().parse::<u32>().unwrap(),expected,"launchctl must leave the owned companion as the tracked child");
        stop_owned_startup(&mut child).await;
        assert!(child.try_wait().unwrap().is_some(),"Only the newly owned startup process is drained");
    }
    #[test]
    fn launch_scheme_is_a_literal_operation_without_parameters() {
        assert!(validate_launch_url(LAUNCH_URL).is_ok());
        for value in ["photofindry-local://open/", "photofindry-local://open?path=/tmp", "photofindry-local://open#ticket=x", "photofindry-local://user@open", "photofindry-local://open:42", "photofindry-local://open/../delete", "PHOTOFINDRY-LOCAL://open", "photofindry://archive/v1/test", " photofindry-local://open", "photofindry-local://%6fpen"] {
            assert!(validate_launch_url(value).is_err(), "{value}");
        }
    }
    #[test]
    fn browser_target_never_accepts_a_host_path_or_callback() {
        let good = LaunchTarget { port: 23456, ticket: "a".repeat(64), archive_id:None };
        assert_eq!(good.url().unwrap(), format!("http://127.0.0.1:23456/#bootstrap={}", "a".repeat(64)));
        for ticket in ["a".repeat(63), "A".repeat(64), "z".repeat(64), format!("{}&next=https://bad.test", "a".repeat(64))] {
            assert!(LaunchTarget { port: 23456, ticket,archive_id:None }.url().is_err());
        }
        assert!(LaunchTarget { port: 0, ticket: "a".repeat(64),archive_id:None }.url().is_err());
        assert!(serde_json::from_str::<LaunchTarget>(r#"{"port":23456,"ticket":"a","host":"bad.test"}"#).is_err());
    }
    #[test]
    fn archive_navigation_is_only_a_canonical_uuid() {
        let id="ABCDEF01-2345-6789-ABCD-EF0123456789";
        assert_eq!(archive_id(id).unwrap(),id.to_ascii_lowercase());
        for bad in ["../archive","00000000-0000-0000-0000-000000000000?attach=1","photofindry://archive/v1/00000000-0000-0000-0000-000000000000","00000000-0000-0000-0000-000000000000/","%30000000-0000-0000-0000-000000000000"] {assert!(archive_id(bad).is_err());}
        let target=LaunchTarget{port:23456,ticket:"a".repeat(64),archive_id:Some(id.into())};
        assert!(target.url().unwrap().ends_with("&archive=abcdef01-2345-6789-abcd-ef0123456789"));
        let mut invalid=target;invalid.archive_id=Some("/private/path".into());assert!(invalid.url().is_err());
    }
    #[test]
    fn socket_and_registry_reject_public_permissions_and_special_files() {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!("pf-launch-{}-{}", std::process::id(), rand_suffix()));
        private_directory(&path, true).unwrap();
        assert!(private_directory(&path, false).is_ok());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_directory(&path, false).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let socket = path.join(SOCKET);
        fs::write(&socket, b"not a socket").unwrap();
        assert!(socket_metadata(&socket).is_err());
        fs::remove_file(&socket).unwrap();
        std::os::unix::fs::symlink("/tmp", &socket).unwrap();
        assert!(socket_metadata(&socket).is_err());
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn locks_serialize_startup_and_never_follow_links() {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!("pf-launch-{}-{}", std::process::id(), rand_suffix()));
        private_directory(&path, true).unwrap();
        let first = try_lock(&path, "startup.lock").unwrap().unwrap();
        assert!(try_lock(&path, "startup.lock").unwrap().is_none());
        drop(first);
        assert!(try_lock(&path, "startup.lock").unwrap().is_some());
        std::os::unix::fs::symlink(path.join("startup.lock"), path.join("alias.lock")).unwrap();
        assert!(try_lock(&path, "alias.lock").is_err());
        fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn frame_reader_rejects_oversize_and_same_user_is_checked() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        assert!(same_user(&a));
        let writer = tokio::spawn(async move { a.write_all(&vec![b'x'; FRAME_LIMIT + 1]).await.unwrap(); a.shutdown().await.unwrap(); });
        assert!(read_frame(&mut b).await.is_err());
        writer.await.unwrap();
    }
    struct Fixture { root: PathBuf, state: PathBuf, record: Registration }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!("pfl-{}", rand_suffix()));
            private_directory(&root, true).unwrap();
            let state = root.join("state"); private_directory(&state, true).unwrap();
            let data_dir = root.join("library"); fs::create_dir(&data_dir).unwrap();
            let runtime = root.join("runtime"); fs::create_dir(&runtime).unwrap();
            let executable = root.join("photofindry");
            for path in [&executable, &runtime.join("PhotoWorker"), &runtime.join("PhotoEngine")] {
                fs::write(path, b"fixture executable").unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let record = Registration { version: 1, data_dir, runtime, executable };
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(state.join(REGISTRATION)).unwrap();
            serde_json::to_writer(&mut file, &record).unwrap();
            Self { root, state, record }
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }
    #[tokio::test]
    async fn update_request_is_same_user_private_and_deferred_without_changing_launch_authority() {
        let fixture=Fixture::new();
        let allowed=Arc::new(std::sync::atomic::AtomicBool::new(false));let ready=allowed.clone();
        let broker=Broker::start_with_shutdown_and_update(&fixture.state,&fixture.record,23456,||Ok("a".repeat(64)),||Ok(()),move||{
            let ready=ready.clone();async move {if ready.load(std::sync::atomic::Ordering::Acquire){Ok(())}else{Err("Pause active work in Jobs, then retry the update.".into())}}
        }).unwrap();
        let error=request_update_stop(&fixture.state).await.unwrap_err();assert!(error.contains("Pause active work"));
        assert!(request_open(&fixture.state).await.unwrap().is_some(),"A deferred update leaves normal launch authority usable");
        allowed.store(true,std::sync::atomic::Ordering::Release);
        assert!(request_update_stop(&fixture.state).await.unwrap());broker.stop().await;
    }
    #[tokio::test]
    async fn broker_roundtrip_is_bounded_nonreplaying_and_checks_registration() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fixture = Fixture::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let broker = Broker::start(&fixture.state, &fixture.record, 23456, move || {
            Ok(format!("{:064x}", observed.fetch_add(1, Ordering::SeqCst) + 1))
        }).unwrap();
        assert!(Broker::start(&fixture.state, &fixture.record, 23456, || Ok("a".repeat(64))).is_err());
        let first = request_open(&fixture.state).await.unwrap().unwrap();
        let second = request_open(&fixture.state).await.unwrap().unwrap();
        assert_eq!(first.port, 23456);
        assert_ne!(first.ticket, second.ticket);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(request_health(&fixture.state).await.unwrap());
        assert_eq!(fs::symlink_metadata(fixture.state.join(SOCKET)).unwrap().mode() & 0o777, 0o600);
        broker.stop().await;
        assert!(!fixture.state.join(SOCKET).exists());
        let mut wrong = fixture.record.clone(); wrong.data_dir = fixture.root.clone();
        assert!(Broker::start(&fixture.state, &wrong, 23456, || Ok("a".repeat(64))).is_err());
    }
    #[tokio::test]
    async fn broker_rejects_unknown_commands_fields_and_oversize_without_issuing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fixture = Fixture::new();
        let calls = Arc::new(AtomicUsize::new(0)); let observed = calls.clone();
        let broker = Broker::start(&fixture.state, &fixture.record, 23456, move || {
            observed.fetch_add(1, Ordering::SeqCst); Ok("a".repeat(64))
        }).unwrap();
        for bytes in [br#"{"version":1,"operation":"delete"}"#.to_vec(), br#"{"version":1,"operation":"open","path":"/tmp"}"#.to_vec(), br#"{"version":2,"operation":"open"}"#.to_vec(), vec![b'x'; 1025]] {
            let mut stream = UnixStream::connect(fixture.state.join(SOCKET)).await.unwrap();
            stream.write_all(&bytes).await.unwrap(); stream.shutdown().await.unwrap();
            let mut response = Vec::new(); stream.read_to_end(&mut response).await.unwrap();
            assert!(response.is_empty());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        broker.stop().await;
    }
    #[tokio::test]
    async fn stale_socket_cleanup_preserves_unexpected_files() {
        let fixture = Fixture::new();
        let path = fixture.state.join(SOCKET);
        let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap(); drop(stale);
        let broker = Broker::start(&fixture.state, &fixture.record, 23456, || Ok("a".repeat(64))).unwrap();
        assert!(request_open(&fixture.state).await.unwrap().is_some());
        broker.stop().await;
        fs::write(&path, b"preserve me").unwrap();
        assert!(Broker::start(&fixture.state, &fixture.record, 23456, || Ok("a".repeat(64))).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserve me");
    }
    #[test]
    fn registration_is_bounded_strict_and_rejects_other_account_writable_ancestors() {
        let fixture = Fixture::new();
        assert_eq!(read_registration(&fixture.state).unwrap(), fixture.record);
        // Registry inspection itself must not open the library or require its
        // directory lock: a running worker already owns that lock.
        let mut value = serde_json::to_value(&fixture.record).unwrap();
        value["callback"] = serde_json::json!("https://example.test");
        fs::write(fixture.state.join(REGISTRATION), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(read_registration(&fixture.state).is_err());
        fs::write(fixture.state.join(REGISTRATION), vec![b' '; MAX_REGISTRATION as usize + 1]).unwrap();
        assert!(read_registration(&fixture.state).is_err());
        fs::write(fixture.state.join(REGISTRATION), serde_json::to_vec(&fixture.record).unwrap()).unwrap();
        fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(read_registration(&fixture.state).is_err());
        fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&fixture.record.executable, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(read_registration(&fixture.state).is_err());
    }
    #[test]
    fn browser_opener_is_a_trusted_fixed_sibling() {
        let fixture = Fixture::new();
        let opener = fixture.root.join("CompanionLauncher");
        fs::write(&opener, b"fixture helper").unwrap();
        fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(browser_opener(&fixture.record.executable).unwrap(), opener);
        fs::set_permissions(&opener, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(browser_opener(&fixture.record.executable).is_err());
        fs::remove_file(&opener).unwrap();
        std::os::unix::fs::symlink(&fixture.record.executable, &opener).unwrap();
        assert!(browser_opener(&fixture.record.executable).is_err());
    }
    fn rand_suffix() -> String {
        let mut bytes = [0u8; 8]; getrandom::fill(&mut bytes).unwrap(); bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
