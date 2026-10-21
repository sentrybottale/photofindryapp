//! macOS-only experiment: immutable local UI, explicitly isolated library, no remote trust.
#[path = "cli_release.rs"]
mod cli_release;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "retained_access_owner.rs"]
mod retained_access_owner;
#[path = "auth.rs"]
mod auth;
#[path = "appearance.rs"]
mod appearance;
#[path = "downloads.rs"]
mod downloads;
#[path = "feedback.rs"]
mod feedback;
#[path = "glyphs.rs"]
mod glyphs;
#[path = "labels.rs"]
mod labels;
#[path = "launch.rs"]
mod launch;
#[path = "model_files.rs"]
mod model_files;
#[path = "review_files.rs"]
mod review_files;
#[path = "native_picker.rs"]
mod native_picker;
#[path = "native_access.rs"]
mod native_access;
#[path = "operations.rs"]
mod operations;
#[path = "onboarding.rs"]
mod onboarding;
#[path = "handover.rs"]
mod handover;
#[path = "migration.rs"]
mod migration;
#[path = "browser_package.rs"]
mod browser_package;
#[path = "power.rs"]
mod power;
#[path = "preview.rs"]
mod preview;
#[path = "projection.rs"]
mod projection;
#[path = "source_watcher.rs"]
mod source_watcher;
#[path = "source_access.rs"]
mod source_access;
#[path = "source_access_restore.rs"]
mod source_access_restore;
#[path = "source_access_status.rs"]
mod source_access_status;
#[path = "support.rs"]
mod support;
#[path = "source_preview.rs"]
mod source_preview;
#[path = "worker.rs"]
mod worker;
#[path = "transfer_results.rs"]
mod transfer_results;
#[path = "uninstall.rs"]
mod uninstall;

use auth::Authority;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    Method, Request, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderMap, HeaderValue},
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    fs::OpenOptions,
    io::{BufRead, Read, Write},
    net::Ipv4Addr,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, mpsc},
    task::JoinSet,
};

type Reply = Response<Full<Bytes>>;
const MAX_BODY: usize = 16 * 1024;
const PAGE: u64 = 60;
const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self'; font-src 'none'; media-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; worker-src 'none'; manifest-src 'none'";
struct State {
    authority: Mutex<Authority>,
    origin: String,
    host: String,
    worker: worker::Worker,
    previews: preview::Previews,
    scopes: Arc<projection::Scopes>,
    picker: Arc<native_picker::Picker>,
    source_access_status: source_access_status::Shared,
    power: Mutex<power::Power>,
    glyphs: glyphs::Glyphs,
    labels: labels::Labels,
    data: PathBuf,
    runtime: PathBuf,
    admitted: bool,
    cleanup: AtomicBool,
    downloads: downloads::Downloads,
    observer_available: AtomicBool,
    onboarding: Mutex<onboarding::Store>,
    appearance: Mutex<appearance::Store>,
    shutdown: tokio::sync::Notify,
    library_generation: Arc<AtomicU64>,
    library_replacing: Arc<AtomicBool>,
    library_transition: Arc<tokio::sync::RwLock<()>>,
    model_checks: Arc<Semaphore>,
    support: Mutex<Option<support::Usage>>,
    transfer_results: Mutex<transfer_results::Results>,
    launch_directory: PathBuf,
    registered: bool,
    uninstall_active: Arc<AtomicBool>,
    uninstall_error: Arc<Mutex<Option<String>>>,
    uninstall_shutdown: AtomicBool,
}

enum LibraryGuard {
    Read { _guard: tokio::sync::OwnedRwLockReadGuard<()> },
    Write { _guard: tokio::sync::OwnedRwLockWriteGuard<()> },
}

