//! One owned native FSEvents/volume observer. Paths stay in the private host;
//! notifications schedule existing attribute-only source checks, never prove loss.
use super::worker::Worker;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::{Component, Path}, process::Stdio, sync::{Arc, Mutex}, time::Duration};
use tokio::{io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader}, process::Command, sync::{Notify, watch}, task::JoinHandle};

const ROOT_LIMIT: usize = 256;
const ROOT_BYTES: usize = 16_384;
const FRAME_BYTES: usize = 4 * 1024 * 1024 + 65_536;

#[derive(Clone, Debug, Default, PartialEq)]
struct Configuration { generation: u64, roots: Vec<String> }
#[derive(Default)]
struct Hints { generation: u64, roots: BTreeSet<String> }
#[derive(Clone)]
pub struct SourceWatcher { roots: watch::Sender<Configuration>, stop: watch::Sender<bool> }

fn normalize(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.contains('\0') || path.len() > ROOT_BYTES { return None; }
    let mut parts = Vec::new();
    for part in Path::new(path).components() {
        match part { Component::Normal(name) => parts.push(name.to_str()?), Component::ParentDir => { parts.pop(); }, Component::RootDir | Component::CurDir => {}, _ => return None }
    }
    Some(format!("/{}", parts.join("/")))
}

fn registered_roots(state: &Value) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for source in state["sources"].as_array().into_iter().flatten() {
        if source["accessOriginals"] == false || matches!(source["mode"].as_str(), Some("metadata" | "archived")) { continue; }
        for root in source["roots"].as_array().into_iter().flatten() {
            if root["active"] == false { continue; }
            if let Some(path) = root["path"].as_str().and_then(normalize) { roots.insert(path); }
            if roots.len() > ROOT_LIMIT { roots.pop_last(); }
        }
    }
    roots.into_iter().collect()
}

async fn frame<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> Result<Option<Vec<u8>>, ()> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|_| ())?;
        if available.is_empty() { return if bytes.is_empty() { Ok(None) } else { Err(()) }; }
        let amount = available.iter().position(|byte| *byte == b'\n').map(|n| n + 1).unwrap_or(available.len());
        if bytes.len() + amount > FRAME_BYTES { return Err(()); }
        let complete = available[amount - 1] == b'\n';
        bytes.extend_from_slice(&available[..amount]); reader.consume(amount);
        if complete { return Ok(Some(bytes)); }
    }
}

fn accept_hints(value: &Value, config: &Configuration, pending: &mut Hints) -> bool {
    let Some(object) = value.as_object() else { return false; };
    if object.len() != 2 || value["generation"].as_u64() != Some(config.generation) { return false; }
    let Some(paths) = value["roots"].as_array().filter(|paths| paths.len() <= ROOT_LIMIT) else { return false; };
    if paths.iter().any(|path| !path.as_str().is_some_and(|path| config.roots.iter().any(|root| root == path))) { return false; }
    if pending.generation != config.generation { pending.generation = config.generation; pending.roots.clear(); }
    for path in paths { pending.roots.insert(path.as_str().unwrap().to_owned()); }
    !pending.roots.is_empty()
}

