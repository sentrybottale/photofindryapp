//! Current-user Windows entry. Readiness has no session keys. A SID-only local
//! named pipe validates its real peer PID/account and mints a one-use ticket.
use photofindry_platform::fs;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    sync::oneshot,
    task::JoinHandle,
};
use windows_api::Win32::{
    Foundation::HANDLE,
    System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
};
const READY: &str = "ready.json";
#[derive(Clone, Serialize, Deserialize)]
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
fn directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Use an absolute Windows launch-state directory.".into());
    }
    fs::private_dir(path, true)
        .map_err(|e| format!("Windows launch-state is not private to this account: {e}"))?;
    std::fs::canonicalize(path).map_err(|_| "Cannot resolve launch-state storage.".into())
}
fn name(path: &Path) -> Result<String, String> {
    let sid = super::native::current_sid()?;
    let mut hash = Sha256::new();
    hash.update(sid.as_bytes());
    hash.update([0]);
    hash.update(path.as_os_str().as_encoded_bytes());
    Ok(format!("\\\\.\\pipe\\PhotoFindryDev-{:x}", hash.finalize()))
}
fn read(path: &Path) -> Result<Info, String> {
    let file = fs::private_file(&path.join(READY), false)
        .map_err(|_| "No usable private readiness record. Start the Windows package first.")?;
    let id = fs::identity(&file).map_err(|_| "Cannot inspect readiness.")?;
    if id.is_directory() || id.links != 1 || id.size > 16384 {
        return Err("Invalid private readiness file.".into());
    }
    let mut bytes = Vec::new();
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read readiness.")?;
    let info: Info = serde_json::from_slice(&bytes).map_err(|_| "Invalid readiness record.")?;
    if info.version != 1
        || info.pid == 0
        || info.port == 0
        || info.instance_id.len() != 64
        || !info.instance_id.bytes().all(|b| b.is_ascii_hexdigit())
        || info.library_id.is_empty()
        || info.library_id.len() > 128
    {
        return Err("Unsupported readiness identity.".into());
    }
    Ok(info)
}
async fn frame<T: AsyncRead + Unpin>(stream: &mut T) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 1024];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| "The local Windows entry disconnected.")?;
        if n == 0 || bytes.len() + n > 4096 {
            return Err("Incomplete or oversized local entry frame.".into());
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
struct Files {
    path: PathBuf,
    info: Info,
    _lock: fs::ExclusiveLock,
}
impl Drop for Files {
    fn drop(&mut self) {
        if read(&self.path)
            .is_ok_and(|i| i.pid == self.info.pid && i.instance_id == self.info.instance_id)
        {
            let _ = std::fs::remove_file(self.path.join(READY));
        }
    }
}
pub struct Entry {
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}
fn pipe(path: &Path, first: bool) -> Result<NamedPipeServer, String> {
    let security = super::native::PipeSecurity::new()?;
    let mut attributes = security.attributes();
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(2)
            .create_with_security_attributes_raw(
                name(path)?,
                (&mut attributes as *mut windows_api::Win32::Security::SECURITY_ATTRIBUTES).cast(),
            )
    }
    .map_err(|e| format!("Cannot bind the private Windows entry: {e}"))
}
impl Entry {
    pub async fn start(
        path: &Path,
        port: u16,
        instance: &str,
        library: &str,
        issue: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
        shutdown: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let path = directory(path)?;
        let lock = fs::ExclusiveLock::acquire(&path.join(".entry.lock"))
            .map_err(|_| "Another companion owns this Windows launch-state directory.")?;
        // Each connection gets a fresh Tokio I/O object. Reusing a disconnected
        // server retains cached EOF/readiness from the previous client.
        let server = pipe(&path, true)?;
        let files = Files {
            path,
            info: Info {
                version: 1,
                pid: std::process::id(),
                port,
                instance_id: instance.into(),
                library_id: library.into(),
            },
            _lock: lock,
        };
        let mut record = serde_json::to_vec(&files.info).map_err(|_| "Cannot encode readiness.")?;
        record.push(b'\n');
        fs::atomic_write_private(&files.path.join(READY), &record)
            .map_err(|_| "Cannot publish private readiness.")?;
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let files = files;
            let mut server = server;
            loop {
                let connected = tokio::select! {_=&mut stopped=>break,v=server.connect()=>v};
                if connected.is_err() {
                    break;
                }
                let mut pid = 0;
                let same = unsafe {
                    GetNamedPipeClientProcessId(HANDLE(server.as_raw_handle()), &mut pid).is_ok()
                } && super::native::same_user(pid);
                if same {
                    let operation = async {
                        let request: Request = serde_json::from_slice(&frame(&mut server).await?)
                            .map_err(|_| "Invalid Windows entry request.")?;
                        if request.instance_id != files.info.instance_id {
                            return Err("The Windows entry changed. Try again.".to_owned());
                        }
                        let mut result = json!({"pid":files.info.pid,"instanceId":files.info.instance_id,"libraryId":files.info.library_id,"running":true});
                        match request.action.as_str() {
                            "open" => match issue() {
                                Ok(ticket)
                                    if ticket.len() == 64
                                        && ticket.bytes().all(|b| b.is_ascii_hexdigit()) =>
                                {
                                    result["url"] = json!(format!(
                                        "http://127.0.0.1:{}/#bootstrap={ticket}",
                                        files.info.port
                                    ));
                                }
                                Ok(_) => return Err("Invalid local bootstrap ticket.".into()),
                                Err(e) => {
                                    result["error"] = json!(e);
                                }
                            },
                            "status" => {}
                            "stop" => {
                                result["stopping"] = json!(true);
                            }
                            _ => return Err("Unsupported local entry action.".into()),
                        }
                        let mut bytes = serde_json::to_vec(&result)
                            .map_err(|_| "Cannot encode local reply.")?;
                        bytes.push(b'\n');
                        server
                            .write_all(&bytes)
                            .await
                            .map_err(|_| "Local entry disconnected.")?;
                        // DisconnectNamedPipe discards unread buffered replies.
                        // Retain the connection until this exact client confirms
                        // consumption; the surrounding timeout bounds a stalled peer.
                        if frame(&mut server).await? != b"received" {
                            return Err("Invalid Windows entry acknowledgement.".into());
                        }
                        if request.action == "stop" {
                            shutdown();
                        }
                        Ok::<(), String>(())
                    };
                    let _ = tokio::time::timeout(Duration::from_secs(5), operation).await;
                }
                // Create the successor before closing this instance so the pipe
                // name never disappears between clients. Both keep the same DACL.
                let Ok(next) = pipe(&files.path, false) else { break };
                let _ = server.disconnect();
                server = next;
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
pub async fn request(path: &Path, action: &str) -> Result<Value, String> {
    if !["open", "status", "stop"].contains(&action) {
        return Err("Unsupported entry request.".into());
    }
    let path = directory(path)?;
    let info = read(&path)?;
    let operation = async {
        let pipe_name = name(&path)?;
        let mut stream = loop {
            match ClientOptions::new().security_qos_flags(0x00010000).open(&pipe_name){
                Ok(stream)=>break stream,
                Err(error)if error.raw_os_error()==Some(231)=>tokio::time::sleep(Duration::from_millis(10)).await,
                Err(_)=>return Err("The Windows companion cannot be reached. Start the private package again; saved work is kept.".to_owned()),
            }
        };
        let mut pid = 0;
        if !unsafe { GetNamedPipeServerProcessId(HANDLE(stream.as_raw_handle()), &mut pid).is_ok() }
            || pid != info.pid
            || !super::native::same_user(pid)
        {
            return Err("The Windows entry belongs to a different process/account.".into());
        }
        let mut bytes = serde_json::to_vec(&json!({"action":action,"instanceId":info.instance_id}))
            .map_err(|_| "Cannot encode local request.")?;
        bytes.push(b'\n');
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| "Windows entry disconnected.")?;
        let result: Value = serde_json::from_slice(&frame(&mut stream).await?)
            .map_err(|_| "Invalid Windows entry reply.")?;
        if result["pid"] != info.pid
            || result["instanceId"] != info.instance_id
            || result["libraryId"] != info.library_id
        {
            return Err("The Windows library entry changed.".into());
        }
        stream
            .write_all(b"received\n")
            .await
            .map_err(|_| "Windows entry disconnected before acknowledgement.")?;
        if let Some(e) = result["error"].as_str() {
            return Err(e.chars().take(1024).collect::<String>());
        }
        if action == "open" {
            let prefix = format!("http://127.0.0.1:{}/#bootstrap=", info.port);
            let ticket = result["url"]
                .as_str()
                .and_then(|v| v.strip_prefix(&prefix))
                .ok_or("Unexpected local browser origin.")?;
            if ticket.len() != 64 || !ticket.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Invalid bootstrap ticket.".into());
            }
        }
        Ok(result)
    };
    let response = tokio::time::timeout(Duration::from_secs(6), operation)
        .await
        .map_err(|_| "The Windows entry took too long. Its current library is kept.")??;
    if action == "stop" {
        let until = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if read(&path).is_err() {
                break;
            }
            if tokio::time::Instant::now() >= until {
                return Err(
                    "Windows shutdown is still draining owned work. Saved data is kept.".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    #[tokio::test]
    async fn same_account_native_pipe_binds_real_owner_and_one_use_current_authority() {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("pf-win-entry-{id}"));
        let authority = Arc::new(Mutex::new(super::super::auth::Authority::new().unwrap()));
        let instance = authority.lock().unwrap().instance_id().to_owned();
        let mint = authority.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let entry = Entry::start(
            &path,
            8123,
            &instance,
            "windows-test-library",
            Arc::new(move || {
                mint.lock()
                    .unwrap()
                    .issue_bootstrap(std::time::Instant::now())
                    .map_err(str::to_owned)
            }),
            Arc::new(move || {
                stop.store(true, Ordering::Release);
            }),
        )
        .await
        .unwrap();
        let bytes = std::fs::read_to_string(path.join(READY)).unwrap();
        assert!(!bytes.contains("token"));
        assert!(!bytes.contains("bootstrap"));
        assert!(
            Entry::start(
                &path,
                8124,
                &instance,
                "other",
                Arc::new(|| Ok("0".repeat(64))),
                Arc::new(|| {})
            )
            .await
            .is_err()
        );
        let status = request(&path, "status").await.unwrap();
        assert_eq!(status["pid"], std::process::id());
        assert!(status["url"].is_null());
        let reply = request(&path, "open").await.unwrap();
        let ticket = reply["url"]
            .as_str()
            .unwrap()
            .split_once("#bootstrap=")
            .unwrap()
            .1;
        let now = std::time::Instant::now();
        let token = authority
            .lock()
            .unwrap()
            .exchange_bootstrap(ticket, now)
            .unwrap();
        assert!(
            authority
                .lock()
                .unwrap()
                .exchange_bootstrap(ticket, now)
                .is_err()
        );
        assert!(authority.lock().unwrap().authorized(&token, now));
        let stopped_path = path.clone();
        let waiter = tokio::spawn(async move { request(&stopped_path, "stop").await });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !stopping.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // A shutdown notification alone must not erase readiness before owned work drains.
        assert!(path.join(READY).is_file());
        entry.stop().await;
        assert!(waiter.await.unwrap().unwrap()["stopping"] == true);
        assert!(!path.join(READY).exists());
        assert!(request(&path, "open").await.is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}
