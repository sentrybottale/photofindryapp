//! One owned native FSEvents/volume observer. Paths stay in the private host;
//! Notifications schedule bounded saved-source reconciliation; periodic fallback
//! checks indexed original locations only. Neither observer proves file loss.
use super::worker::Worker;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::{Component, Path}, process::Stdio, sync::{Arc, Mutex}, time::Duration};
use tokio::{io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader}, process::Command, sync::{Notify, watch}, task::JoinHandle};

const ROOT_LIMIT: usize = 256;
const OBSERVATION_ROOT_LIMIT: usize = 20_000;
const OBSERVATION_ROOT_BYTES: usize = 4 * 1024 * 1024;
const ROOT_BYTES: usize = 16_384;
const FRAME_BYTES: usize = 4 * 1024 * 1024 + 65_536;
const OBSERVATION_ROOTS: usize = 8;
const OBSERVATION_INTERVAL: Duration = Duration::from_secs(5);
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Default, PartialEq)]
struct Configuration { generation: u64, roots: Vec<String>, observation_roots: Vec<String> }
#[derive(Default)]
struct Hints { generation: u64, roots: BTreeSet<String>, events: BTreeSet<String> }
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

fn selected_roots(state: &Value, observation_only: bool) -> Vec<String> {
    let mut roots = BTreeSet::new();let mut bytes=0;
    let limit=if observation_only{OBSERVATION_ROOT_LIMIT}else{ROOT_LIMIT};
    for source in state["sources"].as_array().into_iter().flatten() {
        if observation_only && source["automaticLifecycleVersion"].as_u64() != Some(1) { continue; }
        if source["accessOriginals"] == false || matches!(source["mode"].as_str(), Some("metadata" | "archived")) { continue; }
        for root in source["roots"].as_array().into_iter().flatten() {
            if root["active"] == false { continue; }
            if let Some(path) = root["path"].as_str().and_then(normalize) {let amount=path.len();if roots.insert(path){bytes+=amount;}}
            while roots.len()>limit||bytes>OBSERVATION_ROOT_BYTES{if let Some(path)=roots.pop_last(){bytes-=path.len();}}
        }
    }
    roots.into_iter().collect()
}
fn registered_roots(state: &Value) -> Vec<String> { selected_roots(state, false) }
fn allowed(config:&Configuration,path:&String)->bool{config.roots.binary_search(path).is_ok()||config.observation_roots.binary_search(path).is_ok()}

#[derive(Default)]
struct ObservationSchedule { generation: u64, cursor: usize, retry_after: Option<tokio::time::Instant> }
impl ObservationSchedule {
    fn batch(&mut self, config: &Configuration, hints: &mut Hints, periodic: bool, now: tokio::time::Instant) -> Vec<String> {
        if self.generation != config.generation {
            self.generation = config.generation; self.cursor = 0; self.retry_after = None;
        }
        if hints.generation != config.generation { hints.generation = config.generation; hints.roots.clear(); hints.events.clear(); }
        hints.roots.retain(|path| allowed(config,path));
        hints.events.retain(|path| config.roots.contains(path));
        if self.retry_after.is_some_and(|deadline| now < deadline) { return Vec::new(); }
        // A periodic turn reserves half its bounded batch for pending events.
        // Periodic roots never enter that backlog, so event-only legacy roots
        // cannot starve behind a continuously rotating set of capable roots.
        let hint_limit = if periodic && !config.observation_roots.is_empty() { OBSERVATION_ROOTS / 2 } else { OBSERVATION_ROOTS };
        let mut paths: Vec<_> = hints.roots.iter().take(hint_limit).cloned().collect();
        if periodic && !config.observation_roots.is_empty() {
            let count = config.observation_roots.len();
            for _ in 0..count {
                if paths.len() == OBSERVATION_ROOTS { break; }
                let path = &config.observation_roots[self.cursor % count];
                self.cursor = (self.cursor + 1) % count;
                if allowed(config,path) && !paths.contains(path) { paths.push(path.clone()); }
            }
        }
        if paths.len() < OBSERVATION_ROOTS {
            for path in &hints.roots {
                if paths.len() == OBSERVATION_ROOTS { break; }
                if !paths.contains(path) { paths.push(path.clone()); }
            }
        }
        for path in &paths { hints.roots.remove(path); }
        paths
    }
    fn failed(&mut self, config: &Configuration, hints: &mut Hints, paths: &[String], now: tokio::time::Instant) {
        if hints.generation != config.generation { hints.generation = config.generation; hints.roots.clear(); hints.events.clear(); }
        for path in paths.iter().filter(|path| allowed(config,path)) { hints.roots.insert(path.clone()); }
        self.retry_after = Some(now + OBSERVATION_INTERVAL);
    }
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
    if pending.generation != config.generation { pending.generation = config.generation; pending.roots.clear(); pending.events.clear(); }
    for path in paths { let path=path.as_str().unwrap().to_owned(); pending.roots.insert(path.clone()); pending.events.insert(path); }
    !pending.roots.is_empty()
}