fn response(status: StatusCode, mime: &'static str, bytes: impl Into<Bytes>) -> Reply {
    let mut result = Response::new(Full::new(bytes.into()));
    *result.status_mut() = status;
    let headers = result.headers_mut();
    for (name, value) in [
        (header::CONTENT_TYPE, mime),
        (header::CACHE_CONTROL, "no-store"),
        (header::CONTENT_SECURITY_POLICY, POLICY),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_FRAME_OPTIONS, "DENY"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), usb=(), payment=()"),
    );
    result
}
fn error(status: StatusCode, message: &str) -> Reply {
    response(
        status,
        "application/json",
        serde_json::to_vec(&json!({"error":message})).unwrap(),
    )
}
fn json_response(value: Value) -> Reply {
    let bytes = serde_json::to_vec(&value).unwrap();
    if bytes.len() > 12 * 1024 * 1024 {
        return error(
            StatusCode::BAD_GATEWAY,
            "The local response exceeds its size limit.",
        );
    }
    response(StatusCode::OK, "application/json", bytes)
}
fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}
fn boundary(headers: &HeaderMap, host: &str, origin: &str, api: bool) -> bool {
    if one_header(headers, "host") != Some(host) {
        return false;
    }
    if api && one_header(headers, "origin") != Some(origin) {
        return false;
    }
    if headers.contains_key("origin") && one_header(headers, "origin") != Some(origin) {
        return false;
    }
    if let Some(site) = headers.get("sec-fetch-site") {
        if !matches!(site.to_str(), Ok("same-origin") | Ok("none")) {
            return false;
        }
    }
    // APIs are fetch-only; reject browser navigations/forms even from this origin.
    if api
        && headers
            .get("sec-fetch-mode")
            .is_some_and(|v| v == "navigate" || v == "no-cors")
    {
        return false;
    }
    true
}
fn authorized(headers: &HeaderMap, state: &State) -> bool {
    one_header(headers, "authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| {
            state
                .authority
                .lock()
                .unwrap()
                .authorized(token, Instant::now())
        })
}
fn library_generation_header(headers:&HeaderMap)->Option<u64> {
    let raw=one_header(headers,"x-photofindry-library-generation")?;
    let parsed=raw.parse::<u64>().ok()?;
    (parsed.to_string()==raw).then_some(parsed)
}
fn needs_library_generation(path:&str)->bool {
    matches!(path,"/api/action"|"/api/model-files/check"|"/api/review-files/check")
}
fn transition_cancel(action:&Action)->bool {
    // Restore uses a non-waiting writer, so ordinary Stop/Pause can still take
    // concurrent reader guards while another operation is active. Only this
    // inspection cancellation must pass the writer during backup verification;
    // it addresses no photo/job ID and cannot mutate the replacement library.
    action.action=="cancelTransfer"
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    code: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    ticket: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Photo {
    #[serde(rename = "photoId")]
    photo_id: String,
    #[serde(default, rename = "faceId")]
    face_id: Option<String>,
    #[serde(default, rename = "cropId")]
    crop_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    action: String,
    #[serde(default = "empty_object")]
    params: Value,
}
fn empty_object() -> Value {
    json!({})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pick {
    kind: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Activity {
    #[serde(rename = "jobId")]
    job_id: String,
    #[serde(rename = "photoId")]
    photo_id: String,
    #[serde(default)]
    kind: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Story {
    kind: String,
    #[serde(rename = "jobId")]
    job_id: String,
    sequence: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct SourceCandidate { source_id:String, photo_id:String, candidate_id:String }
impl SourceCandidate {
    fn params(&self)->Result<Value,String> {
        if [&self.source_id,&self.photo_id,&self.candidate_id].into_iter().any(|id|id.is_empty()||id.len()>128||id.contains('\0')) {
            return Err("Choose a current original-location candidate.".into());
        }
        Ok(json!({"sourceId":self.source_id,"photoId":self.photo_id,"candidateId":self.candidate_id}))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct SourceReveal { source_id:String, #[serde(default)] root_id:Option<String> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GuideUpdate {
    library_id: String,
    changes: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GuideReset {
    library_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceLabel {
    source_id: String,
    operation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceScan {
    mode: String,
}
fn preview_access(photo: &Value, id: &str, allow_removed: bool) -> bool {
    photo["id"] == id
        && (photo["previewOrigin"] == "active" || (allow_removed && photo["previewOrigin"] == "removed"))
        && !["metadata", "archived"].contains(&photo["source"]["mode"].as_str().unwrap_or(""))
}
fn project_photo(
    photo: &Value,
    previews: &preview::Previews,
    scopes: &projection::Scopes,
    details: bool,
) -> Value {
    // Crop paths are never folder scopes. Expose only saved identity and labels.
    let mut presentation = photo.clone();
    if let Some(map) = presentation.as_object_mut() { map.remove("detailCrops"); }
    let mut result = scopes.project(&presentation);
    result["hasPreview"] = json!(
        !["metadata", "archived"].contains(&photo["source"]["mode"].as_str().unwrap_or(""))
            && previews
                .reference(photo, None, None)
                .is_some()
    );
    if details {
        let accessible = !["metadata", "archived"].contains(&photo["source"]["mode"].as_str().unwrap_or(""));
        if let Some(crops) = photo["detailCrops"].as_array() {
            if crops.len() > 128 {
                result["detailCropError"] = json!("The saved crop list exceeds its display bound.");
            } else {
            result["detailCrops"] = json!(crops.iter().map(|crop| {
                json!({"id":crop["id"],"label":crop["label"],"matched":crop["matched"],"box":crop["box"],
                    "hasPreview":accessible && crop["id"].as_str().is_some_and(|id| previews.reference(photo,None,Some(id)).is_some())})
            }).collect::<Vec<_>>());
            }
        }
    }
    if !details {
        if let Some(map) = result.as_object_mut() {
            for key in ["detailSections", "detailCrops", "faces"] {
                map.remove(key);
            }
        }
    }
    result
}
fn public_result(mut value: Value, state: &State) -> Value {
    for key in ["photos", "peoplePhotos"] {
        if let Some(photos) = value[key].as_array() {
            value[key] = json!(
                photos
                    .iter()
                    .map(|p| project_photo(p, &state.previews, &state.scopes, false))
                    .collect::<Vec<_>>()
            );
        }
    }
    state.scopes.project(&value)
}
fn release_cleanup(state: &State) {
    if state.cleanup.swap(false, Ordering::AcqRel) {
        let worker = state.worker.clone();
        tokio::spawn(async move {
            let _ = worker.call("endCleanupSession", json!({})).await;
        });
    }
}
fn operation_error(message: &str) -> Reply {
    // Paths in native diagnostics are presentation, never a new capability.
    let safe = message
        .split_whitespace()
        .map(|word| {
            if word.starts_with('/') {
                Path::new(word)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("selected file")
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    error(StatusCode::CONFLICT, &safe)
}
// Publication belongs to the accepted native transfer, not the browser request.
// Dropping a JoinHandle detaches this bounded task; HTTP timeout/disconnect cannot
// strand a staged restore, attach or disconnection waiting for worker restart.
fn owned_transfer(
    worker: worker::Worker,
    admitted_data: Option<PathBuf>,
    restore_capabilities: Option<(Arc<projection::Scopes>, Arc<native_picker::Picker>, Arc<AtomicU64>, Arc<AtomicBool>)>,
    method: &'static str,
    params: Value,
    library_guard: Option<LibraryGuard>,
) -> tokio::task::JoinHandle<Result<Value, String>> {
    tokio::spawn(async move {
        // Accepted transfers outlive HTTP. Retain the transition guard until
        // publication/restart completes, including when the browser goes away.
        let _library_guard=library_guard;
        let restore_data = admitted_data.filter(|_| method == "restoreLibrary");
        if let Some(data) = &restore_data {
            let source = params["source"].as_str().ok_or("The selected backup is unavailable.")?;
            let (data, backup) = (data.clone(), PathBuf::from(source));
            tokio::task::spawn_blocking(move || handover::begin_restore_intent(&data, &backup))
                .await.map_err(|_| "Restore admission preparation stopped unexpectedly.")??;
        }
        let mut value = match worker.call(method, params).await {
            Ok(value) => value,
            Err(error) => {
                if let Some(data) = restore_data {
                    // Failure may follow a durable native publication. Preserve
                    // that intent unless the unchanged old library proves abort.
                    let _ = tokio::task::spawn_blocking(move || handover::abort_restore_intent(&data)).await;
                }
                return Err(error);
            }
        };
        if restore_data.is_some() && value["restartRequired"] != true {
            let data = restore_data.unwrap();
            let _ = tokio::task::spawn_blocking(move || handover::abort_restore_intent(&data)).await;
            return Err("The restore did not publish a native restart handoff. Check the retained library before retrying.".into());
        }
        if value["restartRequired"] == true {
            if let Some(data) = &restore_data {
                let data = data.clone();
                tokio::task::spawn_blocking(move || handover::confirm_staged_restore(&data))
                    .await.map_err(|_| "Restore stage confirmation stopped unexpectedly.")??;
            }
            if method == "restoreLibrary" {
                if let Some((scopes, picker, generation, replacing)) = restore_capabilities.as_ref() {
                    replacing.store(true, Ordering::Release);
                    generation.fetch_add(1, Ordering::AcqRel);
                    scopes.reset(); picker.revoke(); feedback::clear_reviews();
                }
            }
            worker.restart().await?;
            if let Some(data) = restore_data {
                let snapshot = worker.snapshot();
                let id = snapshot["libraryId"].as_str().ok_or("The restored library did not report its identity.")?.to_owned();
                tokio::task::spawn_blocking(move || handover::finish_restore_admission(&data, &id))
                    .await.map_err(|_| "Restore admission publication stopped unexpectedly.")??;
            }
            value["restarted"] = json!(true);
            if let Some((_,_,_,replacing)) = restore_capabilities.as_ref() { replacing.store(false,Ordering::Release); }
        }
        Ok(value)
    })
}
fn uninstall_transfer_busy(state: &State) -> bool {
    let snapshot=state.worker.snapshot();
    state.library_replacing.load(Ordering::Acquire)
        || snapshot["storageJob"]["running"]==true || snapshot["sourceTransfer"]["running"]==true
}
async fn uninstall_plan(state: &State) -> Result<uninstall::Plan,String> {
    if !state.registered {return Err("Uninstall is available from the registered installed companion. Development copies and other libraries are not removed.".into());}
    if uninstall_transfer_busy(state) {return Err("Finish or cancel the current backup or source transfer before uninstalling.".into());}
    let directory=state.launch_directory.clone();
    let executable=std::env::current_exe().map_err(|_|"Cannot locate the installed companion.")?;
    tokio::task::spawn_blocking(move||uninstall::plan(&directory,&executable))
        .await.map_err(|_|"The local uninstall review could not finish.")?
}
async fn open_native_uninstall(state: &State) -> Result<(),String> {
    uninstall_plan(state).await?;
    if let Some(root)=lifecycle::installed_root(&std::env::current_exe().map_err(|_|"Cannot locate companion")?) {
        return lifecycle::schedule(&root,"uninstall").await;
    }
    if state.uninstall_active.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() {
        return Err("An uninstall review is already open in macOS. Finish or cancel it before trying again.".into());
    }
    *state.uninstall_error.lock().unwrap()=None;
    let started=(|| {
        let executable=std::env::current_exe().map_err(|_|"Cannot locate the installed companion.")?;
        let mut command=launch::desktop_command(&executable);
        command.arg("uninstall").arg("--interactive").arg("--launch-state").arg(&state.launch_directory)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped()).kill_on_drop(false);
        // The confirmed native uninstaller must survive the companion it stops.
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
        command.spawn().map_err(|_|"The installed native uninstall review could not open.".to_owned())
    })();
    let mut child=match started {Ok(child)=>child,Err(error)=>{state.uninstall_active.store(false,Ordering::Release);return Err(error)}};
    let active=state.uninstall_active.clone();let last_error=state.uninstall_error.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut diagnostic=Vec::new();
        if let Some(mut pipe)=child.stderr.take() {
            let mut chunk=[0u8;1024];
            while let Ok(size)=pipe.read(&mut chunk).await {if size==0 {break;}let kept=size.min(4096usize.saturating_sub(diagnostic.len()));diagnostic.extend_from_slice(&chunk[..kept]);}
        }
        if !child.wait().await.is_ok_and(|status|status.success()) {
            let detail=String::from_utf8_lossy(&diagnostic).chars().filter(|c|!c.is_control()||*c=='\n').take(2048).collect::<String>();
            *last_error.lock().unwrap()=Some(if detail.trim().is_empty(){"The native uninstall did not finish. Your saved library is kept. Check the macOS message or retry from the installed companion.".into()}else{detail.trim().to_owned()});
        }
        active.store(false,Ordering::Release);
    });
    Ok(())
}
async fn api(path: &str, bytes: &[u8], state: &State, library_guard:Option<LibraryGuard>) -> Reply {
    let invalid = || error(StatusCode::BAD_REQUEST, "Invalid request fields.");
    if state.uninstall_shutdown.load(Ordering::Acquire) {return error(StatusCode::CONFLICT,"The local companion is closing. Saved progress is kept; reopen PhotoFindry when it finishes.");}
    if state.library_replacing.load(Ordering::Acquire) && !matches!(path,"/api/state"|"/api/session"|"/api/pair"|"/api/bootstrap"|"/api/disconnect"|"/api/quit"|"/api/appearance"|"/api/appearance/update") {
        return error(StatusCode::CONFLICT,"The restored library is opening. Wait for it to finish before continuing.");
    }
    match path {
        "/api/lifecycle/status" => {
            if serde_json::from_slice::<Empty>(bytes).is_err(){return invalid();}
            let exe=match std::env::current_exe(){Ok(v)=>v,Err(_)=>return operation_error("Cannot locate installed companion")};
            let Some(root)=lifecycle::installed_root(&exe)else{return json_response(json!({"installed":false,"message":"Open the latest verified installer once to enable background startup and signed CLI updates."}));};
            match tokio::task::spawn_blocking(move||lifecycle::status(&root)).await {
                Ok(Ok(value))=>json_response(value),Ok(Err(reason))=>operation_error(&reason),Err(_)=>operation_error("Could not check companion status. Try again.")
            }
        }
        "/api/lifecycle/action" => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Request {action:String}
            let Ok(input)=serde_json::from_slice::<Request>(bytes)else{return invalid();};
            if !matches!(input.action.as_str(),"check"|"update"|"rollback"|"recover"|"enable-autostart"|"disable-autostart"|"enable-update-checks"|"disable-update-checks"){return invalid();}
            if !state.registered{return operation_error("Open the installed PhotoFindry companion to manage updates.");}
            if uninstall_transfer_busy(state){return operation_error("Finish or cancel the current backup or transfer, then retry.");}
            let exe=match std::env::current_exe(){Ok(v)=>v,Err(_)=>return operation_error("Cannot locate companion")};
            let Some(root)=lifecycle::installed_root(&exe)else{return operation_error("Open the verified installer once to finish CLI migration.");};
            match lifecycle::schedule(&root,&input.action).await{Ok(())=>json_response(json!({"started":true})),Err(reason)=>operation_error(&reason)}
        }
        "/api/uninstall/plan" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {return invalid();}
            let pending=state.uninstall_active.load(Ordering::Acquire);
            let last_error=state.uninstall_error.lock().unwrap().clone();
            match uninstall_plan(state).await {
                Ok(plan)=>json_response(json!({"available":true,"pending":pending,"lastError":last_error,
                    "packageLabel":plan.package_path.display().to_string(),"libraryLabel":plan.library_path.display().to_string(),
                    "keeps":["Saved libraries, tags, people, previews and job progress","Original photographs and source folders","Downloaded models and saved model settings","Other PhotoFindry installations"]})),
                Err(reason)=>json_response(json!({"available":false,"pending":pending,"lastError":last_error,"reason":reason})),
            }
        }
        "/api/uninstall/open" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {return invalid();}
            match open_native_uninstall(state).await {Ok(())=>json_response(json!({"opened":true})),Err(reason)=>operation_error(&reason)}
        }
        "/api/pair" => {
            let Ok(input) = serde_json::from_slice::<Pair>(bytes) else {
                return invalid();
            };
            if input.code.len() != 64 {
                return invalid();
            }
            let mut authority = state.authority.lock().unwrap();
            match authority.pair(&input.code, Instant::now()) {
                Ok(token) => json_response(
                    json!({"token":token,"apiVersion":4,"instanceId":authority.instance_id(),"sessionLifetime":"process"}),
                ),
                Err(message) => error(StatusCode::UNAUTHORIZED, message),
            }
        }
        "/api/bootstrap" => {
            let Ok(input) = serde_json::from_slice::<Bootstrap>(bytes) else {
                return invalid();
            };
            if input.ticket.len() != 64 {
                return invalid();
            }
            let mut authority = state.authority.lock().unwrap();
            match authority.exchange_bootstrap(&input.ticket, Instant::now()) {
                Ok(token) => json_response(json!({"token":token,"apiVersion":4,"instanceId":authority.instance_id(),"sessionLifetime":"process"})),
                Err(message) => error(StatusCode::UNAUTHORIZED, message),
            }
        }
        "/api/session" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            let authority = state.authority.lock().unwrap();
            if !authority.active(Instant::now()) {
                return error(StatusCode::UNAUTHORIZED, "The local session was revoked.");
            }
            json_response(json!({"apiVersion":4,"instanceId":authority.instance_id(),"sessionLifetime":"process"}))
        }
        "/api/disconnect" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            state.authority.lock().unwrap().disconnect();
            state.picker.revoke();
            feedback::clear_reviews();
            release_cleanup(state);
            json_response(json!({"disconnected":true}))
        }
        "/api/quit" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            state.uninstall_shutdown.store(true, Ordering::Release);
            state.authority.lock().unwrap().disconnect();
            state.shutdown.notify_one();
            json_response(json!({"stopping":true}))
        }
        "/api/state" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            if state.library_replacing.load(Ordering::Acquire) {
                return json_response(json!({"loading":true,"libraryId":null,"libraryGeneration":state.library_generation.load(Ordering::Acquire),"runtime":{"status":"switching","message":"Opening the restored local library."}}));
            }
            let mut snapshot = state.worker.snapshot();
            source_access_status::overlay(&mut snapshot,&state.source_access_status,&state.worker,&state.library_generation).await;
            snapshot["libraryGeneration"] = json!(state.library_generation.load(Ordering::Acquire));
            snapshot["powerPreferences"] = state.power.lock().unwrap().state();
            snapshot["download"] = state.downloads.snapshot(&state.picker);
            snapshot["sourceObserver"] =
                json!({"available":state.observer_available.load(Ordering::Acquire)});
            { let generation=state.library_generation.load(Ordering::Acquire);let mut results=state.transfer_results.lock().unwrap();results.observe(&snapshot,generation);snapshot["transferResults"]=results.presentation(generation); }
            json_response(state.scopes.project_source_locations(&snapshot))
        }
        "/api/appearance" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() { return invalid(); }
            match state.appearance.lock().unwrap().snapshot() {
                Ok(value) => json_response(value), Err(message) => operation_error(&message),
            }
        }
        "/api/appearance/update" => {
            let Ok(input) = serde_json::from_slice::<appearance::Update>(bytes) else { return invalid(); };
            match state.appearance.lock().unwrap().update(input.mode) {
                Ok(value) => json_response(value), Err(message) => operation_error(&message),
            }
        }
        "/api/support" => {
            let Ok(input)=serde_json::from_slice::<Value>(bytes) else {return invalid();};
            match state.support.lock().unwrap().as_mut() {
                Some(usage)=>match usage.handle(input) {Ok(value)=>json_response(value),Err(message)=>operation_error(&message)},
                None=>json_response(json!({"showInvitation":false,"handled":true})),
            }
        }
        "/api/monitor" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() { return invalid(); }
            let snapshot = state.worker.snapshot();
            feedback::observe_diagnostics(&snapshot);
            json_response(state.scopes.project(&json!({"workerGeneration":snapshot["workerGeneration"],"monitor":snapshot["monitor"],"runtime":snapshot["runtime"]})))
        }
        "/api/library-info" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() { return invalid(); }
            let snapshot = state.worker.snapshot();
            json_response(json!({"libraryId":snapshot["libraryId"],
                "libraryGeneration":state.library_generation.load(Ordering::Acquire),
                "locationLabel":state.data.to_string_lossy(),
                "ready":snapshot["libraryId"].is_string() && snapshot["loading"] != true && snapshot["libraryError"].is_null(),
                "error":snapshot["libraryError"]}))
        }
        "/api/onboarding" | "/api/onboarding/update" | "/api/onboarding/reset" => {
            let snapshot = state.worker.snapshot();
            let Some(library_id) = snapshot["libraryId"].as_str().filter(|id| !id.is_empty() && id.len() <= 128) else {
                return error(StatusCode::SERVICE_UNAVAILABLE, "Wait for this local library to finish opening.");
            };
            let result = if path == "/api/onboarding" {
                if serde_json::from_slice::<Empty>(bytes).is_err() { return invalid(); }
                state.onboarding.lock().unwrap().snapshot(library_id)
            } else if path == "/api/onboarding/reset" {
                let Ok(input) = serde_json::from_slice::<GuideReset>(bytes) else { return invalid(); };
                if input.library_id != library_id { return error(StatusCode::CONFLICT, "The local library changed. Reopen the guide."); }
                state.onboarding.lock().unwrap().reset(library_id)
            } else {
                let Ok(input) = serde_json::from_slice::<GuideUpdate>(bytes) else { return invalid(); };
                if input.library_id != library_id { return error(StatusCode::CONFLICT, "The local library changed. Reopen the guide."); }
                state.onboarding.lock().unwrap().update(library_id, input.changes)
            };
            match result { Ok(value) => json_response(value), Err(message) => operation_error(&message) }
        }
        "/api/model-files/check" => {
            let Ok(input) = serde_json::from_slice::<Value>(bytes) else { return invalid(); };
            let (_, changes) = match operations::prepare("configure", input, &state.scopes, &state.picker) {
                Ok(value) => value, Err(message) => return operation_error(&message),
            };
            let mut settings = state.worker.snapshot()["settings"].clone();
            if !settings.is_object() { return error(StatusCode::CONFLICT, "Model settings are not available yet. Try again when the library is ready."); }
            for (key,value) in changes.as_object().unwrap() { settings[key] = value.clone(); }
            let Ok(permit) = state.model_checks.clone().try_acquire_owned() else { return error(StatusCode::CONFLICT, "A file check is already running. Try again shortly."); };
            match tokio::task::spawn_blocking(move || { let _permit=permit; model_files::check(&settings) }).await {
                Ok(report)=>json_response(report), Err(_)=>error(StatusCode::INTERNAL_SERVER_ERROR,"The local file check could not finish. Choose the file again."),
            }
        }
        "/api/review-files/check" => {
            let Ok(input) = serde_json::from_slice::<Value>(bytes) else { return invalid(); };
            let (_, changes) = match operations::prepare("configurePhotoReview", input, &state.scopes, &state.picker) {
                Ok(value) => value, Err(message) => return operation_error(&message),
            };
            let mut settings = state.worker.snapshot()["photoReviewSettings"].clone();
            if !settings.is_object() { settings=json!({"provider":"tagging-model","modelPath":"","projectorPath":"","lowMemoryTrial":false}); }
            for (key,value) in changes.as_object().unwrap() { settings[key]=value.clone(); }
            let Ok(permit)=state.model_checks.clone().try_acquire_owned() else { return error(StatusCode::CONFLICT,"A file check is already running. Try again shortly."); };
            match tokio::task::spawn_blocking(move || { let _permit=permit; review_files::check(&settings,review_files::physical_memory()) }).await {
                Ok(report)=>json_response(report), Err(_)=>error(StatusCode::INTERNAL_SERVER_ERROR,"The local reviewer check could not finish. Choose the files again."),
            }
        }
        "/api/source-candidate-preview" => {
            let Ok(input)=serde_json::from_slice::<SourceCandidate>(bytes) else { return invalid(); };
            let params=match input.params(){Ok(value)=>value,Err(message)=>return operation_error(&message)};
            let resolved=match state.worker.call("sourceCandidate",params.clone()).await {Ok(value)=>value,Err(message)=>return operation_error(&message)};
            let Some(path)=resolved["path"].as_str() else {return error(StatusCode::NOT_FOUND,"The candidate location is unavailable.");};
            let bytes=match source_preview::prepare(&state.runtime,Path::new(path)).await {Ok(value)=>value,Err(message)=>return operation_error(&message)};
            // A source disconnect, purge or new candidate review during decode
            // must not publish the formerly authorized photograph.
            match state.worker.call("sourceCandidate",params).await {
                Ok(current) if current==resolved=>response(StatusCode::OK,"image/jpeg",bytes),
                _=>error(StatusCode::CONFLICT,"The candidate location changed. Refresh Sources before previewing it."),
            }
        }
        "/api/pick" => {
            let Ok(input) = serde_json::from_slice::<Pick>(bytes) else {
                return invalid();
            };
            match state.picker.pick(&input.kind).await {
                Ok(value) => json_response(value),
                Err(message) => operation_error(&message),
            }
        }
        "/api/action" => {
            let Ok(input) = serde_json::from_slice::<Action>(bytes) else {
                return invalid();
            };
            if input.action == "feedbackDiagnostics" {
                if serde_json::from_value::<Empty>(input.params).is_err() {return invalid();}
                return json_response(feedback::diagnostic_snapshot());
            }
            if input.action == "feedbackEnvironment" {
                if serde_json::from_value::<Empty>(input.params).is_err() {return invalid();}
                return json_response(feedback::environment_snapshot());
            }
            if input.action == "cancelFeedback" {
                return match feedback::cancel_submission(input.params) {Ok(value)=>json_response(value),Err(message)=>operation_error(&message)};
            }
            if input.action == "prepareFeedbackScreenshot" {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct Screenshot {
                    grant_id: String,
                }
                let Ok(screenshot) = serde_json::from_value::<Screenshot>(input.params) else {
                    return invalid();
                };
                return match feedback::prepare_screenshot(&screenshot.grant_id, &state.picker).await
                {
                    Ok(v) => json_response(v),
                    Err(e) => operation_error(&e),
                };
            }
            if input.action == "sendFeedback" {
                return match feedback::submit(input.params, &state.picker).await {
                    Ok(value) => json_response(value),
                    Err(message) => operation_error(&message),
                };
            }
            if input.action == "sourceLabel" {
                let Ok(input) = serde_json::from_value::<SourceLabel>(input.params) else {
                    return invalid();
                };
                if input.source_id.is_empty()
                    || input.source_id.len() > 128
                    || !["show", "print", "savePDF"].contains(&input.operation.as_str())
                {
                    return invalid();
                }
                let snapshot = state.worker.snapshot();
                let Some(source) = snapshot["sources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|source| source["id"] == input.source_id)
                else {
                    return error(StatusCode::NOT_FOUND, "Choose an existing source to label.");
                };
                return match state
                    .labels
                    .label(
                        &input.source_id,
                        source["label"].as_str().unwrap_or("PhotoFindry source"),
                        &input.operation,
                    )
                    .await
                {
                    Ok(value) => json_response(value),
                    Err(message) => operation_error(&message),
                };
            }
            if input.action == "sourceLabelScan" {
                let Ok(input) = serde_json::from_value::<SourceScan>(input.params) else {
                    return invalid();
                };
                if !["image", "camera"].contains(&input.mode.as_str()) {
                    return invalid();
                }
                return match state.labels.scan(&input.mode).await {
                    Ok(value) => json_response(value),
                    Err(message) => operation_error(&message),
                };
            }
            if input.action == "startDownload" {
                let Ok(input) = serde_json::from_value::<Pick>(input.params) else {
                    return invalid();
                };
                return match state.downloads.start(&input.kind).await {
                    Ok(v) => json_response(v),
                    Err(e) => operation_error(&e),
                };
            }
            if input.action == "cancelDownload" {
                if serde_json::from_value::<Empty>(input.params).is_err() {
                    return invalid();
                }
                state.downloads.cancel();
                return json_response(json!({"cancelled":true}));
            }
            if input.action == "configurePower" {
                let Some(map) = input.params.as_object() else {
                    return invalid();
                };
                if map.len() != 1 || !map.contains_key("mode") {
                    return invalid();
                }
                return match state
                    .power
                    .lock()
                    .unwrap()
                    .configure(input.params["mode"].as_str().unwrap_or(""))
                {
                    Ok(v) => json_response(v),
                    Err(e) => operation_error(&e),
                };
            }
            if input.action == "openOriginal" || input.action == "revealPhoto" {
                let Ok(photo_input) = serde_json::from_value::<Photo>(input.params) else {
                    return invalid();
                };
                let photo = match state
                    .worker
                    .call("photoDetails", json!({"photoId":photo_input.photo_id}))
                    .await
                {
                    Ok(v) => v,
                    Err(e) => return operation_error(&e),
                };
                if photo["source"]["accessOriginals"] == false
                    || !["available", "onlineOnly", "unchecked"]
                        .contains(&photo["source"]["status"].as_str().unwrap_or(""))
                {
                    return error(
                        StatusCode::CONFLICT,
                        "Reconnect or confirm this original before opening it.",
                    );
                }
                return match state
                    .picker
                    .open_photo(
                        photo["path"].as_str().unwrap_or(""),
                        input.action == "revealPhoto",
                    )
                    .await
                {
                    Ok(v) => json_response(v),
                    Err(e) => operation_error(&e),
                };
            }
            if input.action == "revealSourceCandidate" {
                let Ok(candidate)=serde_json::from_value::<SourceCandidate>(input.params) else {return invalid();};
                let params=match candidate.params(){Ok(value)=>value,Err(message)=>return operation_error(&message)};
                let resolved=match state.worker.call("sourceCandidate",params).await {Ok(value)=>value,Err(message)=>return operation_error(&message)};
                return match state.picker.open_photo(resolved["path"].as_str().unwrap_or(""),true).await {
                    Ok(value)=>json_response(value),Err(message)=>operation_error(&message),
                };
            }
            if input.action == "revealSource" {
                let Ok(source)=serde_json::from_value::<SourceReveal>(input.params) else {return invalid();};
                if source.source_id.is_empty()||source.source_id.len()>128||source.source_id.contains('\0')
                    || source.root_id.as_ref().is_some_and(|id|id.is_empty()||id.len()>128||id.contains('\0')) {return invalid();}
                let details=match state.worker.call("sourceDetails",json!({"sourceId":source.source_id,"limit":1})).await {Ok(value)=>value,Err(message)=>return operation_error(&message)};
                if details["source"]["mode"]=="archived" { return error(StatusCode::CONFLICT,"Reconnect this archived source before revealing its originals folder."); }
                let root=details["source"]["roots"].as_array().and_then(|roots|roots.iter().find(|root|source.root_id.as_ref().is_none_or(|id|root["id"]==id.as_str())));
                let Some(path)=root.and_then(|root|root["path"].as_str()) else {return error(StatusCode::NOT_FOUND,"This source folder is no longer available.");};
                return match state.picker.open_photo(path,true).await {Ok(value)=>json_response(value),Err(message)=>operation_error(&message)};
            }
            if input.action == "revealMemoryLogs" {
                if serde_json::from_value::<Empty>(input.params).is_err() { return invalid(); }
                let path=state.data.join("diagnostics");
                if !path.is_dir() { return error(StatusCode::NOT_FOUND,"No local memory diagnostics have been saved yet."); }
                return match state.picker.open_photo(path.to_str().unwrap_or(""),true).await {
                    Ok(value)=>json_response(value),Err(message)=>operation_error(&message),
                };
            }
            if input.action == "revealTransferResult" {
                let generation=state.library_generation.load(Ordering::Acquire);let snapshot=state.worker.snapshot();
                let resolved={let mut results=state.transfer_results.lock().unwrap();results.observe(&snapshot,generation);results.resolve(input.params,generation)};
                let path=match resolved {Ok(path)=>path,Err(message)=>return operation_error(&message)};
                return match state.picker.open_photo(path.to_str().unwrap_or(""),true).await {Ok(value)=>json_response(value),Err(message)=>operation_error(&message)};
            }
            if input.action == "revealLibrary" {
                if serde_json::from_value::<Empty>(input.params).is_err() {
                    return invalid();
                }
                return match state
                    .picker
                    .open_photo(state.data.to_str().unwrap_or(""), true)
                    .await
                {
                    Ok(v) => json_response(v),
                    Err(e) => operation_error(&e),
                };
            }
            let (method, params) = match operations::prepare(
                &input.action,
                input.params,
                &state.scopes,
                &state.picker,
            ) {
                Ok(v) => v,
                Err(e) => return error(StatusCode::BAD_REQUEST, &e),
            };
            if method=="configurePhotoReview" {
                let mut settings=state.worker.snapshot()["photoReviewSettings"].clone();
                if !settings.is_object(){settings=json!({"provider":"tagging-model","modelPath":"","projectorPath":""});}
                for(key,value)in params.as_object().unwrap(){settings[key]=value.clone();}
                let Ok(permit)=state.model_checks.clone().try_acquire_owned() else {return error(StatusCode::CONFLICT,"A file check is already running. Try again shortly.");};
                let report=match tokio::task::spawn_blocking(move||{let _permit=permit;review_files::check(&settings,review_files::physical_memory())}).await {
                    Ok(value)=>value,Err(_)=>return error(StatusCode::INTERNAL_SERVER_ERROR,"The reviewer file check could not finish."),
                };
                if report["canSave"]!=true {
                    let message=report["files"].as_array().and_then(|rows|rows.iter().find(|row|row["status"]=="error"))
                        .and_then(|row|row["message"].as_str()).or_else(||report["summary"].as_str()).unwrap_or("Review the selected photo-detail reviewer files and memory requirements.");
                    return operation_error(message);
                }
            }
            let page_limit = params["limit"].as_u64().unwrap_or(60);
            let completed = if [
                "restoreLibrary",
                "attachSourceArchive",
                "exportSourceArchive",
            ]
            .contains(&method)
            {
                owned_transfer(state.worker.clone(), state.admitted.then(|| state.data.clone()),
                    Some((state.scopes.clone(),state.picker.clone(),state.library_generation.clone(),state.library_replacing.clone())), method, params,library_guard)
                    .await
                    .unwrap_or_else(
                        |_| Err("The local transfer owner stopped unexpectedly.".into()),
                    )
            } else {
                state.worker.call(method, params).await
            };
            match completed {
                Ok(mut v) => {
                    if method=="sourceDetails"{source_access_status::overlay(&mut v,&state.source_access_status,&state.worker,&state.library_generation).await;}
                    if method=="exportSourceArchive" {state.transfer_results.lock().unwrap().completed_source(&v,state.library_generation.load(Ordering::Acquire));}
                    if ["jobResults", "aiSearchPage"].contains(&method) {
                        v["limit"] = json!(page_limit);
                    }
                    if method == "beginCleanupSession" {
                        state.cleanup.store(true, Ordering::Release);
                    }
                    if method == "endCleanupSession" {
                        state.cleanup.store(false, Ordering::Release);
                    }
                    json_response(if ["sourceDetails", "prepareSourceConnection"].contains(&method) {
                        state.scopes.project_source_locations(&v)
                    } else {
                        public_result(v, state)
                    })
                }
                Err(e) => operation_error(&e),
            }
        }
        "/api/library" => {
            let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
                return invalid();
            };
            let mut input = match operations::library(value, &state.scopes) {
                Ok(v) => v,
                Err(e) => return error(StatusCode::BAD_REQUEST, &e),
            };
            let offset = input["offset"].as_u64().unwrap_or(0);
            let query = input["query"].as_str().unwrap_or("").to_owned();
            if query.len() > 4096 || offset % PAGE != 0 {
                return invalid();
            }
            let filters = input["metadataFilters"]
                .as_object()
                .is_some_and(|v| v.values().any(|v| !v.is_null() && v != ""));
            let browse = query.trim().is_empty() && !filters;
            let mut params = input.take();
            params["offset"] = json!(if browse { offset / 240 * 240 } else { offset });
            params["limit"] = json!(PAGE);
            if browse {
                params["status"] = params.get("status").cloned().unwrap_or(json!("all"));
                params["sort"] = params.get("sort").cloned().unwrap_or(json!("library"));
            } else {
                params["query"] = json!(query);
                params["statusFilter"] = params.get("status").cloned().unwrap_or(json!("all"));
                params["sort"] = params.get("sort").cloned().unwrap_or(json!("relevance"));
                params["includeRelated"] = params
                    .get("includeRelated")
                    .cloned()
                    .unwrap_or(json!(false));
            }
            let page = match state
                .worker
                .call(
                    if browse {
                        "browse"
                    } else {
                        "searchInteractive"
                    },
                    params,
                )
                .await
            {
                Ok(v) => v,
                Err(e) => return operation_error(&e),
            };
            if !page["libraryError"].is_null() {
                return error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "The local library is unavailable.",
                );
            }
            let Some(photos) = page["photos"].as_array() else {
                return error(StatusCode::BAD_GATEWAY, "Invalid local library response.");
            };
            let total = if browse {
                page["library"]["matched"].as_u64()
            } else {
                page["totalCount"].as_u64()
            };
            let Some(total) = total else {
                return error(StatusCode::BAD_GATEWAY, "Invalid local library count.");
            };
            let worker_offset = if browse {
                page["library"]["offset"].as_u64().unwrap_or(0)
            } else {
                page["offset"].as_u64().unwrap_or(offset)
            };
            let skip = offset.saturating_sub(worker_offset) as usize;
            let projected: Vec<_> = photos
                .iter()
                .skip(skip)
                .take(PAGE as usize)
                .map(|p| project_photo(p, &state.previews, &state.scopes, false))
                .collect();
            let mut result = if browse {
                json!({})
            } else {
                state.scopes.project(&page)
            };
            result["photos"] = json!(projected);
            result["totalCount"] = json!(total);
            result["offset"] = json!(offset);
            result["limit"] = json!(PAGE);
            json_response(result)
        }
        "/api/photo" | "/api/preview" => {
            let Ok(input) = serde_json::from_slice::<Photo>(bytes) else {
                return invalid();
            };
            if input.photo_id.is_empty()
                || input.photo_id.len() > 128
                || input.photo_id.contains('\0')
                || input
                    .face_id
                    .as_ref()
                    .is_some_and(|id| id.is_empty() || id.len() > 128 || id.contains('\0'))
                || input.crop_id.as_ref().is_some_and(|id| id.is_empty() || id.len() > 128 || id.contains('\0'))
                || (input.face_id.is_some() && input.crop_id.is_some())
                || (path == "/api/photo" && (input.face_id.is_some() || input.crop_id.is_some()))
            {
                return invalid();
            }
            let mut params = json!({"photoId":input.photo_id});
            if path == "/api/preview" {
                if let Some(face_id) = input.face_id.as_ref() { params["faceId"] = json!(face_id); }
                if let Some(crop_id) = input.crop_id.as_ref() { params["cropId"] = json!(crop_id); }
            }
            let photo = state
                .worker
                .call(if path == "/api/preview" { "previewDetails" } else { "photoDetails" },
                    params)
                .await;
            let photo = match photo { Ok(photo) => photo, Err(e) => return operation_error(&e) };
            if path == "/api/preview" && photo.is_null() {
                return error(StatusCode::NOT_FOUND, "This photo no longer has preview access.");
            }
            if photo["id"] != input.photo_id {
                return error(StatusCode::BAD_GATEWAY, "Invalid local photo response.");
            }
            if path == "/api/photo" {
                return json_response(project_photo(&photo, &state.previews, &state.scopes, true));
            }
            if !preview_access(&photo, &input.photo_id, true) {
                return error(StatusCode::NOT_FOUND, "Cached images are disconnected for this source.");
            }
            let Some(file) = state
                .previews
                .reference(&photo, input.face_id.as_deref(), input.crop_id.as_deref())
            else {
                return error(
                    StatusCode::NOT_FOUND,
                    "No retained preview is available for this photo.",
                );
            };
            match state.previews.read(&file) {
                Ok((mime, bytes)) => response(StatusCode::OK, mime, bytes),
                Err(_) => error(
                    StatusCode::NOT_FOUND,
                    "No retained preview is available for this photo.",
                ),
            }
        }
        "/api/activity-preview" => {
            let Ok(input) = serde_json::from_slice::<Activity>(bytes) else {
                return invalid();
            };
            let key = match input.kind.as_deref() {
                None | Some("tagging") => "taggingStory",
                Some("improvement") => "improvementStory",
                _ => return invalid(),
            };
            let snapshot = state.worker.snapshot();
            let story = &snapshot[key];
            if story["jobID"] != input.job_id || story["currentPhoto"]["photoID"] != input.photo_id
            {
                return error(
                    StatusCode::NOT_FOUND,
                    "The current prepared photo is not available.",
                );
            }
            let access = match state.worker.call("previewDetails", json!({"photoId":input.photo_id})).await {
                Ok(access) => access,
                Err(e) => return operation_error(&e),
            };
            if !preview_access(&access, &input.photo_id, false) {
                return error(StatusCode::NOT_FOUND, "The current prepared photo is no longer available.");
            }
            let current = state.worker.snapshot();
            if current[key]["jobID"] != input.job_id || current[key]["currentPhoto"] != story["currentPhoto"] {
                return error(StatusCode::CONFLICT, "The current prepared photo changed.");
            }
            match state.previews.read(
                story["currentPhoto"]["thumbnailPath"]
                    .as_str()
                    .unwrap_or(""),
            ) {
                Ok((mime, bytes)) => response(StatusCode::OK, mime, bytes),
                Err(_) => error(
                    StatusCode::NOT_FOUND,
                    "Waiting for the current prepared photo.",
                ),
            }
        }
        "/api/story" => {
            let Ok(input) = serde_json::from_slice::<Story>(bytes) else {
                return invalid();
            };
            let snapshot = state.worker.snapshot();
            if snapshot["searchAppearance"]["progressArtEnabled"] == false {
                return json_response(json!({"side":16,"beats":[[],[],[]]}));
            }
            let key = match input.kind.as_str() {
                "tagging" => "taggingStory",
                "improvement" => "improvementStory",
                "ai" => "aiStory",
                _ => return invalid(),
            };
            let story = &snapshot[key];
            if story["jobID"] != input.job_id || story["sequence"] != input.sequence {
                return error(StatusCode::CONFLICT, "The displayed story changed.");
            }
            let art = if input.kind == "ai" {
                &story["art"]
            } else {
                &story["lastCompleted"]["art"]
            };
            match state.glyphs.render(art).await {
                Ok(v) => {
                    let current = state.worker.snapshot();
                    if current["searchAppearance"]["progressArtEnabled"] == false
                        || current[key]["jobID"] != input.job_id
                        || current[key]["sequence"] != input.sequence
                    {
                        return error(StatusCode::CONFLICT, "The displayed story changed.");
                    }
                    json_response(v)
                }
                Err(e) => operation_error(&e),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "Unknown endpoint."),
    }
}

