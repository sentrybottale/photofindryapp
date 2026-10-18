//! Bounded multiplexed transport for the one owned native worker. No generic HTTP RPC.
use serde_json::{Value, json};
use std::os::unix::process::CommandExt;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::Hasher,
    io::Write,
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
};
const MAX_FRAME: usize = 16 * 1024 * 1024;
const MAX_PENDING: usize = 32;
struct Request {
    id: u64,
    method: &'static str,
    params: Value,
}
struct Pending {
    method: &'static str,
    created: Instant,
    reply: oneshot::Sender<Result<Value, String>>,
}
#[derive(Default)]
struct Cache {
    state: Value,
    revision: u64,
    gallery_revision: u64,
    gallery_hash: u64,
}
struct HashWriter(DefaultHasher);
impl Write for HashWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.write(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
// Cached progress is deliberately independent of result pages. Direct RPC
// replies retain their complete bounded pages; polling never repeats those rows.
fn strip_photo_pages(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, value| {
                !(["photos", "peoplePhotos"].contains(&key.as_str()) && value.is_array())
            });
            for value in map.values_mut() {
                strip_photo_pages(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_photo_pages(value);
            }
        }
        _ => {}
    }
}
impl Cache {
    fn ingest(&mut self, event: &str, mut value: Value) {
        if !self.state.is_object() {
            self.state = json!({});
        }
        if event == "state" {
            let mut hash = HashWriter(DefaultHasher::new());
            for key in ["photos", "sources", "folders", "metadataPreferences"] {
                let _ = serde_json::to_writer(&mut hash, &value[key]);
            }
            let _ = serde_json::to_writer(&mut hash, &value["people"]["assignmentRevision"]);
            for key in ["total", "tagged", "matched"] {
                let _ = serde_json::to_writer(&mut hash, &value["library"][key]);
            }
            let digest = hash.0.finish();
            if digest != self.gallery_hash {
                self.gallery_hash = digest;
                self.gallery_revision += 1;
            }
            if let Some(map) = value.as_object_mut() {
                for key in ["photos", "peoplePhotos", "nativeHost"] {
                    map.remove(key);
                }
            }
            // Independent metric/story events remain authoritative when state snapshots omit them.
            for key in ["taggingStory", "improvementStory", "aiResults", "aiStory"] {
                if value.get(key).is_none() {
                    if let Some(saved) = self.state.get(key) {
                        value[key] = saved.clone();
                    }
                }
            }
            self.state = value;
        } else {
            let key = match event {
                "jobs" => "jobs",
                "monitor" => "monitor",
                "semanticProgress" => "semanticIndex",
                "faceActivity" => "faceJob",
                "faceCoverage" => "faceCoverage",
                "taggingStory" => "taggingStory",
                "improvementStory" => "improvementStory",
                "aiResults" => "aiResults",
                _ => return,
            };
            self.state[key] = value;
        }
        strip_photo_pages(&mut self.state);
        let search = &self.state["aiSearch"];
        let latest = &self.state["aiResults"]["result"];
        let job = search
            .get("aiJobId")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| latest.get("aiJobId").cloned())
            .unwrap_or(json!("legacy-ai"));
        let art = search
            .get("aiProgressArt")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| {
                (latest["aiJobId"] == job)
                    .then(|| latest.get("aiProgressArt").cloned())
                    .flatten()
            })
            .unwrap_or(Value::Null);
        if self.state["aiStory"]["art"] != art || self.state["aiStory"]["jobID"] != job {
            let sequence = self.state["aiStory"]["sequence"].as_u64().unwrap_or(0) + 1;
            self.state["aiStory"] = json!({"jobID":job,"sequence":sequence,"art":art});
        }
        self.revision += 1;
    }
    fn snapshot(&self) -> Value {
        let mut value = if self.state.is_object() {
            self.state.clone()
        } else {
            json!({"loading":true})
        };
        value["revision"] = json!(self.revision);
        value["galleryRevision"] = json!(self.gallery_revision);
        value
    }
}
#[derive(Clone)]
struct NativeWorker {
    requests: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    pending: Arc<Mutex<HashMap<u64, Pending>>>,
    sequence: Arc<AtomicU64>,
    cache: Arc<Mutex<Cache>>,
}
async fn frame<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Result<Option<Vec<u8>>, ()> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|_| ())?;
        if available.is_empty() {
            return if line.is_empty() { Ok(None) } else { Err(()) };
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(available.len());
        if line.len() + count > MAX_FRAME {
            return Err(());
        }
        let complete = available[count - 1] == b'\n';
        line.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            return Ok(Some(line));
        }
    }
}
async fn terminate(child: &mut Child, input: &mut tokio::process::ChildStdin, group: i32) {
    let _ = tokio::time::timeout(
        Duration::from_millis(200),
        input.write_all(b"{\"id\":\"companion-shutdown\",\"method\":\"shutdown\",\"params\":{}}\n"),
    )
    .await;
    let _ = input.shutdown().await;
    if tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .is_err()
    {
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    // Every descendant inherits this freshly owned process group. A crashed worker
    // must not strand its local model server or helpers.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}
impl NativeWorker {
    pub fn spawn(
        executable: &Path,
        data: &Path,
        runtime: &Path,
    ) -> Result<(Self, tokio::task::JoinHandle<()>), String> {
        let mut command = Command::new(executable);
        command
            .args(["--native", "--data-dir"])
            .arg(data)
            .arg("--runtime")
            .arg(runtime)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("PHOTORECALL")
                || name.starts_with("DYLD_")
                || name == "NODE_OPTIONS"
            {
                command.env_remove(key);
            }
        }
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| "Cannot start the specified PhotoWorker.")?;
        let owned_group = child.id().ok_or("Worker process identity unavailable.")? as i32;
        let mut input = child.stdin.take().ok_or("Worker input is unavailable.")?;
        let stdout = child.stdout.take().ok_or("Worker output is unavailable.")?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or("Worker diagnostics are unavailable.")?;
        let diagnostics = tokio::spawn(async move {
            let mut buffer = [0u8; 8192];
            while let Ok(n) = stderr.read(&mut buffer).await {
                if n == 0 {
                    break;
                }
            }
        });
        let pending = Arc::new(Mutex::new(HashMap::<u64, Pending>::new()));
        let cache = Arc::new(Mutex::new(Cache::default()));
        let reader_pending = pending.clone();
        let reader_cache = cache.clone();
        let mut reader = tokio::spawn(async move {
            let mut output = BufReader::with_capacity(8192, stdout);
            while let Ok(Some(line)) = frame(&mut output).await {
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    break;
                };
                if let Some(event) = value["event"].as_str() {
                    reader_cache
                        .lock()
                        .unwrap()
                        .ingest(event, value["data"].clone());
                    continue;
                }
                if let Some(id) = value["id"].as_u64() {
                    let result = if let Some(result) = value.get("result") {
                        if result["settings"].is_object() && result["library"].is_object() {
                            reader_cache.lock().unwrap().ingest("state", result.clone());
                        }
                        Ok(result.clone())
                    } else {
                        let error = value["error"]
                            .as_str()
                            .unwrap_or("The local operation could not complete.");
                        Err(error.chars().take(2048).collect())
                    };
                    if let Some(request) = reader_pending.lock().unwrap().remove(&id) {
                        let _ = request.reply.send(result);
                    }
                }
            }
        });
        let (requests, mut queue) = mpsc::channel::<Request>(32);
        let (stop, mut stopped) = watch::channel(false);
        let sequence = Arc::new(AtomicU64::new(1));
        let driver_pending = pending.clone();
        let driver_sequence = sequence.clone();
        let task = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            let mut latest_search = 0;
            loop {
                let next = tokio::select! {
                 _=stopped.changed()=>break,
                 _=child.wait()=>break,
                 _=&mut reader=>break,
                 request=queue.recv()=>match request{Some(v)=>Some(v),None=>break},
                 _=tick.tick()=>{
                  let mut cancelled_search=false;
                  {let mut pending=driver_pending.lock().unwrap();let stale:Vec<u64>=pending.iter().filter_map(|(id,p)|(p.reply.is_closed()||(p.created.elapsed()>Duration::from_secs(1800)&&!["restoreLibrary","attachSourceArchive","exportSourceArchive"].contains(&p.method))).then_some(*id)).collect();for id in stale{if let Some(p)=pending.remove(&id){if id==latest_search&&p.method=="searchInteractive"{cancelled_search=true;}let _=p.reply.send(Err("The local request expired.".into()));}}}
                  if cancelled_search{Some(Request{id:driver_sequence.fetch_add(1,Ordering::Relaxed),method:"cancelSearch",params:json!({})})}else{None}
                 }
                };
                let Some(request) = next else {
                    continue;
                };
                if request.method != "cancelSearch"
                    && !driver_pending.lock().unwrap().contains_key(&request.id)
                {
                    continue;
                }
                if request.method == "searchInteractive" {
                    latest_search = request.id;
                }
                let mut encoded = serde_json::to_vec(
                    &json!({"id":request.id,"method":request.method,"params":request.params}),
                )
                .unwrap();
                if encoded.len() > MAX_FRAME {
                    if let Some(p) = driver_pending.lock().unwrap().remove(&request.id) {
                        let _ = p
                            .reply
                            .send(Err("The local request exceeds its size bound.".into()));
                    }
                    continue;
                }
                encoded.push(b'\n');
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(5), input.write_all(&encoded)).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
            terminate(&mut child, &mut input, owned_group).await;
            reader.abort();
            diagnostics.abort();
            for (_, request) in driver_pending.lock().unwrap().drain() {
                let _ = request.reply.send(Err("The local worker stopped.".into()));
            }
        });
        Ok((
            Self {
                requests,
                stop,
                pending,
                sequence,
                cache,
            },
            task,
        ))
    }
    pub async fn call(&self, method: &'static str, params: Value) -> Result<Value, String> {
        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let (reply, result) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.len() >= MAX_PENDING {
                return Err(
                    "The local worker has too many pending requests. Try again shortly.".into(),
                );
            }
            pending.insert(
                id,
                Pending {
                    method,
                    created: Instant::now(),
                    reply,
                },
            );
        }
        if self
            .requests
            .try_send(Request { id, method, params })
            .is_err()
        {
            self.pending.lock().unwrap().remove(&id);
            return Err("The local worker is busy or unavailable.".into());
        }
        result
            .await
            .map_err(|_| "The local worker is unavailable.".to_owned())?
    }
    pub fn snapshot(&self) -> Value {
        self.cache.lock().unwrap().snapshot()
    }
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}
/// Stable handle across coordinated native restore/attach restarts.
#[derive(Clone)]
pub struct Worker {
    current: Arc<Mutex<NativeWorker>>,
    restart: mpsc::Sender<oneshot::Sender<Result<(), String>>>,
    stop: watch::Sender<bool>,
    generation: Arc<AtomicU64>,
}
impl Worker {
    pub fn spawn(
        executable: &Path,
        data: &Path,
        runtime: &Path,
    ) -> Result<(Self, tokio::task::JoinHandle<()>), String> {
        let (native, mut running) = NativeWorker::spawn(executable, data, runtime)?;
        let current = Arc::new(Mutex::new(native));
        let driver = current.clone();
        let (restart, mut restarting) = mpsc::channel::<oneshot::Sender<Result<(), String>>>(1);
        let (stop, mut stopped) = watch::channel(false);
        let generation = Arc::new(AtomicU64::new(0));
        let epoch = generation.clone();
        let executable = executable.to_owned();
        let data = data.to_owned();
        let runtime = runtime.to_owned();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                 _=stopped.changed()=>{driver.lock().unwrap().stop();let _=running.await;break;},
                 _=&mut running=>break,
                 request=restarting.recv()=>{let Some(reply)=request else{driver.lock().unwrap().stop();let _=running.await;break;};driver.lock().unwrap().stop();let _=(&mut running).await;
                  match NativeWorker::spawn(&executable,&data,&runtime){Ok((native,next))=>{{let mut current=driver.lock().unwrap();*current=native;epoch.fetch_add(1,Ordering::AcqRel);}running=next;let _=reply.send(Ok(()));},Err(error)=>{let _=reply.send(Err(error));break;}}
                 }
                }
            }
        });
        Ok((
            Self {
                current,
                restart,
                stop,
                generation,
            },
            task,
        ))
    }
    pub async fn call(&self, method: &'static str, params: Value) -> Result<Value, String> {
        let native = self.current.lock().unwrap().clone();
        native.call(method, params).await
    }
    pub fn snapshot(&self) -> Value {
        let current = self.current.lock().unwrap();
        let mut value = current.snapshot();
        let generation = self.generation.load(Ordering::Acquire);
        value["workerGeneration"] = json!(generation);
        let epoch = generation * 1_000_000;
        for key in ["revision", "galleryRevision"] {
            value[key] = json!(epoch + value[key].as_u64().unwrap_or(0));
        }
        value
    }
    pub async fn restart(&self) -> Result<(), String> {
        let (reply, result) = oneshot::channel();
        self.restart
            .try_send(reply)
            .map_err(|_| "The worker is already restarting.".to_owned())?;
        result
            .await
            .map_err(|_| "The local worker could not restart.".to_owned())??;
        let _ = self.call("state", json!({})).await?;
        Ok(())
    }
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_is_bounded_and_rejects_unterminated_frames() {
        let mut reader = BufReader::with_capacity(2, &b"one\ntwo\n"[..]);
        assert_eq!(frame(&mut reader).await.unwrap().unwrap(), b"one\n");
        assert_eq!(frame(&mut reader).await.unwrap().unwrap(), b"two\n");
        assert_eq!(frame(&mut reader).await.unwrap(), None);
        assert!(frame(&mut BufReader::new(&b"partial"[..])).await.is_err());
        let oversized = vec![b'x'; MAX_FRAME + 1];
        assert!(
            frame(&mut BufReader::new(oversized.as_slice()))
                .await
                .is_err()
        );
    }
    #[test]
    fn new_ai_job_does_not_inherit_previous_art() {
        let mut cache = Cache::default();
        cache.ingest("aiResults",json!({"result":{"aiJobId":"old","aiProgressArt":{"version":4,"subject":{"id":"old"}}}}));
        cache.ingest(
            "state",
            json!({"aiSearch":{"aiJobId":"new","running":true}}),
        );
        let snapshot = cache.snapshot();
        assert_eq!(snapshot["aiStory"]["jobID"], "new");
        assert!(snapshot["aiStory"]["art"].is_null());
    }
    #[test]
    fn cached_streams_keep_identity_and_counts_without_photo_pages() {
        let mut cache = Cache::default();
        let streamed = json!({"requestId":"request","invalidated":false,"result":{"aiJobId":"job","totalCount":1000,"aiRefinedCount":17,"photos":[{"id":"photo"}],"nested":{"peoplePhotos":[{"id":"person-photo"}]}}});
        cache.ingest("aiResults", streamed.clone());
        cache.ingest("monitor", json!({"tokens":123}));
        let snapshot = cache.snapshot();
        let event = &snapshot["aiResults"];
        assert_eq!(event["requestId"], "request");
        assert_eq!(event["result"]["aiJobId"], "job");
        assert_eq!(event["result"]["totalCount"], 1000);
        assert_eq!(event["result"]["aiRefinedCount"], 17);
        assert!(event["result"].get("photos").is_none());
        assert!(event["result"]["nested"].get("peoplePhotos").is_none());
        assert_eq!(
            streamed["result"]["photos"].as_array().unwrap().len(),
            1,
            "The direct native response remains complete"
        );
        cache.ingest(
            "state",
            json!({"aiResults":streamed,"library":{"photos":1000}}),
        );
        assert!(
            cache.snapshot()["aiResults"]["result"]
                .get("photos")
                .is_none()
        );
        assert_eq!(cache.snapshot()["library"]["photos"], 1000);
    }
    #[test]
    fn metric_events_do_not_reload_gallery() {
        let mut cache = Cache::default();
        let state = json!({"photos":[{"id":"1","caption":"cat"}],"library":{"total":1}});
        cache.ingest("state", state.clone());
        let revision = cache.snapshot()["galleryRevision"].clone();
        cache.ingest("monitor", json!({"tokens":3}));
        cache.ingest("jobs", json!({"jobs":[]}));
        cache.ingest("state", state);
        assert_eq!(cache.snapshot()["galleryRevision"], revision);
        assert!(cache.snapshot().get("photos").is_none());
    }
}
