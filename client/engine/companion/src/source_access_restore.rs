//! Bounded, silent renewal of currently missing native reader grants.
//! Stable Worker root/owner references select authority; paths never do.
use super::{
    lifecycle,
    source_access::{self, Owner},
    source_access_status::{self, Shared, Status},
    worker::Worker,
};
use serde_json::json;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
const INTERVAL: Duration = Duration::from_secs(5);
static NETWORK_RESTORE_TURN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
async fn restore_with_compatibility(executable:PathBuf,ids:&[String],generation:u64)->Result<source_access::Restored,String> {
    let mut restored=Owner::verified(executable.clone()).restore(ids,generation).await?;
    if restored.failures.is_empty(){return Ok(restored);}
    let owner=executable.clone();
    let context=tokio::task::spawn_blocking(move||lifecycle::source_access_network_context(&owner)).await;
    let Ok(Ok(Some((root,helper))))=context else{return Ok(restored)};
    // Support retained protocol1 owners without replacing their sealed code or
    // bookmark. Every repaired reference must still obtain a real OS scope.
    let start=NETWORK_RESTORE_TURN.fetch_add(4,Ordering::Relaxed);let mut changed=false;let mut present_network=HashSet::new();
    for offset in 0..restored.failures.len().min(4) {
        let id=restored.failures[(start+offset)%restored.failures.len()].owner_grant_id.clone();
        let directory=root.clone();let requested=id.clone();
        let record=tokio::task::spawn_blocking(move||super::retained_access_owner::network_grant_candidate(&directory,&requested)).await;
        let Ok(Ok(Some(record)))=record else{continue};
        let Some(path)=record["selectedPath"].as_str().filter(|s|s.starts_with('/')&&!s.contains('\0')&&s.len()<=16384) else{continue};
        let Ok(info)=source_access::network_info(&helper,path).await else{continue};
        if info["status"]=="available"&&info["isDirectory"]==true&&info["local"]==false&&info["path"]==path {
            present_network.insert(id.clone());
        }
        let directory=root.clone();
        changed |= matches!(tokio::task::spawn_blocking(move||super::retained_access_owner::normalize_network_grant(&directory,&id,&record,&info)).await,Ok(Ok(true)));
    }
    if changed {restored=Owner::verified(executable).restore(ids,generation).await?;}
    // Older owners reported every restoration problem as unavailable. A
    // responding network folder with no restorable OS scope needs the existing
    // native access action, rather than an endless invisible retry.
    for failure in &mut restored.failures {
        if present_network.contains(&failure.owner_grant_id)&&matches!(failure.kind,source_access::FailureKind::Unavailable) {failure.kind=source_access::FailureKind::InvalidGrant;}
    }
    Ok(restored)
}
struct Progress {
    generation: Option<u64>,
    cursor: Option<String>,
}
#[derive(Default)]
pub struct Restorer {
    task: Option<JoinHandle<Result<Progress, String>>>,
    library_id: String,
    host_generation: u64,
    generation: Option<u64>,
    cursor: Option<String>,
    next: Option<Instant>,
    reported_failure: bool,
    statuses: Shared,
}
fn current(worker: &Worker, epoch: &AtomicU64, library: &str, expected: u64) -> bool {
    epoch.load(Ordering::Acquire) == expected && worker.snapshot()["libraryId"] == library
}
async fn turn(
    worker: Worker,
    epoch: Arc<AtomicU64>,
    library: String,
    host_generation: u64,
    generation: Option<u64>,
    cursor: Option<String>,
    statuses: Shared,
) -> Result<Progress, String> {
    if !current(&worker, &epoch, &library, host_generation) {
        return Err("The library changed while restoring folder access.".into());
    }
    // Developer/uninstalled binaries keep their explicit path-only qualification
    // boundary. Managed missing/corrupt owners fail; they never fall back here.
    let owner =
        match tokio::task::spawn_blocking(lifecycle::source_access_helper_for_current_executable)
            .await
        {
            Ok(Ok(None)) => {
                return Ok(Progress {
                    generation: None,
                    cursor: None,
                });
            }
            Ok(Ok(Some(path))) => OwnerPath::Installed(path),
            _ => OwnerPath::Unavailable,
        };
    pages_with_status(
        worker,
        epoch,
        library,
        host_generation,
        generation,
        cursor,
        owner,
        Some(statuses),
    )
    .await
}
enum OwnerPath {
    Installed(PathBuf),
    Unavailable,
    #[cfg(test)]
    Fixture(PathBuf),
}
impl OwnerPath {
    async fn for_page(&self, index: usize) -> Result<PathBuf, String> {
        match self{
        Self::Installed(path) if index==0=>Ok(path.clone()),
        Self::Installed(_)=>tokio::task::spawn_blocking(lifecycle::source_access_helper_for_current_executable).await.map_err(|_|"Cannot verify the folder access owner.")?.and_then(|path|path.ok_or_else(||"The installed folder access owner is unavailable.".into())),
        Self::Unavailable=>Err("The installed folder access owner is unavailable. Use verified companion recovery.".into()),
        #[cfg(test)]Self::Fixture(path)=>Ok(path.clone()),
    }
    }
}
#[cfg(test)]
async fn pages(
    worker: Worker,
    epoch: Arc<AtomicU64>,
    library: String,
    host_generation: u64,
    generation: Option<u64>,
    cursor: Option<String>,
    owner: OwnerPath,
) -> Result<Progress, String> {
    pages_with_status(
        worker,
        epoch,
        library,
        host_generation,
        generation,
        cursor,
        owner,
        None,
    )
    .await
}
async fn note(
    statuses: &Option<Shared>,
    worker: &Worker,
    epoch: &AtomicU64,
    library: &str,
    host: u64,
    intent: u64,
    changes: Vec<(String, Option<Status>)>,
    retain: bool,
) {
    if let Some(shared) = statuses {
        source_access_status::record(
            shared, worker, epoch, library, host, intent, changes, retain,
        )
        .await;
    }
}
async fn pages_with_status(
    worker: Worker,
    epoch: Arc<AtomicU64>,
    library: String,
    host_generation: u64,
    mut generation: Option<u64>,
    mut cursor: Option<String>,
    owner: OwnerPath,
    statuses: Option<Shared>,
) -> Result<Progress, String> {
    for page_index in 0..4 {
        if !current(&worker, &epoch, &library, host_generation) {
            return Err("The library changed while restoring folder access.".into());
        }
        let mut request = json!({"libraryId":library,"limit":16});
        if let Some(value) = generation {
            request["generation"] = json!(value);
        }
        if let Some(value) = &cursor {
            request["cursor"] = json!(value);
        }
        let value = worker.call("sourceAccessRefs", request).await.map_err(
            |_| "Stored folder access could not be read; it will be retried automatically.",
        )?;
        let page = source_access::ref_page(&value, &library, generation)?;
        generation = Some(page.generation);
        if page.roots.is_empty() {
            if page.next_cursor.is_some() && page.next_cursor == cursor {
                return Err("Invalid folder access metadata cursor.".into());
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Progress { generation, cursor });
            }
            continue;
        }
        let mut seen = HashSet::new();
        let ids = page
            .roots
            .iter()
            .filter_map(|root| {
                seen.insert(root.owner_grant_id.clone())
                    .then_some(root.owner_grant_id.clone())
            })
            .collect::<Vec<_>>();
        note(
            &statuses,
            &worker,
            &epoch,
            &library,
            host_generation,
            page.generation,
            page.roots
                .iter()
                .map(|root| (root.root_id.clone(), Some(Status::Restoring)))
                .collect(),
            true,
        )
        .await;
        let executable = match owner.for_page(page_index).await {
            Ok(path) => path,
            Err(error) => {
                note(
                    &statuses,
                    &worker,
                    &epoch,
                    &library,
                    host_generation,
                    page.generation,
                    page.roots
                        .iter()
                        .map(|root| (root.root_id.clone(), Some(Status::OwnerUnavailable)))
                        .collect(),
                    false,
                )
                .await;
                return Err(error);
            }
        };
        let restored = match restore_with_compatibility(executable, &ids, page.generation).await
        {
            Ok(value) => value,
            Err(error) => {
                let status = if source_access::owner_failure(&error) {
                    Status::OwnerUnavailable
                } else {
                    Status::Unavailable
                };
                note(
                    &statuses,
                    &worker,
                    &epoch,
                    &library,
                    host_generation,
                    page.generation,
                    page.roots
                        .iter()
                        .map(|root| (root.root_id.clone(), Some(status)))
                        .collect(),
                    false,
                )
                .await;
                return Err(error);
            }
        };
        if !current(&worker, &epoch, &library, host_generation) {
            return Err("The library changed while restoring folder access.".into());
        }
        let changes = page
            .roots
            .iter()
            .map(|root| {
                let status = restored
                    .failures
                    .iter()
                    .find(|failure| failure.owner_grant_id == root.owner_grant_id)
                    .map(|failure| match failure.kind {
                        source_access::FailureKind::Unavailable => Status::Unavailable,
                        source_access::FailureKind::PermissionDenied => Status::PermissionDenied,
                        source_access::FailureKind::InvalidGrant => Status::InvalidGrant,
                    })
                    .unwrap_or(Status::Restoring);
                (root.root_id.clone(), Some(status))
            })
            .collect();
        note(
            &statuses,
            &worker,
            &epoch,
            &library,
            host_generation,
            page.generation,
            changes,
            false,
        )
        .await;
        let grants = source_access::restored_carriers(&page.roots, &restored);
        let mut deferred = false;
        if !grants.is_empty() {
            let wanted = grants.len();
            let installed = worker
                .call(
                    "installSourceAccess",
                    json!({"libraryId":library,"generation":page.generation,"grants":grants}),
                )
                .await
                .map_err(|_| "Stored folder access changed; it will be retried automatically.")?;
            if installed["libraryId"] != library
                || installed["generation"].as_u64() != Some(page.generation)
            {
                return Err("The library changed while restoring folder access.".into());
            }
            let count = installed["installed"]
                .as_u64()
                .filter(|count| *count <= wanted as u64)
                .ok_or("Invalid folder access restore result.")?;
            let waiting = installed["deferred"]
                .as_array()
                .ok_or("Invalid folder access restore result.")?;
            if waiting.iter().any(|id| {
                !id.as_str()
                    .is_some_and(|id| page.roots.iter().any(|root| root.root_id == id))
            }) {
                return Err("Invalid folder access restore result.".into());
            }
            let unique = waiting
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect::<HashSet<_>>();
            if unique.len() != waiting.len() || count as usize + unique.len() != wanted {
                return Err("Invalid folder access restore result.".into());
            }
            deferred = !waiting.is_empty() || count < wanted as u64;
            let changes = page
                .roots
                .iter()
                .filter(|root| {
                    restored
                        .grants
                        .iter()
                        .any(|grant| grant.owner_grant_id == root.owner_grant_id)
                })
                .map(|root| {
                    (
                        root.root_id.clone(),
                        if unique.contains(root.root_id.as_str()) {
                            Some(Status::Restoring)
                        } else {
                            None
                        },
                    )
                })
                .collect();
            note(
                &statuses,
                &worker,
                &epoch,
                &library,
                host_generation,
                page.generation,
                changes,
                false,
            )
            .await;
        }
        // A full live-reader budget needs time to change. An unavailable folder
        // instead advances the metadata cursor so it cannot strand later roots.
        if deferred {
            return Ok(Progress { generation, cursor });
        }
        cursor = page.next_cursor;
        if !restored.unavailable.is_empty() {
            return Ok(Progress { generation, cursor });
        }
        if cursor.is_none() {
            return Ok(Progress { generation, cursor });
        }
    }
    Ok(Progress { generation, cursor })
}
impl Restorer {
    pub fn with_status(statuses: Shared) -> Self {
        Self {
            statuses,
            ..Self::default()
        }
    }
    pub async fn tick(&mut self, worker: &Worker, epoch: &Arc<AtomicU64>) {
        let snapshot = worker.snapshot();
        let library = snapshot["libraryId"].as_str().unwrap_or("").to_owned();
        let host_generation = epoch.load(Ordering::Acquire);
        if self.library_id != library || self.host_generation != host_generation {
            if let Some(task) = self.task.take() {
                task.abort();
                let _ = task.await;
            }
            self.library_id = library.clone();
            self.host_generation = host_generation;
            self.generation = None;
            self.cursor = None;
            self.next = None;
            self.reported_failure = false;
            source_access_status::reset(&self.statuses);
        }
        if self.task.as_ref().is_some_and(|task| task.is_finished()) {
            match self.task.take().unwrap().await {
                Ok(Ok(progress)) => {
                    self.generation = progress.generation;
                    self.cursor = progress.cursor;
                    self.reported_failure = false;
                }
                _ => {
                    self.generation = None;
                    self.cursor = None;
                    if !self.reported_failure {
                        eprintln!(
                            "PhotoFindry: stored folder access is not ready; automatic restore will retry. Saved work is kept."
                        );
                        self.reported_failure = true;
                    }
                }
            }
            self.next = Some(Instant::now() + INTERVAL);
        }
        if !library.is_empty()
            && self.task.is_none()
            && self.next.is_none_or(|next| Instant::now() >= next)
        {
            self.task = Some(tokio::spawn(turn(
                worker.clone(),
                epoch.clone(),
                library,
                host_generation,
                self.generation,
                self.cursor.clone(),
                self.statuses.clone(),
            )));
        }
    }
    pub async fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    pub(super) struct Fixture {
        pub(super) root: PathBuf,
        pub(super) worker: Worker,
        task: Option<JoinHandle<()>>,
        pub(super) owner: PathBuf,
        log: PathBuf,
    }
    impl Fixture {
        pub(super) async fn new(mode: &str) -> Self {
            let mut nonce = [0u8; 12];
            getrandom::fill(&mut nonce).unwrap();
            let suffix = nonce.iter().map(|v| format!("{v:02x}")).collect::<String>();
            let root = std::env::temp_dir().join(format!("pf-access-restore-{suffix}"));
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(root.join("epoch"), "11").unwrap();
            let log = root.join("requests.jsonl");
            let process = root.join("worker");
            let script = r#"#!/usr/bin/python3
import sys,json
mode=MODE
log=LOG
epoch=EPOCH
state={'libraryId':'library','library':{'total':0},'settings':{}}
for line in sys.stdin:
 r=json.loads(line);m=r['method'];p=r.get('params',{})
 if m=='shutdown': break
 record={'method':m,'cursor':p.get('cursor')}
 if m=='state': value=state
 elif m=='sourceAccessRefs':
  intent=int(open(epoch).read())
  if p.get('generation',intent)!=intent:
   print(json.dumps({'id':r['id'],'error':'generation changed'}),flush=True);continue
  tail=bool(p.get('cursor')) or mode in ['healthy','slow']
  roots=[{'rootId':'root016','ownerGrantId':'f'*64,'path':'/owned/old-location','needed':True}] if tail else [{'rootId':'root%03d'%n,'ownerGrantId':('%064x'%n),'path':'/owned/offline','needed':True} for n in range(16)]
  if mode=='empty' and not tail: roots=[]
  value={'libraryId':'library','generation':intent,'roots':roots,'hasMore':not tail,'nextCursor':None if tail else 'root015'}
 elif m=='installSourceAccess':
  record['roots']=[{'rootId':g['rootId'],'path':g['path'],'ownerGrantId':g['ownerGrantId']} for g in p['grants']]
  value={'libraryId':'library','generation':p['generation'],'installed':0 if mode=='budget' else len(p['grants']),'deferred':[g['rootId'] for g in p['grants']] if mode=='budget' else []}
 else: value=True
 with open(log,'a') as out: out.write(json.dumps(record)+'\n')
 print(json.dumps({'id':r['id'],'result':value}),flush=True)
"#;
            fs::write(
                &process,
                script
                    .replace(
                        "mode=MODE",
                        &format!("mode={}", serde_json::to_string(mode).unwrap()),
                    )
                    .replace(
                        "log=LOG",
                        &format!(
                            "log={}",
                            serde_json::to_string(log.to_str().unwrap()).unwrap()
                        ),
                    )
                    .replace(
                        "epoch=EPOCH",
                        &format!(
                            "epoch={}",
                            serde_json::to_string(root.join("epoch").to_str().unwrap()).unwrap()
                        ),
                    ),
            )
            .unwrap();
            fs::set_permissions(&process, fs::Permissions::from_mode(0o700)).unwrap();
            let owner = root.join("owner");
            let marker = root.join("owner-started");
            let script = r#"#!/usr/bin/python3
import sys,json,base64,time
r=json.load(sys.stdin)
open(MARKER,'w').write('ready')
if MODE=='slow': time.sleep(.25)
if MODE=='intent-failure': time.sleep(.25);sys.exit(1)
v={k:r[k] for k in ['schema','operation','requestNonce','generation']}
v['entries']=[]
for x in r['grants']:
 item={'ownerGrantId':x['ownerGrantId'],'status':'unavailable'}
 if MODE=='permission': item['status']='permission'
 if x['ownerGrantId']=='f'*64 or MODE=='budget': item.update(status='available',selectedPath='/owned/original-location',path='/owned/moved-location',readerBookmark=base64.b64encode(b'fixture-reader-only').decode())
 v['entries'].append(item)
print(json.dumps(v))
"#;
            fs::write(
                &owner,
                script
                    .replace(
                        "MARKER",
                        &serde_json::to_string(marker.to_str().unwrap()).unwrap(),
                    )
                    .replace("MODE", &serde_json::to_string(mode).unwrap()),
            )
            .unwrap();
            fs::set_permissions(&owner, fs::Permissions::from_mode(0o700)).unwrap();
            let (worker, task) = Worker::spawn(&process, &root, &root).unwrap();
            worker.call("state", json!({})).await.unwrap();
            Self {
                root,
                worker,
                task: Some(task),
                owner,
                log,
            }
        }
        fn requests(&self) -> Vec<serde_json::Value> {
            fs::read_to_string(&self.log)
                .unwrap_or_default()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
        pub(super) async fn close(mut self) {
            self.worker.stop();
            if let Some(task) = self.task.take() {
                let _ = task.await;
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.worker.stop();
        }
    }
    #[tokio::test]
    async fn sixteen_unavailable_owners_do_not_strand_the_seventeenth_root() {
        let f = Fixture::new("offline").await;
        let epoch = Arc::new(AtomicU64::new(0));
        let first = pages(
            f.worker.clone(),
            epoch.clone(),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
        )
        .await
        .unwrap();
        assert_eq!(first.cursor.as_deref(), Some("root015"));
        let second = pages(
            f.worker.clone(),
            epoch,
            "library".into(),
            0,
            first.generation,
            first.cursor,
            OwnerPath::Fixture(f.owner.clone()),
        )
        .await
        .unwrap();
        assert!(second.cursor.is_none());
        let requests = f.requests();
        let installs = requests
            .iter()
            .filter(|v| v["method"] == "installSourceAccess")
            .collect::<Vec<_>>();
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0]["roots"][0]["rootId"], "root016");
        assert_eq!(installs[0]["roots"][0]["path"], "/owned/moved-location");
        f.close().await;
    }
    #[tokio::test]
    async fn full_reader_budget_yields_without_reopening_the_priority_page() {
        let f = Fixture::new("budget").await;
        let result = pages(
            f.worker.clone(),
            Arc::new(AtomicU64::new(0)),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
        )
        .await
        .unwrap();
        assert!(result.cursor.is_none());
        let requests = f.requests();
        assert_eq!(
            requests
                .iter()
                .filter(|v| v["method"] == "sourceAccessRefs")
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|v| v["method"] == "installSourceAccess")
                .count(),
            1
        );
        f.close().await;
    }
    #[tokio::test]
    async fn an_empty_metadata_page_advances_to_a_later_missing_root() {
        let f = Fixture::new("empty").await;
        let result = pages(
            f.worker.clone(),
            Arc::new(AtomicU64::new(0)),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
        )
        .await
        .unwrap();
        assert!(result.cursor.is_none());
        let requests = f.requests();
        assert_eq!(
            requests
                .iter()
                .filter(|v| v["method"] == "sourceAccessRefs")
                .count(),
            2
        );
        let installs = requests
            .iter()
            .filter(|v| v["method"] == "installSourceAccess")
            .collect::<Vec<_>>();
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0]["roots"][0]["rootId"], "root016");
        f.close().await;
    }
    #[tokio::test]
    async fn a_library_replacement_during_owner_restore_cannot_install_its_late_grant() {
        let f = Fixture::new("slow").await;
        let epoch = Arc::new(AtomicU64::new(0));
        let task = tokio::spawn(pages(
            f.worker.clone(),
            epoch.clone(),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
        ));
        let end = tokio::time::Instant::now() + Duration::from_secs(3);
        while !f.root.join("owner-started").is_file() {
            assert!(tokio::time::Instant::now() < end);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        epoch.store(1, Ordering::Release);
        assert!(task.await.unwrap().is_err());
        assert!(
            !f.requests()
                .iter()
                .any(|v| v["method"] == "installSourceAccess")
        );
        f.close().await;
    }
}