async fn observe_native(
    mut child: tokio::process::Child,
    mut updates: watch::Receiver<Configuration>,
    current: watch::Receiver<Configuration>,
    pending: Arc<Mutex<Hints>>,
    notify: Arc<Notify>,
    mut stopped: watch::Receiver<bool>,
) {
    let (Some(mut input), Some(output), Some(mut errors)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill().await; let _ = child.wait().await; return;
    };
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
    let mut reader = tokio::spawn(async move {
        let mut output = BufReader::with_capacity(8192, output);
        while let Ok(Some(bytes)) = frame(&mut output).await {
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { break; };
            if accept_hints(&value, &current.borrow(), &mut pending.lock().unwrap()) { notify.notify_one(); }
        }
    });
    let diagnostics = tokio::spawn(async move { let mut bytes = [0u8; 8192]; while let Ok(n) = errors.read(&mut bytes).await { if n == 0 { break; } } });
    let writer_finished = tokio::select! {
        _ = stopped.changed() => false,
        _ = child.wait() => false,
        _ = &mut writer => true,
        _ = &mut reader => false,
    };
    reader.abort(); diagnostics.abort(); writer.abort();
    // Closing stdin asks a healthy helper to exit. Native provider work can
    // stall, so only this owned process gets a bounded drain and final kill.
    if !writer_finished { let _ = writer.await; }
    if tokio::time::timeout(Duration::from_secs(2), child.wait()).await.is_err() { let _ = child.kill().await; let _ = child.wait().await; }
}