async fn handle(request: Request<Incoming>, state: Arc<State>) -> Result<Reply, Infallible> {
    let path = request.uri().path().to_owned();
    let is_api = path.starts_with("/api/");
    if request.uri().query().is_some()
        || request.uri().authority().is_some()
        || !boundary(request.headers(), &state.host, &state.origin, is_api)
    {
        return Ok(error(
            StatusCode::FORBIDDEN,
            "Only this local companion origin is allowed.",
        ));
    }
    if !is_api {
        if request.method() != Method::GET {
            return Ok(error(
                StatusCode::METHOD_NOT_ALLOWED,
                "Use GET for local assets.",
            ));
        }
        return Ok(match path.as_str() {
            "/" => response(
                StatusCode::OK,
                "text/html; charset=utf-8",
                appearance::html(include_str!("../../../web-companion/index.html"),
                    state.appearance.lock().unwrap().mode().unwrap_or_default()),
            ),
            "/ui-framework.js" => response(
                StatusCode::OK, "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/ui-framework.js")),
            ),
            "/ui-framework.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/ui-framework.css")),
            ),
            "/ui-framework.NOTICES.txt" => response(
                StatusCode::OK, "text/plain; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/ui-framework.NOTICES.txt")),
            ),
            "/photo-presentation.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/photo-presentation.css")),
            ),
            "/product-theme.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/product-theme.css")),
            ),
            "/design-system.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/design-system.css")),
            ),
            "/app.js" => response(
                StatusCode::OK,
                "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/app.js")),
            ),
            "/workspace.js" => response(
                StatusCode::OK,
                "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/workspace.js")),
            ),
            "/workspace.css" => response(
                StatusCode::OK,
                "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/workspace.css")),
            ),
            "/model-monitor.js" => response(
                StatusCode::OK, "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/model-monitor.js")),
            ),
            "/model-monitor.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/model-monitor.css")),
            ),
            "/onboarding.js" => response(
                StatusCode::OK, "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/onboarding.js")),
            ),
            "/support.js" => response(StatusCode::OK,"text/javascript; charset=utf-8",Bytes::from_static(include_bytes!("../../../web-companion/support.js"))),
            "/support-qr.png" => response(StatusCode::OK,"image/png",Bytes::from_static(include_bytes!("../../../Resources/CoffeeSupportQR.png"))),
            "/onboarding.css" => response(
                StatusCode::OK, "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/onboarding.css")),
            ),
            "/panels.js" => response(
                StatusCode::OK,
                "text/javascript; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/panels.js")),
            ),
            "/panels.css" => response(
                StatusCode::OK,
                "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/panels.css")),
            ),
            "/style.css" => response(
                StatusCode::OK,
                "text/css; charset=utf-8",
                Bytes::from_static(include_bytes!("../../../web-companion/style.css")),
            ),
            "/brand.png" => response(
                StatusCode::OK,
                "image/png",
                Bytes::from_static(include_bytes!("../../../Resources/BrandMark.png")),
            ),
            "/assets/untagged-landscape.jpg" => response(StatusCode::OK, "image/jpeg", Bytes::from_static(include_bytes!("../../../web-companion/assets/untagged-landscape.jpg"))),
            "/assets/untagged-spaces.jpg" => response(StatusCode::OK, "image/jpeg", Bytes::from_static(include_bytes!("../../../web-companion/assets/untagged-spaces.jpg"))),
            "/assets/untagged-shapes.jpg" => response(StatusCode::OK, "image/jpeg", Bytes::from_static(include_bytes!("../../../web-companion/assets/untagged-shapes.jpg"))),
            _ => error(StatusCode::NOT_FOUND, "Unknown local asset."),
        });
    }
    if request.method() != Method::POST {
        return Ok(error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Use POST for local requests.",
        ));
    }
    if !matches!(path.as_str(), "/api/pair" | "/api/bootstrap") && !authorized(request.headers(), &state) {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "Pair with the companion terminal first.",
        ));
    }
    if one_header(request.headers(), "content-type")
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        != Some("application/json")
    {
        return Ok(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Requests must use application/json.",
        ));
    }
    let request_headers = request.headers().clone();
    let max_body = if path == "/api/action" {
        12 * 1024 * 1024
    } else {
        MAX_BODY
    };
    let body = match tokio::time::timeout(
        Duration::from_secs(10),
        Limited::new(request.into_body(), max_body).collect(),
    )
    .await
    {
        Ok(Ok(body)) => body.to_bytes(),
        Ok(Err(_)) => {
            return Ok(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Request body exceeds its size limit.",
            ));
        }
        Err(_) => {
            return Ok(error(
                StatusCode::REQUEST_TIMEOUT,
                "Request body timed out.",
            ));
        }
    };
    // Revocation also fences slow request bodies and responses already being prepared.
    if !matches!(path.as_str(), "/api/pair" | "/api/bootstrap") && !authorized(&request_headers, &state) {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "The local session expired or was revoked.",
        ));
    }
    let action=if path=="/api/action" {serde_json::from_slice::<Action>(&body).ok()} else {None};
    let is_restore=action.as_ref().is_some_and(|input|input.action=="restoreLibrary");
    let cancellation=action.as_ref().is_some_and(transition_cancel);
    let library_guard=if needs_library_generation(&path)&&!cancellation {
        if is_restore {
            match state.library_transition.clone().try_write_owned() {
                Ok(guard)=>Some(LibraryGuard::Write{_guard:guard}),
                Err(_)=>return Ok(error(StatusCode::CONFLICT,"Finish or cancel the active local operation, then try Restore again.")),
            }
        } else {
            match state.library_transition.clone().try_read_owned() {
                Ok(guard)=>Some(LibraryGuard::Read{_guard:guard}),
                Err(_)=>return Ok(error(StatusCode::CONFLICT,"A library restore is active. Finish or cancel it before continuing.")),
            }
        }
    } else {None};
    // Revalidate after acquiring the guard: old settings/IDs cannot cross an
    // awaited preflight and subsequently mutate the replacement library.
    let library_generation=state.library_generation.load(Ordering::Acquire);
    if needs_library_generation(&path) {
        let Some(requested)=library_generation_header(&request_headers) else {
            return Ok(error(StatusCode::BAD_REQUEST,"Use the current local library generation for this request."));
        };
        if requested!=library_generation||state.library_replacing.load(Ordering::Acquire) {
            return Ok(error(StatusCode::CONFLICT,"The local library changed. Refresh its current state before continuing."));
        }
    }
    let result = api(&path, &body, &state,library_guard).await;
    if !is_restore && library_generation!=state.library_generation.load(Ordering::Acquire)
        && !matches!(path.as_str(),"/api/pair"|"/api/bootstrap"|"/api/session"|"/api/disconnect"|"/api/quit"|"/api/appearance"|"/api/appearance/update") {
        return Ok(error(StatusCode::CONFLICT,"The local library was replaced. Refresh its current state before continuing."));
    }
    if !matches!(path.as_str(), "/api/pair" | "/api/bootstrap" | "/api/disconnect" | "/api/quit")
        && !authorized(&request_headers, &state)
    {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "The local session expired or was revoked.",
        ));
    }
    Ok(result)
}

