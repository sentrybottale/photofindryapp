//! Per-user Linux development entry. Readiness contains no authentication key;
//! a private Unix socket mints one-use bootstrap tickets for the current owner.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::oneshot,
    task::JoinHandle,
};
const READY: &str = "ready.json";
const SOCKET: &str = "open.sock";
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Info {
    version: u8,
    pid: u32,
    port: u16,
    instance_id: String,
    library_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    action: String,
    instance_id: String,
}
fn private_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Use an absolute Linux launch-state directory.".into());
    }
    if !path.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| "Create the launch-state parent directory first.")?;
    }
    let meta =
        fs::symlink_metadata(path).map_err(|_| "The launch-state directory is unavailable.")?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        return Err(
            "The launch-state directory must be an ordinary owned private directory (0700).".into(),
        );
    }
    let path = fs::canonicalize(path).map_err(|_| "Cannot resolve the launch-state directory.")?;
    if path.join(SOCKET).as_os_str().len() > 100 {
        return Err("The Linux launch-state path is too long for its private socket. Choose a shorter private path.".into());
    }
    Ok(path)
}
fn private_file(path: &Path) -> Result<File, String> {
    let file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC).open(path).map_err(|_|"The Linux companion has no usable readiness record. Start the private package first.")?;
    let m = file
        .metadata()
        .map_err(|_| "Cannot inspect Linux readiness.")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || m.len() > 16384
    {
        return Err("The Linux readiness record is not a private ordinary file.".into());
    }
    Ok(file)
}
fn read_info(path: &Path) -> Result<Info, String> {
    let mut bytes = Vec::new();
    private_file(&path.join(READY))?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read Linux readiness.")?;
    let info: Info = serde_json::from_slice(&bytes)
        .map_err(|_| "The Linux readiness record is invalid. Restart the private package.")?;
    if info.version != 1
        || info.pid == 0
        || info.pid > i32::MAX as u32
        || info.port == 0
        || info.instance_id.len() != 64
        || !info.instance_id.bytes().all(|b| b.is_ascii_hexdigit())
        || info.library_id.is_empty()
        || info.library_id.len() > 128
    {
        return Err("The Linux readiness record has an unsupported identity.".into());
    }
    Ok(info)
}
fn lock(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path.join(".entry.lock"))
        .map_err(|_| "Cannot acquire the Linux entry lock.")?;
    let m = file
        .metadata()
        .map_err(|_| "Cannot inspect the Linux entry lock.")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err(
            "Another companion owns this launch-state directory, or its lock is invalid.".into(),
        );
    }
    Ok(file)
}
fn write_info(path: &Path, info: &Info) -> Result<(), String> {
    if fs::symlink_metadata(path.join(READY)).is_ok() {
        private_file(&path.join(READY))?;
    }
    let mut bytes = serde_json::to_vec(info).map_err(|_| "Cannot record Linux readiness.")?;
    bytes.push(b'\n');
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|_| "The random source is unavailable.")?;
    let suffix: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
    let temporary = path.join(format!(".ready-{suffix}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)
        .map_err(|_| "Cannot prepare Linux readiness.")?;
    let result = (|| {
        file.write_all(&bytes)
            .map_err(|_| "Cannot write Linux readiness.")?;
        file.sync_all()
            .map_err(|_| "Cannot flush Linux readiness.")?;
        fs::rename(&temporary, path.join(READY)).map_err(|_| "Cannot publish Linux readiness.")?;
        File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Cannot flush the launch-state directory.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
async fn frame(stream: &mut UnixStream) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| "The local entry disconnected.")?;
        if n == 0 {
            return Err("The local entry response was incomplete.".into());
        }
        if bytes.len() + n > 4096 {
            return Err("The local entry frame exceeded its limit.".into());
        }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
            if end + 1 != bytes.len() {
                return Err("Invalid local entry frame.".into());
            }
            bytes.pop();
            return Ok(bytes);
        }
    }
}
fn same_user(stream: &UnixStream) -> bool {
    stream
        .peer_cred()
        .is_ok_and(|c| c.uid() == unsafe { libc::geteuid() })
}
struct Files {
    path: PathBuf,
    info: Info,
    socket_identity: (u64, u64),
    _lease: File,
}
impl Drop for Files {
    fn drop(&mut self) {
        if read_info(&self.path)
            .is_ok_and(|i| i.instance_id == self.info.instance_id && i.pid == self.info.pid)
        {
            let _ = fs::remove_file(self.path.join(READY));
        }
        if fs::symlink_metadata(self.path.join(SOCKET))
            .is_ok_and(|m| (m.dev(), m.ino()) == self.socket_identity)
        {
            let _ = fs::remove_file(self.path.join(SOCKET));
        }
    }
}
pub struct Entry {
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}
impl Entry {
    pub async fn start(
        directory: &Path,
        port: u16,
        instance: &str,
        library: &str,
        issue: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
    ) -> Result<Self, String> {
        let path = private_directory(directory)?;
        let lease = lock(&path)?;
        let socket = path.join(SOCKET);
        if let Ok(m) = fs::symlink_metadata(&socket) {
            if !m.file_type().is_socket() || m.uid() != unsafe { libc::geteuid() } {
                return Err(
                    "An unexpected entry occupies the Linux launch socket. It was kept.".into(),
                );
            }
            fs::remove_file(&socket)
                .map_err(|_| "Cannot replace the stopped Linux entry socket.")?;
        }
        let listener = UnixListener::bind(&socket)
            .map_err(|_| "Cannot bind the private Linux entry socket.")?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot protect the Linux entry socket.")?;
        let m =
            fs::symlink_metadata(&socket).map_err(|_| "Cannot inspect the Linux entry socket.")?;
        let info = Info {
            version: 1,
            pid: std::process::id(),
            port,
            instance_id: instance.into(),
            library_id: library.into(),
        };
        let files = Files {
            path,
            info,
            socket_identity: (m.dev(), m.ino()),
            _lease: lease,
        };
        write_info(&files.path, &files.info)?;
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _files = files;
            loop {
                let accepted =
                    tokio::select! {_= &mut stopped=>break,result=listener.accept()=>result};
                let Ok((mut stream, _)) = accepted else {
                    break;
                };
                if !same_user(&stream) {
                    continue;
                }
                let task = async {
                    let bytes = frame(&mut stream).await?;
                    let request: Request = serde_json::from_slice(&bytes)
                        .map_err(|_| "Invalid Linux entry request.")?;
                    if !["open", "status"].contains(&request.action.as_str()) || request.instance_id != _files.info.instance_id {
                        return Err("The local entry changed. Try opening it again.".to_owned());
                    }
                    // status proves the entry answers without minting a browser ticket.
                    let result = if request.action == "status" {
                        json!({"ready":true,"pid":_files.info.pid,"instanceId":_files.info.instance_id,"libraryId":_files.info.library_id})
                    } else { match issue() {
                        Ok(ticket)
                            if ticket.len() == 64
                                && ticket.bytes().all(|b| b.is_ascii_hexdigit()) =>
                        {
                            json!({"url":format!("http://127.0.0.1:{}/#bootstrap={ticket}",_files.info.port),"pid":_files.info.pid,"instanceId":_files.info.instance_id,"libraryId":_files.info.library_id})
                        }
                        Ok(_) => json!({"error":"The local bootstrap ticket was invalid."}),
                        Err(e) => json!({"error":e}),
                    } };
                    let mut bytes = serde_json::to_vec(&result)
                        .map_err(|_| "Cannot encode the local entry reply.")?;
                    bytes.push(b'\n');
                    stream
                        .write_all(&bytes)
                        .await
                        .map_err(|_| "The local browser request disconnected.")?;
                    Ok::<(), String>(())
                };
                let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
            }
        });
        Ok(Self {
            stop: Some(stop),
            task: Some(task),
        })
    }
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(6), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}
impl Drop for Entry {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
pub async fn open(directory: &Path) -> Result<Value, String> {
    request(directory, "open").await
}
/// Ticket-free readiness check for lifecycle health and status.
pub async fn status(directory: &Path) -> Result<Value, String> {
    request(directory, "status").await
}
async fn request(directory: &Path, action: &str) -> Result<Value, String> {
    let path = private_directory(directory)?;
    let info = read_info(&path)?;
    if unsafe { libc::kill(info.pid as i32, 0) } != 0 {
        return Err("The recorded Linux companion is no longer running. Start the private package again; saved work is kept.".into());
    }
    let meta = fs::symlink_metadata(path.join(SOCKET))
        .map_err(|_| "The Linux entry socket is unavailable. Restart the private package.")?;
    if !meta.file_type().is_socket()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        return Err("The Linux entry socket is not private.".into());
    }
    let task = async {
        let mut stream = UnixStream::connect(path.join(SOCKET))
            .await
            .map_err(|_| "The Linux companion cannot be reached. Restart the private package.")?;
        if !same_user(&stream) {
            return Err("The Linux entry is not owned by this user.".to_owned());
        }
        #[cfg(target_os = "linux")]
        if stream
            .peer_cred()
            .ok()
            .and_then(|c| c.pid())
            .map(|pid| pid as u32)
            != Some(info.pid)
        {
            return Err("The Linux entry process changed. Restart the private package.".into());
        }
        let mut bytes = serde_json::to_vec(&json!({"action":action,"instanceId":info.instance_id}))
            .map_err(|_| "Cannot create the local open request.")?;
        bytes.push(b'\n');
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| "The local entry disconnected.")?;
        let response: Value = serde_json::from_slice(&frame(&mut stream).await?)
            .map_err(|_| "The Linux entry returned an invalid response.")?;
        if let Some(e) = response["error"].as_str() {
            return Err(e.chars().take(1024).collect());
        }
        if response["pid"] != info.pid
            || response["instanceId"] != info.instance_id
            || response["libraryId"] != info.library_id
        {
            return Err("The Linux library entry changed. Open the private package again.".into());
        }
        if action == "status" {
            return Ok(response);
        }
        let prefix = format!("http://127.0.0.1:{}/#bootstrap=", info.port);
        let ticket = response["url"]
            .as_str()
            .and_then(|url| url.strip_prefix(&prefix))
            .ok_or("The Linux entry returned an unexpected browser origin.")?;
        if ticket.len() != 64 || !ticket.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("The Linux entry returned an invalid bootstrap ticket.".into());
        }
        Ok(response)
    };
    tokio::time::timeout(Duration::from_secs(6), task)
        .await
        .map_err(|_| "The Linux entry took too long. Its current library is kept.")?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).unwrap();
        let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        std::env::temp_dir().join(format!("pfl-{name}-{id}"))
    }
    #[tokio::test]
    async fn one_use_authority_tickets_require_private_current_entry() {
        let root = fixture("entry");
        let authority = Arc::new(std::sync::Mutex::new(
            super::super::auth::Authority::new().unwrap(),
        ));
        let instance = authority.lock().unwrap().instance_id().to_owned();
        let mint = authority.clone();
        let entry = Entry::start(
            &root,
            8123,
            &instance,
            "test-library",
            Arc::new(move || {
                mint.lock()
                    .unwrap()
                    .issue_bootstrap(std::time::Instant::now())
                    .map_err(str::to_owned)
            }),
        )
        .await
        .unwrap();
        let record = fs::read_to_string(root.join(READY)).unwrap();
        assert!(!record.contains("ticket"));
        assert!(!record.contains("token"));
        assert_eq!(
            fs::metadata(root.join(READY)).unwrap().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
        let response = open(&root).await.unwrap();
        let ticket = response["url"]
            .as_str()
            .unwrap()
            .split("#bootstrap=")
            .nth(1)
            .unwrap();
        let mut a = authority.lock().unwrap();
        let token = a
            .exchange_bootstrap(ticket, std::time::Instant::now())
            .unwrap();
        assert!(a.authorized(&token, std::time::Instant::now()));
        assert!(
            a.exchange_bootstrap(ticket, std::time::Instant::now())
                .is_err()
        );
        drop(a);
        assert!(
            Entry::start(
                &root,
                8124,
                &instance,
                "second-library",
                Arc::new(|| Ok("a".repeat(64)))
            )
            .await
            .is_err()
        );
        entry.stop().await;
        assert!(!root.join(READY).exists());
        assert!(!root.join(SOCKET).exists());
        assert!(open(&root).await.is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn shared_or_linked_readiness_and_wrong_instance_are_rejected() {
        let root = fixture("bad");
        let instance = "a".repeat(64);
        let entry = Entry::start(
            &root,
            8123,
            &instance,
            "library",
            Arc::new(|| Ok("b".repeat(64))),
        )
        .await
        .unwrap();
        fs::set_permissions(root.join(READY), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(open(&root).await.is_err());
        fs::set_permissions(root.join(READY), fs::Permissions::from_mode(0o600)).unwrap();
        let mut info = read_info(&root).unwrap();
        info.instance_id = "c".repeat(64);
        write_info(&root, &info).unwrap();
        assert!(open(&root).await.is_err());
        write_info(
            &root,
            &Info {
                instance_id: instance,
                ..info
            },
        )
        .unwrap();
        let retained = fs::read(root.join(READY)).unwrap();
        let target = root.join("retained-record.json");
        fs::write(&target, &retained).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(root.join(READY)).unwrap();
        std::os::unix::fs::symlink(&target, root.join(READY)).unwrap();
        assert!(open(&root).await.is_err());
        entry.stop().await;
        assert_eq!(fs::read(&target).unwrap(), retained);
        assert!(
            fs::symlink_metadata(root.join(READY))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