#[cfg(test)]
mod status_tests {
    use super::tests::Fixture;
    use super::*;
    fn snapshot() -> serde_json::Value {
        json!({"source":{"id":"source","mode":"connected","accessOriginals":true,"roots":[{"id":"root000","active":true,"selected":true}]}})
    }
    #[tokio::test]
    async fn only_typed_denial_is_presented_and_library_epoch_withdraws_it() {
        let f = Fixture::new("permission").await;
        let epoch = Arc::new(AtomicU64::new(0));
        let statuses: Shared = Default::default();
        pages_with_status(
            f.worker.clone(),
            epoch.clone(),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
            Some(statuses.clone()),
        )
        .await
        .unwrap();
        let mut value = snapshot();
        source_access_status::overlay(&mut value, &statuses, &f.worker, &epoch).await;
        assert_eq!(
            value["source"]["roots"][0]["nativeAccessStatus"],
            "permissionDenied"
        );
        assert!(!value.to_string().contains("ownerGrantId"));
        assert!(!value.to_string().contains("readerBookmark"));
        epoch.store(1, Ordering::Release);
        let mut changed = snapshot();
        source_access_status::overlay(&mut changed, &statuses, &f.worker, &epoch).await;
        assert!(
            changed["source"]["roots"][0]
                .get("nativeAccessStatus")
                .is_none()
        );
        f.close().await;
    }
    #[tokio::test]
    async fn intent_changes_during_a_failed_owner_call_cannot_publish_stale_access_status() {
        let f = Fixture::new("intent-failure").await;
        let epoch = Arc::new(AtomicU64::new(0));
        let statuses: Shared = Default::default();
        let pending = tokio::spawn(pages_with_status(
            f.worker.clone(),
            epoch.clone(),
            "library".into(),
            0,
            None,
            None,
            OwnerPath::Fixture(f.owner.clone()),
            Some(statuses.clone()),
        ));
        let end = tokio::time::Instant::now() + Duration::from_secs(3);
        while !f.root.join("owner-started").is_file() {
            assert!(tokio::time::Instant::now() < end);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        std::fs::write(f.root.join("epoch"), "12").unwrap();
        assert!(pending.await.unwrap().is_err());
        let mut value = snapshot();
        source_access_status::overlay(&mut value, &statuses, &f.worker, &epoch).await;
        assert!(
            value["source"]["roots"][0]
                .get("nativeAccessStatus")
                .is_none()
        );
        f.close().await;
    }
}