struct Config {
    data: PathBuf,
    runtime: PathBuf,
    worker: PathBuf,
    port: u16,
    launch: Launch,
    managed: bool,
    launch_state: Option<PathBuf>,
}
enum Launch {
    Development,
    New,
    Handover(PathBuf),
    Admitted(handover::Admission),
}
fn directory(path: &str) -> Result<PathBuf, String> {
    let supplied = Path::new(path);
    if !supplied.is_absolute() {
        return Err("Use explicit absolute paths for data, runtime and worker.".into());
    }
    let real =
        std::fs::canonicalize(supplied).map_err(|_| "An explicit directory is unavailable.")?;
    if !real.is_dir()
        || std::fs::symlink_metadata(supplied)
            .map_err(|_| "Invalid directory.")?
            .file_type()
            .is_symlink()
    {
        return Err("Use ordinary directories, not symlink roots.".into());
    }
    Ok(real)
}
fn configuration(args: Vec<String>) -> Result<Config, String> {
    let new_library = args.first().map(String::as_str) == Some("new");
    if !new_library && args.first().map(String::as_str) != Some("serve") {
        return Err("Expected serve or new. Use --help for usage.".into());
    }
    let mut data = None;
    let mut runtime = None;
    let mut worker = None;
    let mut port = None;
    let mut allow_existing = false;
    let mut handover_existing = false;
    let mut rollback = None;
    let mut managed = false;
    let mut launch_state = None;
    let mut options = args[1..].iter();
    while let Some(option) = options.next() {
        if option == "--managed" {
            if managed { return Err("Duplicate managed option.".into()); }
            managed = true;
            continue;
        }
        if option == "--allow-existing-library" {
            if allow_existing {
                return Err("Duplicate existing-library option.".into());
            }
            allow_existing = true;
            continue;
        }
        if option == "--handover-existing-library" {
            if handover_existing { return Err("Duplicate handover option.".into()); }
            handover_existing = true;
            continue;
        }
        let slot = match option.as_str() {
            "--data-dir" => &mut data,
            "--runtime" => &mut runtime,
            "--worker" => &mut worker,
            "--port" => &mut port,
            "--rollback-copy" => &mut rollback,
            "--launch-state" => &mut launch_state,
            _ => return Err("Unknown option. Use --help.".into()),
        };
        let value = options.next().ok_or("Every option requires a value.")?;
        if slot.replace(value.clone()).is_some() {
            return Err("Options may only be supplied once.".into());
        }
    }
    if (new_library && (allow_existing || handover_existing || rollback.is_some()))
        || (allow_existing && handover_existing) || (handover_existing != rollback.is_some()) {
        return Err("Choose new, development admission, or handover with its rollback copy separately.".into());
    }
    let supplied = data.ok_or("An explicit --data-dir is required.")?;
    let data = if new_library {
        let path = PathBuf::from(supplied);
        if !path.is_absolute() { return Err("Use an absolute new library path.".into()); }
        path
    } else { directory(&supplied)? };
    let launch = if new_library { Launch::New }
        else if handover_existing {
            let rollback = PathBuf::from(rollback.unwrap());
            if !rollback.is_absolute() { return Err("Use an absolute new rollback-copy path.".into()); }
            // A closed development library may graduate through the same full
            // cold-copy/schema/identity checks; its marker alone never admits it.
            Launch::Handover(rollback)
        } else if allow_existing { Launch::Development }
        else { match data.join("companion-library.json").symlink_metadata() {
            Ok(_) => Launch::Admitted(handover::validate_admission(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Launch::Development,
            Err(_) => return Err("Cannot inspect this library's companion admission.".into()),
        }};
    if !new_library && !handover_existing && !allow_existing && matches!(launch, Launch::Development) {
        if let Some(home) = std::env::var_os("HOME") {
            let active = PathBuf::from(home).join("Library/Application Support/PhotoRecall");
            let active = std::fs::canonicalize(&active).unwrap_or(active);
            if data.starts_with(active) {
                return Err("This app library needs an explicit handover and verified rollback copy before use.".into());
            }
        }
        let marker = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(data.join("companion-prototype.json"))
            .map_err(|_| "The data directory needs an explicit companion-prototype.json marker.")?;
        let metadata = marker.metadata().map_err(|_| "Invalid prototype marker.")?;
        if !metadata.is_file() || metadata.len() > 4096 {
            return Err("Invalid prototype marker.".into());
        }
        let mut bytes = Vec::new();
        marker
            .take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read prototype marker.")?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid prototype marker.")?;
        if value["format"] != "photofindry-companion-prototype" || value["version"] != 1 {
            return Err("Invalid prototype marker.".into());
        }
    }
    let runtime = directory(&runtime.ok_or("An explicit --runtime directory is required.")?)?;
    let worker = worker
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime.join("PhotoWorker"));
    if !worker.is_absolute() {
        return Err("Use an absolute worker path.".into());
    }
    let worker =
        std::fs::canonicalize(worker).map_err(|_| "The specified PhotoWorker is unavailable.")?;
    if !worker.is_file() {
        return Err("The specified PhotoWorker is not a file.".into());
    }
    let port = port
        .unwrap_or_else(|| "0".into())
        .parse()
        .map_err(|_| "Choose a valid TCP port.")?;
    if port != 0 && !matches!(launch, Launch::Development) {
        return Err("Installed library launches require a fresh automatically selected port (--port 0).".into());
    }
    if managed && (!matches!(launch, Launch::Admitted(_)) || port != 0) {
        return Err("Managed launches require an already admitted library and a fresh port.".into());
    }
    let launch_state = launch_state.map(PathBuf::from);
    if launch_state.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("Use an absolute private launch-state directory.".into());
    }
    Ok(Config {
        data,
        runtime,
        worker,
        port,
        launch,
        managed,
        launch_state,
    })
}
fn print_pair(state: &State) -> Result<(), String> {
    state.picker.revoke();
    feedback::clear_reviews();
    release_cleanup(state);
    let code = state
        .authority
        .lock()
        .unwrap()
        .issue(Instant::now())
        .map_err(str::to_owned)?;
    println!("Pairing code: {code}");
    std::io::stdout()
        .flush()
        .map_err(|_| "Cannot write to the local terminal.".into())
}

