//! Ephemeral, intent-fenced access diagnostics. No access material is public.
use super::worker::Worker;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Restoring,
    Unavailable,
    PermissionDenied,
    InvalidGrant,
    OwnerUnavailable,
}
impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Restoring => "restoring",
            Self::Unavailable => "unavailable",
            Self::PermissionDenied => "permissionDenied",
            Self::InvalidGrant => "invalidGrant",
            Self::OwnerUnavailable => "ownerUnavailable",
        }
    }
    fn actionable(self) -> bool {
        matches!(
            self,
            Self::PermissionDenied | Self::InvalidGrant | Self::OwnerUnavailable
        )
    }
}
#[derive(Clone, Default)]
pub struct State {
    library: String,
    host_epoch: u64,
    intent: u64,
    roots: BTreeMap<String, Status>,
}
pub type Shared = Arc<Mutex<State>>;
fn current(worker: &Worker, epoch: &AtomicU64, library: &str, host: u64) -> bool {
    epoch.load(Ordering::Acquire) == host && worker.snapshot()["libraryId"] == library
}
async fn valid(worker: &Worker, epoch: &AtomicU64, library: &str, host: u64, intent: u64) -> bool {
    if !current(worker, epoch, library, host) {
        return false;
    }
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        worker.call(
            "sourceAccessRefs",
            json!({"libraryId":library,"generation":intent,"limit":1}),
        ),
    )
    .await;
    matches!(result,Ok(Ok(value)) if value["libraryId"]==library&&value["generation"].as_u64()==Some(intent))
        && current(worker, epoch, library, host)
}
pub fn reset(shared: &Shared) {
    *shared.lock().unwrap_or_else(|p| p.into_inner()) = State::default();
}
pub async fn record(
    shared: &Shared,
    worker: &Worker,
    epoch: &AtomicU64,
    library: &str,
    host: u64,
    intent: u64,
    changes: Vec<(String, Option<Status>)>,
    retain_actionable: bool,
) {
    if !valid(worker, epoch, library, host, intent).await {
        return;
    }
    let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
    if state.library != library || state.host_epoch != host || state.intent != intent {
        *state = State {
            library: library.into(),
            host_epoch: host,
            intent,
            roots: BTreeMap::new(),
        };
    }
    for (root, status) in changes {
        match status {
            Some(status) => {
                if retain_actionable && state.roots.get(&root).is_some_and(|old| old.actionable()) {
                    continue;
                }
                if state.roots.len() < 20000 || state.roots.contains_key(&root) {
                    state.roots.insert(root, status);
                }
            }
            None => {
                state.roots.remove(&root);
            }
        }
    }
}
pub async fn overlay(value: &mut Value, shared: &Shared, worker: &Worker, epoch: &AtomicU64) {
    let saved = shared.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if saved.roots.is_empty() {
        return;
    }
    if !valid(
        worker,
        epoch,
        &saved.library,
        saved.host_epoch,
        saved.intent,
    )
    .await
    {
        let mut current = shared.lock().unwrap_or_else(|p| p.into_inner());
        if current.library == saved.library
            && current.host_epoch == saved.host_epoch
            && current.intent == saved.intent
        {
            current.roots.clear();
        }
        return;
    }
    fn apply(source: &mut Value, roots: &BTreeMap<String, Status>) {
        if source["mode"]=="archived" || source["folderSelectionPending"]!=true
            && (source["accessOriginals"] == false || source["mode"]=="metadata")
        {
            return;
        }
        if let Some(values) = source["roots"].as_array_mut() {
            for root in values {
                if root["active"] == false && root["selectionPending"]!=true {
                    continue;
                }
                if let Some(status) = root["id"].as_str().and_then(|id| roots.get(id)) {
                    root["nativeAccessStatus"] = json!(status.label());
                    if root["recovery"]["version"]==1 && (root["recovery"]["phase"]!="needsFolder" || *status==Status::OwnerUnavailable) {
                        let (phase,action,title,message)=match status {
                            Status::Restoring => ("checking","none","Restoring folder access","Saved folder permission is being restored automatically. No action is needed while this progresses."),
                            Status::Unavailable => ("retrying","retry","Folder access is temporarily unavailable","PhotoFindry is retrying. Check that the folder opens in your file manager, then retry the connection here."),
                            Status::PermissionDenied | Status::InvalidGrant => ("needsAccess","allowAccess","Allow folder access","Saved access to this folder could not be restored. Allow access below; your photos, tags and progress are kept."),
                            Status::OwnerUnavailable => ("needsRepair","recoverCompanion","Recover the companion","The installed folder-access component needs repair. Use verified companion recovery; your library is kept."),
                        };
                        for (key,value) in [("phase",phase),("action",action),("title",title),("message",message)] {root["recovery"][key]=json!(value);}
                    }
                }
            }
        }
        if source["roots"].as_array().is_some_and(|values|values.len()==1) && source["roots"][0]["recovery"].is_object() {
            source["recovery"]=source["roots"][0]["recovery"].clone();
        }
    }
    if let Some(sources) = value["sources"].as_array_mut() {
        for source in sources {
            apply(source, &saved.roots);
        }
    }
    if value["source"].is_object() {
        apply(&mut value["source"], &saved.roots);
    }
}