impl SourceWatcher {
    pub fn spawn(helper: &Path, worker: Worker) -> Result<(Self, JoinHandle<()>), String> {
        let mut command = Command::new(helper);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        for (key, _) in std::env::vars_os() { if key.to_string_lossy().starts_with("DYLD_") { command.env_remove(key); } }
        let mut child = command.spawn().map_err(|_| "Cannot start the native source observer.".to_owned())?;
        let mut input = child.stdin.take().ok_or("Source observer input is unavailable.")?;
        let output = child.stdout.take().ok_or("Source observer output is unavailable.")?;
        let mut errors = child.stderr.take().ok_or("Source observer diagnostics are unavailable.")?;
        let (roots, mut updates) = watch::channel(Configuration::default());
        let current = roots.subscribe();
        let (stop, mut stopped) = watch::channel(false);
        let pending = Arc::new(Mutex::new(Hints::default()));
        let notify = Arc::new(Notify::new());
        let task_stop = stop.clone();
        let task = tokio::spawn(async move {
            let mut writer = tokio::spawn(async move {
                loop {
                    let config = updates.borrow_and_update().clone();
                    let mut bytes = serde_json::to_vec(&json!({"generation":config.generation,"roots":config.roots})).map_err(|_| ())?;
                    bytes.push(b'\n');
                    if bytes.len() > FRAME_BYTES { return Err::<(), ()>(()); }
                    tokio::time::timeout(Duration::from_secs(5), input.write_all(&bytes)).await.map_err(|_| ())?.map_err(|_| ())?;
                    if updates.changed().await.is_err() { return Ok(()); }
                }
            });
            let reader_current = current.clone(); let reader_pending = pending.clone(); let reader_notify = notify.clone();
            let mut reader = tokio::spawn(async move {
                let mut output = BufReader::with_capacity(8192, output);
                while let Ok(Some(bytes)) = frame(&mut output).await {
                    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { break; };
                    if accept_hints(&value, &reader_current.borrow(), &mut reader_pending.lock().unwrap()) { reader_notify.notify_one(); }
                }
            });
            let dispatcher = tokio::spawn(async move {
                loop {
                    notify.notified().await;
                    let paths = {
                        let configuration = current.borrow();
                        let mut hints = pending.lock().unwrap();
                        if hints.generation != configuration.generation { hints.roots.clear(); continue; }
                        let paths: Vec<String> = std::mem::take(&mut hints.roots).into_iter().filter(|path| configuration.roots.contains(path)).take(ROOT_LIMIT).collect();
                        paths
                    };
                    if paths.is_empty() { continue; }
                    // The worker preserves automatic holds and source identity. Never
                    // resume the queue, read originals or mutate names from an event.
                    if worker.call("checkSources", json!({"paths":paths,"automatic":true})).await.is_err() {
                        eprintln!("PhotoFindry source observer: automatic location check deferred.");
                    }
                }
            });
            let diagnostics = tokio::spawn(async move { let mut bytes = [0u8; 8192]; while let Ok(n) = errors.read(&mut bytes).await { if n == 0 { break; } } });
            let writer_finished = tokio::select! {
                _ = stopped.changed() => false,
                _ = child.wait() => false,
                _ = &mut writer => { eprintln!("PhotoFindry source observer stopped accepting root updates."); true },
                _ = &mut reader => { eprintln!("PhotoFindry source observer stopped sending change hints."); false },
            };
            let _ = task_stop.send(true);
            dispatcher.abort(); writer.abort(); reader.abort(); diagnostics.abort();
            // Dropping the writer closes stdin. Graceful EOF stops the observer;
            // a stalled provider cannot block shutdown of the main companion.
            if !writer_finished { let _ = writer.await; }
            if tokio::time::timeout(Duration::from_secs(2), child.wait()).await.is_err() { let _ = child.kill().await; let _ = child.wait().await; }
        });
        Ok((Self { roots, stop }, task))
    }

    /// Accept only the worker's private raw state, never the browser projection.
    pub fn update(&self, state: &Value) {
        if *self.stop.borrow() { return; }
        let roots = registered_roots(state);
        self.roots.send_if_modified(|current| {
            if current.roots == roots { return false; }
            current.generation = current.generation.wrapping_add(1).min(9_007_199_254_740_991);
            current.roots = roots; true
        });
    }

    pub fn stop(&self) { let _ = self.stop.send(true); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_are_current_registered_sources_with_original_access_only() {
        let state = json!({"sources":[
            {"roots":[{"path":"/photos/one/../two"},{"path":"/photos/two"},{"path":"/inactive","active":false}]},
            {"mode":"metadata","roots":[{"path":"/metadata"}]},
            {"mode":"archived","roots":[{"path":"/archive"}]},
            {"accessOriginals":false,"roots":[{"path":"/blocked"}]},
            {"roots":[{"path":"relative"},{"path":"/bad\u{0000}root"}]}
        ]});
        assert_eq!(registered_roots(&state), vec!["/photos/two"]);
        let large = json!({"sources":[{"roots":(0..1000).map(|n|json!({"path":format!("/root-{n:04}")})).collect::<Vec<_>>()}]});
        assert_eq!(registered_roots(&large).len(), ROOT_LIMIT);
    }
    #[test]
    fn stale_or_unregistered_hints_never_reach_the_worker() {
        let config = Configuration { generation: 4, roots: vec!["/one".into(), "/two".into()] };
        let mut hints = Hints::default();
        assert!(!accept_hints(&json!({"generation":3,"roots":["/one"]}), &config, &mut hints));
        assert!(!accept_hints(&json!({"generation":4,"roots":["/unregistered"]}), &config, &mut hints));
        assert!(accept_hints(&json!({"generation":4,"roots":["/one"]}), &config, &mut hints));
        assert!(accept_hints(&json!({"generation":4,"roots":["/two"]}), &config, &mut hints));
        assert_eq!(hints.roots.len(), 2);
        assert!(!accept_hints(&json!({"generation":4,"roots":["/one"],"method":"delete"}), &config, &mut hints));
    }
    #[tokio::test]
    async fn source_frames_reject_oversize_and_partial_input() {
        let mut reader = BufReader::new(&b"one\ntwo\n"[..]);
        assert_eq!(frame(&mut reader).await.unwrap().unwrap(), b"one\n");
        assert_eq!(frame(&mut reader).await.unwrap().unwrap(), b"two\n");
        assert!(frame(&mut BufReader::new(&b"partial"[..])).await.is_err());
        let large = vec![b'x'; FRAME_BYTES + 1];
        assert!(frame(&mut BufReader::new(large.as_slice())).await.is_err());
    }
}