fn library_lock(data: &Path) -> Result<std::fs::File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(data.join(".companion.lock"))
        .map_err(|_| "Cannot acquire this companion library's ownership lock.")?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err("Invalid companion ownership lock.".into());
    }
    // The native worker's existing directory/worker lease remains authoritative
    // across the Swift application too. This lock gives duplicate CLI launches an
    // immediate error before a browser URL or pairing secret is advertised.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Another PhotoFindry companion already owns this library. Use its existing terminal or quit it first.".into());
    }
    Ok(file)
}
fn startup_timeout_error(stage: &'static str) -> String {
    format!(
        "PF-OPEN-300: The local library did not finish opening after five minutes while {stage}. PhotoFindry stopped this attempt; the library and saved jobs are kept. Choose Recover and open to try the previous verified software when available. If it also fails, report PF-OPEN-300 and this stage through photofindry.com/bugs; do not erase or restore the library."
    )
}
async fn run(mut config: Config) -> Result<(), String> {
    if matches!(config.launch, Launch::New) {
        config.data = handover::create_new_library(&config.data)?;
    }
    let _ownership = library_lock(&config.data)?;
    let authority = Authority::new().map_err(str::to_owned)?;
    let launch_directory = match config.launch_state.as_ref() {
        Some(path) => path.clone(),
        None => launch::default_directory()?,
    };
    let executable = std::env::current_exe().map_err(|_| "Cannot locate the installed companion.")?;
    let _package_lease=uninstall::runtime_lease(&executable,&config.runtime)?;
    let registration = launch::read_registration(&launch_directory).ok()
        .filter(|registration| registration.matches(&config.data, &config.runtime, &executable));
    if registration.is_some() {launch::ensure_available(&launch_directory)?;}
    if config.managed && registration.is_none() {
        return Err("This installed companion and library need explicit local launch registration.".into());
    }
    // Optional support preferences can never prevent opening a library. Invalid
    // or unavailable state suppresses the invitation rather than resetting it.
    let support = support::Usage::open(
        if registration.is_some() {launch_directory.join("support.json")} else {config.data.join("companion-support.json")},
        if registration.is_some() && config.launch_state.is_none() {support::legacy_state()} else {None},
    ).ok();
    let previews = preview::Previews::open(&config.data)?;
    let mut terminated = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "Cannot register companion shutdown.")?;
    let mut hung_up = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|_| "Cannot register terminal shutdown.")?;
    let receipt = if let Launch::Handover(rollback) = &config.launch {
        println!("Checking the closed library and preparing its private rollback copy. No job will start.");
        let cancelled = Arc::new(AtomicBool::new(false));
        let (data, rollback, cancellation) = (config.data.clone(), rollback.clone(), cancelled.clone());
        let mut preparation = tokio::task::spawn_blocking(move || handover::prepare_with_cancel(&data, &rollback, &cancellation));
        let completed = tokio::select! {
            result = &mut preparation => Some(result),
            _ = tokio::signal::ctrl_c() => None,
            _ = terminated.recv() => None,
            _ = hung_up.recv() => None,
        };
        Some(match completed {
            Some(result) => result.map_err(|_| "Library handover preparation stopped unexpectedly.")??,
            None => {
                cancelled.store(true, Ordering::Release);
                let _ = preparation.await;
                return Err("Library handover cancelled. The original library is unchanged; any partial rollback copy stays private.".into());
            }
        })
    } else { None };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))
        .await
        .map_err(|_| "Cannot bind the local companion port.")?;
    let host = format!(
        "127.0.0.1:{}",
        listener
            .local_addr()
            .map_err(|_| "Cannot inspect the local port.")?
            .port()
    );
    let (worker, mut worker_task) =
        worker::Worker::spawn(&config.worker, &config.data, &config.runtime)?;
    // A spawned PID is not proof of a worker lease or an opened library. Never
    // advertise pairing until the actual native host reports usable state.
    let mut state_call = Box::pin(worker.call("state", json!({})));
    let mut deadline = Box::pin(tokio::time::sleep(Duration::from_secs(300)));
    let mut progress = tokio::time::interval(Duration::from_secs(1));
    let mut last_stage = "";
    let installed_root = if config.managed && registration.is_some() {
        lifecycle::installed_root(&executable)
    } else {
        None
    };
    let initial = loop {
        tokio::select! {
            result = &mut state_call => break Some(result),
            _ = &mut deadline => break None,
            _ = progress.tick() => {
                let stage = worker.startup_stage();
                if stage != last_stage {
                    if let Some(root) = &installed_root { lifecycle::note_opening_stage(root, stage); }
                    last_stage = stage;
                }
            }
        }
    };
    drop(state_call);
    let admitted = (|| -> Result<(), String> {
        let initial = initial.ok_or_else(|| startup_timeout_error(worker.startup_stage()))?.map_err(|error| format!(
            "The local worker stopped while {}: {error} Your library and saved jobs are kept.",
            worker.startup_stage(),
        ))?;
        let id = initial["libraryId"].as_str().filter(|id| !id.is_empty())
            .ok_or("The local library did not report its identity.")?;
        if initial["loading"] == true || !initial["libraryError"].is_null() {
            return Err("The local library needs recovery before it can be opened.".into());
        }
        match &config.launch {
            Launch::New => handover::publish_new_admission(&config.data, id),
            Launch::Handover(_) => {
                let receipt = receipt.as_ref().ok_or("The verified handover receipt is unavailable.")?;
                if id != receipt.library_id { return Err("The library identity changed after its verified handover.".into()); }
                handover::publish_handover_admission(&config.data, receipt)
            }
            Launch::Admitted(admission) => handover::complete_admitted_start(&config.data, admission, id),
            _ => Ok(()),
        }
    })();
    if let Err(error) = admitted {
        worker.stop(); let _ = worker_task.await; return Err(error);
    }
    if let Some(receipt) = &receipt { println!("Verified private rollback copy: {}", receipt.rollback.display()); }
    let state = Arc::new(State {
        authority: Mutex::new(authority),
        origin: format!("http://{host}"),
        host,
        worker,
        previews,
        scopes: Arc::new(projection::Scopes::default()),
        picker: Arc::new(native_picker::Picker::new(&config.runtime)),
        source_access_status: Default::default(),
        power: Mutex::new(power::Power::new(&config.data)),
        glyphs: glyphs::Glyphs::new(&config.runtime),
        labels: labels::Labels::new(&config.runtime),
        data: config.data.clone(),
        runtime: config.runtime.clone(),
        admitted: !matches!(config.launch, Launch::Development),
        cleanup: AtomicBool::new(false),
        downloads: downloads::Downloads::new(&config.runtime),
        observer_available: AtomicBool::new(false),
        onboarding: Mutex::new(onboarding::Store::new(&config.data)),
        // Appearance belongs to this installed host, independent of a library
        // restore and of whichever loopback port the next launch receives.
        appearance: Mutex::new(appearance::Store::new(if registration.is_some() { &launch_directory } else { &config.data })),
        shutdown: tokio::sync::Notify::new(),
        library_generation: Arc::new(AtomicU64::new(0)),
        library_replacing: Arc::new(AtomicBool::new(false)),
        library_transition: Arc::new(tokio::sync::RwLock::new(())),
        model_checks: Arc::new(Semaphore::new(1)),
        support: Mutex::new(support),
        transfer_results: Mutex::new(transfer_results::Results::default()),
        launch_directory: launch_directory.clone(),
        registered: registration.is_some(),
        uninstall_active: Arc::new(AtomicBool::new(false)),
        uninstall_error: Arc::new(Mutex::new(None)),
        uninstall_shutdown: AtomicBool::new(false),
    });
    // Issue manual recovery pairing before making native bootstrap available.
    println!("PhotoFindry local companion — macOS browser migration preview");
    println!("Library: {}", config.data.display());
    println!("Local URL: {}", state.origin);
    if !config.managed && let Err(error) = print_pair(&state) {
        state.worker.stop();
        let _ = worker_task.await;
        return Err(error);
    }
    let broker = if let Some(registration) = registration.as_ref() {
        let authority_state = state.clone();
        let shutdown_state=state.clone();
        let update_state=state.clone();
        let port = listener.local_addr().map_err(|_| "Cannot inspect the local port.")?.port();
        match launch::Broker::start_with_shutdown_and_update(&launch_directory, registration, port, move || {
            if authority_state.uninstall_shutdown.load(Ordering::Acquire) {return Err(launch::CLOSING.into());}
            authority_state.authority.lock().unwrap().issue_bootstrap(Instant::now()).map_err(str::to_owned)
        },move|| {
            let _guard=shutdown_state.library_transition.try_write().map_err(|_|"A local operation is still finishing. Finish or cancel it before uninstalling.".to_owned())?;
            if uninstall_transfer_busy(&shutdown_state) {return Err("Finish or cancel the active backup or source transfer before uninstalling.".into());}
            shutdown_state.uninstall_shutdown.store(true,Ordering::Release);
            shutdown_state.authority.lock().unwrap().disconnect();
            shutdown_state.shutdown.notify_one();
            Ok(())
        },move || {
            let state=update_state.clone();
            async move {
                let _guard=state.library_transition.clone().try_write_owned()
                    .map_err(|_|"A local operation is still finishing. Wait for it or pause it, then retry the update.".to_owned())?;
                if state.downloads.snapshot(&state.picker)["running"]==true || uninstall_transfer_busy(&state) {
                    return Err("A download, backup, or source transfer is active. Finish or cancel it, then retry the app update.".into());
                }
                let ready=state.worker.call("prepareAppUpdate",json!({})).await?;
                if ready["ready"]!=true {return Err("The local worker did not confirm update readiness. Quit PhotoFindry, then retry.".into());}
                state.uninstall_shutdown.store(true,Ordering::Release);
                state.authority.lock().unwrap().disconnect();
                state.shutdown.notify_one();
                Ok(())
            }
        }) {
            Ok(broker) => Some(broker),
            Err(error) => { state.worker.stop(); let _ = worker_task.await; return Err(error); }
        }
    } else { None };
    let observer_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("CompanionSources")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| config.runtime.join("CompanionSources"));
    let observer = source_watcher::SourceWatcher::spawn(&observer_path, state.worker.clone()).ok();
    let mut access_restore=source_access_restore::Restorer::with_status(state.source_access_status.clone());
    state
        .observer_available
        .store(observer.is_some(), Ordering::Release);
    println!(
        "Open the local URL and paste the code there. Never paste it into a website.\nCommands: status, pair (revoke and issue a new code), grant KIND /absolute/path, quit. Ctrl-C also stops the owned worker."
    );
    let (commands, mut command_rx) = mpsc::channel::<String>(4);
    // Managed launches own their worker until explicit Quit or an OS signal;
    // the launcher's closed stdin is not a request to terminate them.
    let _managed_commands = config.managed.then(|| commands.clone());
    if !config.managed { std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut bytes = Vec::new();
            match (&mut input).take(1025).read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => {
                    let _ = commands.blocking_send("quit".into());
                    break;
                }
                Ok(_) if bytes.len() > 1024 => {
                    let _ = commands.blocking_send("quit".into());
                    break;
                }
                Ok(_) => {
                    if commands
                        .blocking_send(String::from_utf8_lossy(&bytes).trim().to_owned())
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }); }
    let permits = Arc::new(Semaphore::new(16));
    let mut connections = JoinSet::new();
    let mut worker_finished = false;
    let mut maintenance = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _=maintenance.tick()=>{let raw=state.worker.snapshot();feedback::observe_diagnostics(&raw);access_restore.tick(&state.worker,&state.library_generation).await;if let Some((watcher,task))=&observer{watcher.update(&raw);state.observer_available.store(!task.is_finished(),Ordering::Release);}state.power.lock().unwrap().update(&raw["jobs"]);if !state.authority.lock().unwrap().active(Instant::now()){release_cleanup(&state);}},
            _ = tokio::signal::ctrl_c() => break,
            _ = terminated.recv() => break,
            _ = hung_up.recv() => break,
            _ = state.shutdown.notified() => {
                // Give the authenticated Quit response a bounded opportunity
                // to flush before closing HTTP connections and owned workers.
                tokio::time::sleep(Duration::from_millis(100)).await;
                break;
            },
            _ = &mut worker_task => { worker_finished = true; eprintln!("The owned local worker stopped. Closing the companion."); break; },
            command = command_rx.recv() => match command.as_deref() {
                Some("pair") => { if print_pair(&state).is_err() { break; } },
                Some("status") => { let snapshot=state.worker.snapshot();println!("Library: {}\nPhotos: {} · tagged: {}\nQueue paused: {}\nLocal URL: {}",state.data.display(),snapshot["library"]["total"],snapshot["library"]["tagged"],snapshot["jobs"]["paused"],state.origin); },
                Some("quit") | None => break,
                Some("") => {},
                Some(command) if command.starts_with("grant ")=>{let mut parts=command.splitn(3,' ');let _=parts.next();let kind=parts.next().unwrap_or("");let path=parts.next().unwrap_or("");match state.picker.grant_path(kind,path){Ok(v)=>println!("Selection: {v}"),Err(e)=>println!("Selection error: {e}")}},
                Some(_) => println!("Commands: status, pair, grant KIND /absolute/path, quit"),
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break; };
                let Ok(permit) = permits.clone().try_acquire_owned() else { drop(stream); continue; };
                let state = state.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder.keep_alive(false).max_headers(32).max_buf_size(16 * 1024)
                        .timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5));
                    let service = service_fn(move |request| handle(request, state.clone()));
                    let _ = tokio::time::timeout(Duration::from_secs(1800), builder.serve_connection(TokioIo::new(stream), service)).await;
                });
            },
        }
    }
    state.authority.lock().unwrap().disconnect();
    if let Some(broker) = broker { broker.stop().await; }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    access_restore.stop().await;
    if let Some((watcher, task)) = observer {
        watcher.stop();
        let _ = task.await;
    }
    state.downloads.cancel();
    state.power.lock().unwrap().stop();
    state.picker.revoke();
    state.worker.stop();
    if !worker_finished {
        let _ = (&mut worker_task).await;
    }
    println!("Companion stopped.");
    if worker_finished {
        return Err("The local worker stopped. Close other PhotoFindry instances using this library and check the selected runtime.".into());
    }
    Ok(())
}