async fn dispatch_observations(
    worker: Worker,
    current: watch::Receiver<Configuration>,
    pending: Arc<Mutex<Hints>>,
    notify: Arc<Notify>,
    mut stopped: watch::Receiver<bool>,
    observation_interval: Duration,
) {
    let mut interval = tokio::time::interval(observation_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut schedule = ObservationSchedule::default();
    let mut reported_failure = false;
    'dispatch: loop {
        if *stopped.borrow() { break; }
        let periodic = tokio::select! { _ = stopped.changed() => break, _ = notify.notified() => false, _ = interval.tick() => true };
        let (configuration, paths, events) = {
            let configuration = current.borrow().clone();
            let mut hints = pending.lock().unwrap();
            let paths = schedule.batch(&configuration, &mut hints, periodic, tokio::time::Instant::now());
            let events: BTreeSet<_> = paths.iter().filter(|path|hints.events.remove(*path)).cloned().collect();
            (configuration, paths, events)
        };
        if paths.is_empty() { continue; }
        let (observation, legacy): (Vec<_>, Vec<_>) = paths.into_iter().partition(|path| configuration.observation_roots.contains(path));
        let (changed, periodic): (Vec<_>,Vec<_>) = observation.into_iter().partition(|path|events.contains(path));
        for (method, paths, event) in [("observeSources", changed, true), ("observeSources", periodic, false), ("checkSources", legacy, false)] {
            if paths.is_empty() { continue; }
            if *stopped.borrow() { break 'dispatch; }
            // Capable workers observe a small location cursor without queuing
            // an audit or releasing user/safety pauses. Legacy workers retain
            // event-only checks; periodic ticks never scan their roots.
            let mut params=json!({"paths":&paths,"automatic":true});
            if event {params["event"]=json!(true);}
            let result = tokio::select! {
                _ = stopped.changed() => break 'dispatch,
                result = tokio::time::timeout(OBSERVATION_TIMEOUT, worker.call(method, params)) => result,
            };
            if !matches!(result, Ok(Ok(_))) {
                let latest = current.borrow().clone();
                if method == "observeSources" && latest.generation == configuration.generation {
                    let mut hints=pending.lock().unwrap();
                    schedule.failed(&latest, &mut hints, &paths, tokio::time::Instant::now());
                    if event {for path in &paths {if latest.roots.contains(path) {hints.events.insert(path.clone());}}}
                }
                if !reported_failure { eprintln!("PhotoFindry source observer: automatic location observation deferred; retrying safely."); reported_failure = true; }
            } else { reported_failure = false; }
        }
    }
}

impl SourceWatcher {
    pub fn spawn(helper: &Path, worker: Worker) -> Result<(Self, JoinHandle<()>), String> {
        Self::spawn_with_interval(helper, worker, OBSERVATION_INTERVAL)
    }
    fn spawn_with_interval(helper: &Path, worker: Worker, observation_interval: Duration) -> Result<(Self, JoinHandle<()>), String> {
        let mut command = Command::new(helper);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        for (key, _) in std::env::vars_os() { if key.to_string_lossy().starts_with("DYLD_") { command.env_remove(key); } }
        let native = command.spawn();
        let (roots, updates) = watch::channel(Configuration::default());
        let (stop, stopped) = watch::channel(false);
        let pending = Arc::new(Mutex::new(Hints::default()));
        let notify = Arc::new(Notify::new());
        let mut completion_stop = stopped.clone();
        let dispatcher = tokio::spawn(dispatch_observations(worker, roots.subscribe(), pending.clone(), notify.clone(), stopped.clone(), observation_interval));
        let task = tokio::spawn(async move {
            match native {
                Ok(child) => observe_native(child, updates.clone(), updates, pending, notify, stopped).await,
                Err(_) => eprintln!("PhotoFindry native source observer unavailable; bounded location observation remains active."),
            }
            // Periodic observation is owned independently of FSEvents. A dead
            // native helper cannot strand an already registered returning drive.
            if !*completion_stop.borrow() {
                let _ = completion_stop.changed().await;
            }
            dispatcher.abort(); let _ = dispatcher.await;
        });
        Ok((Self { roots, stop }, task))
    }

    /// Accept only the worker's private raw state, never the browser projection.
    pub fn update(&self, state: &Value) {
        if *self.stop.borrow() { return; }
        let roots = registered_roots(state);
        let observation_roots = selected_roots(state, true);
        self.roots.send_if_modified(|current| {
            if current.roots == roots && current.observation_roots == observation_roots { return false; }
            current.generation = current.generation.wrapping_add(1).min(9_007_199_254_740_991);
            current.roots = roots; current.observation_roots = observation_roots; true
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
        let config = Configuration { generation: 4, roots: vec!["/one".into(), "/two".into()], observation_roots: vec![] };
        let mut hints = Hints::default();
        assert!(!accept_hints(&json!({"generation":3,"roots":["/one"]}), &config, &mut hints));
        assert!(!accept_hints(&json!({"generation":4,"roots":["/unregistered"]}), &config, &mut hints));
        assert!(accept_hints(&json!({"generation":4,"roots":["/one"]}), &config, &mut hints));
        assert!(accept_hints(&json!({"generation":4,"roots":["/two"]}), &config, &mut hints));
        assert_eq!(hints.roots.len(), 2);
        assert!(!accept_hints(&json!({"generation":4,"roots":["/one"],"method":"delete"}), &config, &mut hints));
    }
    #[test]
    fn periodic_observation_is_bounded_capability_gated_and_round_robin() {
        let state = json!({"sources":[
            {"automaticLifecycleVersion":1,"roots":(0..20).map(|n|json!({"path":format!("/auto-{n:02}")})).collect::<Vec<_>>()},
            {"roots":[{"path":"/legacy"}]},
            {"automaticLifecycleVersion":1,"accessOriginals":false,"roots":[{"path":"/blocked"}]},
            {"automaticLifecycleVersion":1,"mode":"metadata","roots":[{"path":"/metadata"}]}
        ]});
        let config = Configuration { generation:1, roots:registered_roots(&state), observation_roots:selected_roots(&state,true) };
        let mut schedule=ObservationSchedule::default();let mut hints=Hints::default();let now=tokio::time::Instant::now();let mut seen=BTreeSet::new();
        for tick in 0..3 {
            let batch=schedule.batch(&config,&mut hints,true,now+OBSERVATION_INTERVAL*tick);
            assert!(batch.len()<=OBSERVATION_ROOTS);assert!(batch.iter().all(|path|path.starts_with("/auto-")));seen.extend(batch);
        }
        assert_eq!(seen.len(),20);assert!(hints.roots.is_empty(),"Periodic observation never creates an audit/event backlog");
        let legacy=Configuration{generation:2,roots:vec!["/legacy".into()],observation_roots:vec![]};
        assert!(schedule.batch(&legacy,&mut hints,true,now).is_empty());
    }
    #[test]
    fn periodic_turns_include_the_257th_root_without_expanding_native_helper_authority(){
        let state=json!({"sources":[{"automaticLifecycleVersion":1,"roots":(0..300).map(|n|json!({"path":format!("/root-{n:03}")})).collect::<Vec<_>>()}]});
        let config=Configuration{generation:1,roots:registered_roots(&state),observation_roots:selected_roots(&state,true)};
        assert_eq!(config.roots.len(),ROOT_LIMIT);assert_eq!(config.observation_roots.len(),300);
        let mut hints=Hints::default();assert!(!accept_hints(&json!({"generation":1,"roots":["/root-256"]}),&config,&mut hints),"A periodic root does not extend the native helper's input allowlist");
        let now=tokio::time::Instant::now();let mut schedule=ObservationSchedule::default();let mut seen=BTreeSet::new();
        for tick in 0..38{let batch=schedule.batch(&config,&mut hints,true,now+OBSERVATION_INTERVAL*tick);assert!(batch.len()<=OBSERVATION_ROOTS);seen.extend(batch);}
        assert_eq!(seen.len(),300);assert!(seen.contains("/root-256"));
        schedule.failed(&config,&mut hints,&["/root-299".into()],now+OBSERVATION_INTERVAL*38);
        assert!(schedule.batch(&config,&mut hints,false,now+OBSERVATION_INTERVAL*39).contains(&"/root-299".to_owned()),"A later registered root retains bounded failure retry too");
    }
    #[test]
    fn pending_event_roots_drain_without_another_hint_while_periodic_roots_remain_fair() {
        let automatic=(0..24).map(|n|format!("/auto-{n:02}")).collect::<Vec<_>>();
        let legacy=(0..24).map(|n|format!("/legacy-{n:02}")).collect::<Vec<_>>();
        let mut all=automatic.clone();all.extend(legacy.clone());
        let config=Configuration{generation:5,roots:all,observation_roots:automatic.clone()};
        let mut hints=Hints{generation:5,roots:legacy.iter().cloned().collect(),events:legacy.iter().cloned().collect()};let mut schedule=ObservationSchedule::default();let now=tokio::time::Instant::now();let mut seen=BTreeSet::new();
        for tick in 0..6 {seen.extend(schedule.batch(&config,&mut hints,true,now+OBSERVATION_INTERVAL*tick));}
        assert!(hints.roots.is_empty());assert!(legacy.iter().all(|path|seen.contains(path)));assert!(automatic.iter().all(|path|seen.contains(path)));
    }
    #[test]
    fn failed_observations_retry_with_backoff_and_never_leak_previous_generation_roots() {
        let config=Configuration{generation:1,roots:vec!["/one".into()],observation_roots:vec!["/one".into()]};
        let mut hints=Hints::default();let mut schedule=ObservationSchedule::default();let now=tokio::time::Instant::now();
        let first=schedule.batch(&config,&mut hints,true,now);schedule.failed(&config,&mut hints,&first,now);
        assert!(schedule.batch(&config,&mut hints,false,now+Duration::from_secs(1)).is_empty());
        assert_eq!(schedule.batch(&config,&mut hints,true,now+OBSERVATION_INTERVAL),vec!["/one"]);
        schedule.failed(&config,&mut hints,&first,now+OBSERVATION_INTERVAL);
        let replacement=Configuration{generation:2,roots:vec!["/two".into()],observation_roots:vec!["/two".into()]};
        assert_eq!(schedule.batch(&replacement,&mut hints,true,now+OBSERVATION_INTERVAL),vec!["/two"]);
        hints.roots.insert("/one".into());let same_generation=Configuration{generation:2,roots:vec!["/three".into()],observation_roots:vec!["/three".into()]};
        assert_eq!(schedule.batch(&same_generation,&mut hints,true,now+OBSERVATION_INTERVAL),vec!["/three"]);
        assert!(!hints.roots.contains("/one"));
    }
    #[tokio::test]
    async fn periodic_observation_survives_dead_missing_and_silent_native_helpers_and_stops_owned_work() {
        use std::{fs, os::unix::fs::PermissionsExt};
        for mode in ["dead", "missing", "silent", "hint"] {
            let root=std::env::temp_dir().canonicalize().unwrap().join(format!("source-observer-{}-{}-{mode}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            fs::create_dir(&root).unwrap();fs::set_permissions(&root,fs::Permissions::from_mode(0o700)).unwrap();
            let script=root.join("worker");
            fs::write(&script,r#"#!/usr/bin/python3
import json,pathlib,sys
root=pathlib.Path(sys.argv[sys.argv.index('--data-dir')+1])
for line in sys.stdin:
 request=json.loads(line)
 if request['method']=='shutdown': break
 with (root/'requests.jsonl').open('a') as output: output.write(json.dumps(request)+'\n')
 print(json.dumps({'id':request['id'],'result':{'scheduled':True}}),flush=True)
"#).unwrap();fs::set_permissions(&script,fs::Permissions::from_mode(0o700)).unwrap();
            let helper=root.join("helper");
            if mode!="missing" {
                fs::write(&helper,match mode {"dead"=>"#!/bin/sh\nexit 1\n", "hint"=>"#!/bin/sh\nwhile IFS= read -r line; do printf '%s\\n' \"$line\"; done\n", _=>"#!/bin/sh\nwhile IFS= read -r line; do :; done\n"}).unwrap();
                fs::set_permissions(&helper,fs::Permissions::from_mode(0o700)).unwrap();
            }
            let (worker,worker_task)=Worker::spawn(&script,&root,&root).unwrap();
            let (watcher,watcher_task)=SourceWatcher::spawn_with_interval(&helper,worker.clone(),Duration::from_millis(25)).unwrap();
            let requests=|| -> Vec<Value> {fs::read_to_string(root.join("requests.jsonl")).unwrap_or_default().lines().map(|line|serde_json::from_str(line).unwrap()).collect()};
            for location in ["/registered-one","/registered-two"] {
                watcher.update(&json!({"sources":[{"automaticLifecycleVersion":1,"roots":[{"path":location}]}]}));
                let deadline=tokio::time::Instant::now()+Duration::from_secs(3);
                while !requests().iter().any(|request|request["method"]=="observeSources"&&request["params"]["paths"]==json!([location])) {
                    assert!(tokio::time::Instant::now()<deadline,"{mode} helper stranded periodic observation");tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            assert!(requests().iter().all(|request|request["method"]=="observeSources"&&request["params"]["automatic"]==true));
            if mode=="hint" {
                let deadline=tokio::time::Instant::now()+Duration::from_secs(3);
                while !(requests().iter().any(|request|request["params"]["event"]==true)&&requests().iter().any(|request|request["params"].get("event").is_none())) {
                    assert!(tokio::time::Instant::now()<deadline,"Native events must select bounded reconciliation while periodic turns remain attribute-only");tokio::time::sleep(Duration::from_millis(10)).await;
                }
            } else {assert!(requests().iter().all(|request|request["params"].get("event").is_none()),"No native event is invented after observer failure or silence");}
            watcher.stop();tokio::time::timeout(Duration::from_secs(3),watcher_task).await.unwrap().unwrap();
            let stopped=requests().len();tokio::time::sleep(Duration::from_millis(100)).await;assert_eq!(requests().len(),stopped,"No timer or retry survives Stop");
            worker.stop();tokio::time::timeout(Duration::from_secs(3),worker_task).await.unwrap().unwrap();fs::remove_dir_all(root).unwrap();
        }
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