async fn launch_command(args: &[String]) -> Result<(), String> {
    let installed_only = args.first().map(String::as_str) == Some("open-companion");
    if installed_only && args.len() != 1 {return Err("Open companion accepts no paths or launch options. Run the verified installer to finish setup.".into())}
    let register = args.first().map(String::as_str) == Some("register-launch");
    let archive=if args.first().map(String::as_str)==Some("open-archive") {
        Some(launch::archive_id(args.get(1).ok_or("Choose a complete PhotoFindry archive UUID.")?)?)
    } else {None};
    let (mut data, mut runtime, mut state) = (None, None, None);
    let mut options = args[if archive.is_some(){2}else{1}..].iter();
    while let Some(option) = options.next() {
        let slot = match option.as_str() {
            "--data-dir" if register => &mut data,
            "--runtime" if register => &mut runtime,
            "--launch-state" => &mut state,
            _ => return Err("Unknown local launch option. Use --help.".into()),
        };
        let value = options.next().ok_or("Every launch option requires a value.")?;
        if slot.replace(PathBuf::from(value)).is_some() { return Err("Launch options may only be supplied once.".into()); }
    }
    let custom_state = state.is_some();
    let directory = match state { Some(path) => path, None => launch::default_directory()? };
    if register {
        let executable = std::env::current_exe().map_err(|_| "Cannot locate the installed companion.")?;
        launch::register(&directory, &data.ok_or("Choose --data-dir for local registration.")?,
            &runtime.ok_or("Choose --runtime for local registration.")?, &executable)?;
        let bundle = executable.parent().ok_or("Cannot locate the installed launcher.")?.join("CompanionLauncher.app");
        if !custom_state && bundle.is_dir() {
            let status = tokio::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
                .arg("-f").arg(&bundle).status().await.map_err(|_| "Cannot register the installed Open PhotoFindry launcher.")?;
            if !status.success() { return Err("The local library was registered, but macOS could not register its launcher.".into()); }
            let status=launch::desktop_command(&launch::browser_opener(&executable)?).arg("--register-links")
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                .status().await.map_err(|_|"Cannot register PhotoFindry's local and archive links.")?;
            if !status.success(){return Err("The local library was registered, but macOS did not select this package's launcher for both link types. Another copy of the same build may still be selected. Keep this package in place and use its bin/photofindry open command to continue, then retry registration from this package.".into());}
        }
        println!("Registered this local library. Use Open PhotoFindry on app.photofindry.com or the installed CompanionLauncher.app.");
    } else {
        let executable = std::env::current_exe().map_err(|_| "Cannot locate the installed companion.")?;
        let helper = launch::browser_opener(&executable)?;
        let mut target = launch::open_registered(&directory, installed_only).await?;
        target.archive_id=archive;
        target.url()?;
        // A capability must not appear in process arguments or diagnostics.
        // The trusted native helper receives a bounded record through stdin and
        // constructs the fixed loopback URL itself with NSWorkspace.
        let mut browser = launch::desktop_command(&helper).arg("--open-local")
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()).kill_on_drop(true).spawn()
            .map_err(|_| "The installed PhotoFindry browser opener is unavailable.")?;
        let bytes = serde_json::to_vec(&target).map_err(|_| "Cannot prepare local browser opening.")?;
        let status = tokio::time::timeout(Duration::from_secs(5), async {
            use tokio::io::AsyncWriteExt;
            let mut input = browser.stdin.take().ok_or("Cannot contact the local browser opener.")?;
            input.write_all(&bytes).await.map_err(|_| "Cannot contact the local browser opener.")?;
            input.shutdown().await.map_err(|_| "Cannot complete local browser opening.")?;
            drop(input);
            browser.wait().await.map_err(|_| "Cannot open the local workspace in your browser.")
        }).await.map_err(|_| "The local browser opener did not respond.")??;
        if !status.success() { return Err("The browser did not open. Try the installed launcher again.".into()); }
    }
    Ok(())
}

async fn uninstall_command(args:&[String])->Result<(),String> {
    let mut directory=None;let mut mode=uninstall::Mode::Plan;let mut confirmation=false;
    let mut options=args.iter().skip(1);
    while let Some(option)=options.next() {
        match option.as_str() {
            "--launch-state"=>{let value=options.next().ok_or("Choose a private launch-state directory.")?;
                if directory.replace(PathBuf::from(value)).is_some(){return Err("Uninstall options may only be supplied once.".into());}},
            "--confirm"|"--interactive"=>{
                if confirmation{return Err("Choose only one uninstall confirmation mode.".into());}
                confirmation=true;mode=if option=="--confirm"{uninstall::Mode::Confirm}else{uninstall::Mode::Interactive};
            },
            _=>return Err("Unknown uninstall option. Use uninstall to review, uninstall --confirm to move the installed package to Trash, or uninstall --interactive for macOS confirmation.".into()),
        }
    }
    let directory=directory.map(Ok).unwrap_or_else(launch::default_directory)?;
    let executable=std::env::current_exe().map_err(|_|"Cannot locate the installed companion.")?;
    let outcome=uninstall::run(&directory,&executable,mode).await?;
    println!("{}",serde_json::to_string_pretty(&outcome).map_err(|_|"Cannot describe the uninstall result.")?);
    if mode==uninstall::Mode::Plan {eprintln!("Nothing changed. Saved libraries, original photos, models and settings are kept. Run uninstall --interactive for a macOS confirmation, or uninstall --confirm to move only this verified installation to Trash.");}
    Ok(())
}

#[tokio::main(worker_threads = 2)]
pub async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] || args.is_empty() {
        println!(
            "PhotoFindry macOS browser migration preview\n\nNew empty library:\n  photofindry new --data-dir /absolute/new/library --runtime /absolute/runtime\nReopen an admitted library:\n  photofindry serve --data-dir /absolute/library --runtime /absolute/runtime\nOne-time closed-app handover:\n  photofindry serve --data-dir /absolute/existing/library --runtime /absolute/runtime --handover-existing-library --rollback-copy /absolute/new/rollback\nRegister one admitted library for browser entry (once, while closed):\n  photofindry register-launch --data-dir /absolute/library --runtime /absolute/runtime\nOpen the registered library without a terminal pairing code:\n  photofindry open\nCheck a private rollback copy:\n  photofindry verify-handover --rollback-copy /absolute/rollback\n\nInstalled library launches use a fresh loopback port. Handover keeps the same directory, verifies a private cold copy, and refuses incompatible schemas or a running former host. Queues reopen paused. Never add a prototype marker to a customer library.\nDevelopment only: serve also accepts a companion-prototype.json library, or explicit --allow-existing-library, with optional --worker and --port. These bypass the production handover preparation.\nThe embedded local UI uses reviewed native actions. The public website has no library API or update authority. Requested model downloads and feedback remain explicit.\nKeep the terminal open. Type status for local state, pair to revoke access and issue a new code, or quit/Ctrl-C to stop owned processes. Installed versions use lifecycle status/check/update/recover for signed CLI updates and recovery. Use lifecycle autostart --enabled true or false to change login startup."
        );
        println!("\nReview removal of the registered companion (keeps all saved data):\n  photofindry uninstall\nNative confirmation:\n  photofindry uninstall --interactive\nTerminal confirmation:\n  photofindry uninstall --confirm\nThe installed package moves to Trash. Libraries, originals, models, settings and other installations are kept.");
        return;
    }
    let installing_cli=args.get(0).map(String::as_str)==Some("lifecycle")&&args.get(1).map(String::as_str)==Some("install");
    let result = if args.first().map(String::as_str)==Some("lifecycle") {
        lifecycle::command(&args).await
    } else if matches!(args.first().map(String::as_str),Some("migration-status"|"migrate-library"|"prepare-app-update")) {
        migration::command(&args).await
    } else if matches!(args.first().map(String::as_str), Some("open" | "open-companion" | "open-archive" | "register-launch")) {
        launch_command(&args).await
    } else if args.first().map(String::as_str)==Some("uninstall") {
        uninstall_command(&args).await
    } else if args.len() == 3 && args[0] == "verify-handover" && args[1] == "--rollback-copy" {
        handover::verify_snapshot(Path::new(&args[2])).map(|receipt| {
            println!("Verified private rollback: {}\nFiles/directories: {} · bytes: {}\nSHA-256: {}", receipt.rollback.display(), receipt.entries, receipt.bytes, receipt.sha256);
        })
    } else { match configuration(args) {
        Ok(config) => run(config).await,
        Err(error) => Err(error),
    }};
    if let Err(error) = result {
        if installing_cli{println!("{}",json!({"ok":false,"state":"blocked","message":error,"nextAction":"Choose Try again. Keep your library and recovery copy in place; use the current beta if migration cannot finish."}));}
        eprintln!("PhotoFindry: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_timeout_names_the_stage_and_preserved_recovery_path() {
        let error = startup_timeout_error("checking the photo catalog");
        assert!(error.starts_with("PF-OPEN-300:"));
        assert!(error.contains("checking the photo catalog"));
        assert!(error.contains("library and saved jobs are kept"));
        assert!(error.contains("previous verified software"));
    }
    #[tokio::test]
    async fn companion_entry_cannot_accept_state_or_library_overrides() {
        for tail in [vec!["--launch-state","/tmp/old"],vec!["--data-dir","/tmp/photos"],vec!["photofindry-local://open"]] {
            let mut args=vec!["open-companion".to_string()];args.extend(tail.into_iter().map(str::to_string));
            assert!(launch_command(&args).await.unwrap_err().contains("accepts no paths"));
        }
    }
    #[test]
    fn library_generation_headers_are_single_canonical_and_required_for_stateful_actions() {
        let mut headers=HeaderMap::new();assert_eq!(library_generation_header(&headers),None);
        for (raw,expected) in [("0",Some(0)),("12",Some(12)),("00",None),("+1",None),("-1",None),(" 1",None),("1 ",None),("18446744073709551616",None)] {
            headers.insert("x-photofindry-library-generation",raw.parse().unwrap());assert_eq!(library_generation_header(&headers),expected,"{raw}");
        }
        headers.insert("x-photofindry-library-generation","1".parse().unwrap());headers.append("x-photofindry-library-generation","1".parse().unwrap());assert_eq!(library_generation_header(&headers),None);
        for path in ["/api/action","/api/model-files/check","/api/review-files/check"] {assert!(needs_library_generation(path));}
        assert!(!needs_library_generation("/api/state"));assert!(!needs_library_generation("/api/session"));
        assert!(transition_cancel(&Action{action:"cancelTransfer".into(),params:json!({})}));
        assert!(!transition_cancel(&Action{action:"jobAction".into(),params:json!({"action":"cancelAll"})}));
    }
    #[test]
    fn cached_images_need_current_membership_and_source_access() {
        let active = json!({"id":"photo","previewOrigin":"active","source":{"mode":"connected","status":"offline","accessOriginals":false}});
        assert!(preview_access(&active, "photo", false), "Offline originals do not revoke retained previews");
        assert!(!preview_access(&active, "other", true));
        assert!(!preview_access(&Value::Null, "photo", true));
        let mut removed = active.clone(); removed["previewOrigin"] = json!("removed");
        assert!(preview_access(&removed, "photo", true), "Removed retains its explicit preview/save flow");
        assert!(!preview_access(&removed, "photo", false), "A stale activity story cannot use Removed membership");
        for mode in ["metadata", "archived"] {
            let mut blocked = active.clone(); blocked["source"]["mode"] = json!(mode);
            assert!(!preview_access(&blocked, "photo", true));
            blocked["previewOrigin"] = json!("removed");
            assert!(!preview_access(&blocked, "photo", true));
        }
    }
    #[tokio::test]
    async fn staged_transfer_publishes_after_browser_waiter_disappears() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("photofindry-transfer-owner-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join("fake-worker");
        std::fs::write(
            &executable,
            r#"#!/usr/bin/python3
import json,sys,time,pathlib
root=pathlib.Path(sys.argv[3])
if (root/'staged').exists(): (root/'published').write_text('done')
for line in sys.stdin:
    request=json.loads(line); method=request['method']; ident=request['id']
    if method=='shutdown':
        print(json.dumps({'id':ident,'result':{'stopped':True}}),flush=True)
        break
    if method=='restoreLibrary':
        (root/'received').write_text('yes')
        time.sleep(0.25)
        (root/'staged').write_text('durable')
        result={'restartRequired':True}
    else: result={'settings':{},'library':{'total':0},'photos':[]}
    print(json.dumps({'id':ident,'result':result}),flush=True)
"#,
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (worker, owner) = worker::Worker::spawn(&executable, &root, &root).unwrap();
        let transition=Arc::new(tokio::sync::RwLock::new(()));
        let guard=transition.clone().try_write_owned().unwrap();
        let browser_waiter = owned_transfer(worker.clone(), None, None, "restoreLibrary", json!({}),Some(LibraryGuard::Write{_guard:guard}));
        let until = Instant::now() + Duration::from_secs(5);
        while !root.join("received").exists() && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(root.join("received").exists());
        drop(browser_waiter); // exactly the handler's dropped JoinHandle on disconnect
        assert!(transition.clone().try_read_owned().is_err(),"Detached restore keeps mutations blocked until publication");
        while !root.join("published").exists() && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let published = root.join("published").exists();
        while transition.clone().try_read_owned().is_err() && Instant::now()<until {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(transition.try_read_owned().is_ok(),"The completed restore releases its mutation barrier");
        worker.stop();
        owner.await.unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            published,
            "Accepted transfer must finish native publication without its browser waiter"
        );
    }
    #[test]
    fn native_label_actions_do_not_accept_paths_or_browser_labels() {
        assert!(
            serde_json::from_value::<SourceLabel>(
                json!({"sourceId":"id","operation":"show","label":"forged"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<SourceScan>(json!({"mode":"image","path":"/etc/passwd"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<SourceLabel>(json!({"sourceId":"id","operation":"show"}))
                .is_ok()
        );
    }
    #[test]
    fn duplicate_companions_cannot_open_one_library() {
        let root =
            std::env::temp_dir().join(format!("photofindry-companion-lock-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let first = library_lock(&root).unwrap();
        assert!(library_lock(&root).is_err());
        drop(first);
        assert!(library_lock(&root).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn origins_hosts_and_fetch_metadata_are_exact() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:1234".parse().unwrap());
        let check =
            |h: &HeaderMap, api| boundary(h, "127.0.0.1:1234", "http://127.0.0.1:1234", api);
        assert!(check(&headers, false));
        assert!(!check(&headers, true));
        for origin in [
            "null",
            "https://photofindry.com",
            "https://app.photofindry.com",
            "http://localhost:1234",
            "http://127.0.0.1:1235",
        ] {
            headers.insert("origin", origin.parse().unwrap());
            assert!(!check(&headers, true));
            assert!(!check(&headers, false));
        }
        headers.insert("origin", "http://127.0.0.1:1234".parse().unwrap());
        assert!(check(&headers, true));
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!check(&headers, true));
        headers.remove("sec-fetch-site");
        headers.insert("host", "evil.invalid:1234".parse().unwrap());
        assert!(!check(&headers, true));
        headers.insert("host", "127.0.0.1:1234".parse().unwrap());
        headers.append("host", "127.0.0.1:1234".parse().unwrap());
        assert!(!check(&headers, true));
    }
    #[test]
    fn request_objects_are_narrow_and_projection_excludes_paths() {
        assert!(
            operations::library(
                json!({"query":"cat","method":"shutdown"}),
                &projection::Scopes::default()
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Photo>(json!({"photoId":"a","path":"/etc/passwd"})).is_err()
        );
        let root = std::env::temp_dir().join("photofindry-no-previews");
        let previews = preview::Previews::open(&root).unwrap();
        let scopes = projection::Scopes::default();
        let projected = project_photo(
            &json!({"id":"a","name":"a.jpg","path":"/private/photo","thumbnailPath":"/etc/passwd","detailSections":[{"id":"x","title":"People","items":["Two people"],"path":"/private/model"}]}),
            &previews,
            &scopes,
            true,
        );
        let text = projected.to_string();
        assert!(!text.contains("/private"));
        assert!(!text.contains("/etc"));
        assert_eq!(projected["detailSections"][0]["items"][0], "Two people");
    }
}
