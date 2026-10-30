//! Private Linux browser-companion development path. The macOS lifecycle,
//! signing feed and native helper stack are deliberately separate.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[path = "appearance.rs"]
mod appearance;
#[path = "auth.rs"]
mod auth;
#[cfg(target_os = "linux")]
#[path = "cli_release.rs"]
mod cli_release;
#[cfg(target_os = "linux")]
#[path = "platform_lifecycle.rs"]
mod lifecycle;
#[path = "linux_entry.rs"]
mod entry;
#[path = "library_restore.rs"]
mod library_restore;
#[path = "model_files.rs"]
mod model_files;
#[path = "linux_picker.rs"]
mod native_picker;
#[path = "onboarding.rs"]
mod onboarding;
#[path = "operations.rs"]
mod operations;
#[path = "preview.rs"]
mod preview;
#[path = "preview_features.rs"]
mod preview_features;
#[path = "projection.rs"]
mod projection;
#[path = "worker.rs"]
mod worker;
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
    fs::{self, OpenOptions},
    io::{BufRead, Read, Write},
    net::Ipv4Addr,
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
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
const MARKER: &str = "linux-development-library.json";
struct State {
    authority: Mutex<Authority>,
    origin: String,
    host: String,
    worker: worker::Worker,
    previews: preview::Previews,
    scopes: projection::Scopes,
    picker: native_picker::Picker,
    appearance: Mutex<appearance::Store>,
    onboarding: Mutex<onboarding::Store>,
    data: PathBuf,
    shutdown: tokio::sync::Notify,
    closing: AtomicBool,
    cleanup: AtomicBool,
    model_checks: Arc<Semaphore>,
    replacement: library_restore::Replacement,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
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
struct Pick {
    kind: String,
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Photo {
    photo_id: String,
    #[serde(default)]
    face_id: Option<String>,
    #[serde(default)]
    crop_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Activity {
    job_id: String,
    photo_id: String,
    #[serde(default)]
    kind: Option<String>,
}
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
fn release_cleanup(state: &State) {
    if state.cleanup.swap(false, Ordering::AcqRel) {
        let worker = state.worker.clone();
        tokio::spawn(async move {
            let _ = worker.call("endCleanupSession", json!({})).await;
        });
    }
}
fn unsupported() -> Reply {
    error(
        StatusCode::NOT_IMPLEMENTED,
        "This platform integration is not available in the private Linux development build. Saved work is kept.",
    )
}
fn capabilities() -> Value {
    preview_features::merge(json!({"platform":"linux","channel":"private-development","signedUpdates":true,"autostart":true,"desktopChooser":"zenity","sourceObserver":"bounded-polling","fontStories":false,"libraryReplacement":false,"faceDetection":false,"photoReview":false,"modelDownloads":false,"powerInhibition":false,"nativeUninstall":false,"sourceLabels":false,"feedback":false,"backupTransfer":false,"openOriginal":false,"metadata":false,"modelOptimization":false,"managedDeletion":true}), preview_features::current())
}
fn linux_method_supported(method: &str) -> bool {
    !matches!(
        method,
        "attachSourceArchive"
            | "exportSourceArchive"
            | "exportPhotos"
            | "importSourceArchive"
    )
}
fn preview_access(photo: &Value, id: &str, allow_removed: bool) -> bool {
    photo["id"] == id
        && (photo["previewOrigin"] == "active"
            || (allow_removed && photo["previewOrigin"] == "removed"))
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
    if let Some(map) = presentation.as_object_mut() {
        map.remove("detailCrops");
    }
    let mut result = scopes.project(&presentation);
    result["hasPreview"] = json!(
        !["metadata", "archived"].contains(&photo["source"]["mode"].as_str().unwrap_or(""))
            && previews.reference(photo, None, None).is_some()
    );
    if details {
        let accessible =
            !["metadata", "archived"].contains(&photo["source"]["mode"].as_str().unwrap_or(""));
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
fn operation_error(message: &str) -> Reply {
    // Paths in native diagnostics are presentation, never a new capability.
    let safe = message
        .split_whitespace()
        .map(|word| {
            // Restored Windows locations (`C:\...`) are paths too.
            if word.contains(":\\") {
                word.rsplit('\\').next().filter(|s| !s.is_empty()).unwrap_or("selected file")
            } else if word.starts_with('/') {
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
async fn api(path: &str, bytes: &[u8], state: &Arc<State>) -> Reply {
    let invalid = || error(StatusCode::BAD_REQUEST, "Invalid request fields.");
    if state.closing.load(Ordering::Acquire) {
        return operation_error("The companion is closing. Reopen it after shutdown finishes.");
    }
    if state.replacement.replacing() && !library_restore::Replacement::allowed_while_replacing(path) {
        return operation_error("The restored library is opening. Try again in a moment.");
    }
    match path {
        "/api/pair" => {
            let Ok(input) = serde_json::from_slice::<Pair>(bytes) else {
                return invalid();
            };
            if input.code.len() != 64 {
                return invalid();
            }
            let mut a = state.authority.lock().unwrap();
            match a.pair(&input.code, Instant::now()) {
                Ok(token) => json_response(
                    json!({"token":token,"apiVersion":4,"instanceId":a.instance_id(),"sessionLifetime":"process"}),
                ),
                Err(e) => error(StatusCode::UNAUTHORIZED, e),
            }
        }
        "/api/bootstrap" => {
            let Ok(input) = serde_json::from_slice::<Bootstrap>(bytes) else {
                return invalid();
            };
            if input.ticket.len() != 64 {
                return invalid();
            }
            let mut a = state.authority.lock().unwrap();
            match a.exchange_bootstrap(&input.ticket, Instant::now()) {
                Ok(token) => json_response(
                    json!({"token":token,"apiVersion":4,"instanceId":a.instance_id(),"sessionLifetime":"process"}),
                ),
                Err(e) => error(StatusCode::UNAUTHORIZED, e),
            }
        }
        "/api/session" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            let a = state.authority.lock().unwrap();
            json_response(
                json!({"apiVersion":4,"instanceId":a.instance_id(),"sessionLifetime":"process"}),
            )
        }
        "/api/disconnect" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            state.authority.lock().unwrap().disconnect();
            state.picker.revoke();
            release_cleanup(state);
            json_response(json!({"disconnected":true}))
        }
        "/api/quit" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            state.closing.store(true, Ordering::Release);
            state.authority.lock().unwrap().disconnect();
            state.shutdown.notify_one();
            json_response(json!({"stopping":true}))
        }
        "/api/capabilities" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            json_response(capabilities())
        }
        "/api/lifecycle/status" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            #[cfg(target_os = "linux")]
            {match lifecycle::web_status(&state.data).await {Ok(value)=>json_response(value),Err(reason)=>operation_error(&reason)}}
            #[cfg(not(target_os = "linux"))]
            {unsupported()}
        }
        "/api/uninstall/plan" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            json_response(
                json!({"available":false,"reason":"Use the private Linux package uninstall command; saved libraries and model files are kept."}),
            )
        }
        "/api/state" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            if state.replacement.replacing() {
                return json_response(state.replacement.opening_snapshot());
            }
            let mut snapshot = state.worker.snapshot();
            snapshot["libraryGeneration"] = json!(state.replacement.generation());
            snapshot["platformCapabilities"] = capabilities();
            snapshot["sourceObserver"] = json!({"available":true,"provider":"bounded-polling"});
            snapshot["powerPreferences"] = json!({"mode":"allowSleep","available":false});
            snapshot["download"] = json!({"running":false,"available":false});
            json_response(state.scopes.project_source_locations(&snapshot))
        }
        "/api/monitor" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            let s = state.worker.snapshot();
            json_response(state.scopes.project(&json!({"workerGeneration":s["workerGeneration"],"monitor":s["monitor"],"runtime":s["runtime"]})))
        }
        "/api/library-info" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            let s = state.worker.snapshot();
            json_response(
                json!({"libraryId":s["libraryId"],"libraryGeneration":state.replacement.generation(),"locationLabel":state.data,"ready":s["libraryId"].is_string()&&s["loading"]!=true&&s["libraryError"].is_null(),"error":s["libraryError"]}),
            )
        }
        "/api/appearance" => {
            if serde_json::from_slice::<Empty>(bytes).is_err() {
                return invalid();
            }
            match state.appearance.lock().unwrap().snapshot() {
                Ok(v) => json_response(v),
                Err(e) => operation_error(&e),
            }
        }
        "/api/appearance/update" => {
            let Ok(input) = serde_json::from_slice::<appearance::Update>(bytes) else {
                return invalid();
            };
            match state.appearance.lock().unwrap().update(input.mode) {
                Ok(v) => json_response(v),
                Err(e) => operation_error(&e),
            }
        }
        "/api/support" => {
            if !serde_json::from_slice::<Value>(bytes).is_ok_and(|v| v.is_object()) {
                return invalid();
            }
            json_response(json!({"showInvitation":false,"handled":true}))
        }
        "/api/onboarding" | "/api/onboarding/update" | "/api/onboarding/reset" => {
            let snapshot = state.worker.snapshot();
            let Some(id) = snapshot["libraryId"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 128)
            else {
                return operation_error("Wait for the local library to finish opening.");
            };
            let result = if path == "/api/onboarding" {
                if serde_json::from_slice::<Empty>(bytes).is_err() {
                    return invalid();
                }
                state.onboarding.lock().unwrap().snapshot(id)
            } else if path == "/api/onboarding/reset" {
                let Ok(input) = serde_json::from_slice::<GuideReset>(bytes) else {
                    return invalid();
                };
                if input.library_id != id {
                    return operation_error("The library changed. Reopen the guide.");
                }
                state.onboarding.lock().unwrap().reset(id)
            } else {
                let Ok(input) = serde_json::from_slice::<GuideUpdate>(bytes) else {
                    return invalid();
                };
                if input.library_id != id {
                    return operation_error("The library changed. Reopen the guide.");
                }
                state.onboarding.lock().unwrap().update(id, input.changes)
            };
            match result {
                Ok(v) => json_response(v),
                Err(e) => operation_error(&e),
            }
        }
        "/api/pick" => {
            let Ok(input) = serde_json::from_slice::<Pick>(bytes) else {
                return invalid();
            };
            match state.picker.pick(&input.kind).await {
                Ok(v) => json_response(v),
                Err(e) => operation_error(&e),
            }
        }
        "/api/model-files/check" => {
            let Ok(input) = serde_json::from_slice::<Value>(bytes) else {
                return invalid();
            };
            let (_, changes) =
                match operations::prepare("configure", input, &state.scopes, &state.picker) {
                    Ok(v) => v,
                    Err(e) => return operation_error(&e),
                };
            let mut settings = state.worker.snapshot()["settings"].clone();
            if !settings.is_object() {
                return operation_error("Wait for model settings to finish opening.");
            }
            for (k, v) in changes.as_object().unwrap() {
                settings[k] = v.clone();
            }
            let Ok(permit) = state.model_checks.clone().try_acquire_owned() else {
                return operation_error("A model file check is already running.");
            };
            match tokio::task::spawn_blocking(move || {
                let _permit = permit;
                model_files::check(&settings)
            })
            .await
            {
                Ok(v) => json_response(v),
                Err(_) => operation_error("The model file check stopped unexpectedly."),
            }
        }
        "/api/action" => {
            let Ok(input) = serde_json::from_slice::<Action>(bytes) else {
                return invalid();
            };
            if matches!(
                input.action.as_str(),
                "feedbackDiagnostics"
                    | "feedbackEnvironment"
                    | "cancelFeedback"
                    | "prepareFeedbackScreenshot"
                    | "sendFeedback"
                    | "sourceLabel"
                    | "sourceLabelScan"
                    | "startDownload"
                    | "cancelDownload"
                    | "configurePower"
                    | "openOriginal"
                    | "revealPhoto"
                    | "revealSourceCandidate"
                    | "revealSource"
                    | "revealMemoryLogs"
                    | "revealTransferResult"
                    | "revealLibrary"
                    | "optimizeModel"
            ) || preview_features::action_unavailable(&input.action, &input.params, preview_features::current())
            {
                return unsupported();
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
            if !linux_method_supported(method) {
                return unsupported();
            }
            if method == "configurePhotoReview" {
                if let Some(message) = preview_features::review_settings_error(&params) {
                    return operation_error(message);
                }
            }
            let limit = params["limit"].as_u64().unwrap_or(60);
            let completed = if method == "restoreLibrary" {
                // The restart that publishes a staged restore belongs to the
                // accepted action, not to this HTTP request.
                let owner = state.clone();
                tokio::spawn(async move {
                    let (call, restart) = (owner.worker.clone(), owner.worker.clone());
                    library_restore::run(
                        async move { call.call("restoreLibrary", params).await },
                        async move || {
                            restart.restart().await?;
                            Ok(restart.snapshot())
                        },
                        &owner.replacement,
                        || {
                            owner.scopes.reset();
                            owner.picker.revoke();
                            release_cleanup(&owner);
                        },
                    )
                    .await
                })
                .await
                .unwrap_or_else(|_| Err("The local restore owner stopped unexpectedly.".into()))
            } else {
                state.worker.call(method, params).await
            };
            match completed {
                Ok(mut v) => {
                    if ["jobResults", "aiSearchPage"].contains(&method) {
                        v["limit"] = json!(limit);
                    }
                    if method == "beginCleanupSession" {
                        state.cleanup.store(true, Ordering::Release);
                    }
                    if method == "endCleanupSession" {
                        state.cleanup.store(false, Ordering::Release);
                    }
                    json_response(
                        if ["sourceDetails", "prepareSourceConnection", "sourceConnectionReview"].contains(&method) {
                            state.scopes.project_source_locations(&v)
                        } else {
                            public_result(v, state)
                        },
                    )
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
                || input
                    .crop_id
                    .as_ref()
                    .is_some_and(|id| id.is_empty() || id.len() > 128 || id.contains('\0'))
                || (input.face_id.is_some() && input.crop_id.is_some())
                || (path == "/api/photo" && (input.face_id.is_some() || input.crop_id.is_some()))
            {
                return invalid();
            }
            let mut params = json!({"photoId":input.photo_id});
            if path == "/api/preview" {
                if let Some(face_id) = input.face_id.as_ref() {
                    params["faceId"] = json!(face_id);
                }
                if let Some(crop_id) = input.crop_id.as_ref() {
                    params["cropId"] = json!(crop_id);
                }
            }
            let photo = state
                .worker
                .call(
                    if path == "/api/preview" {
                        "previewDetails"
                    } else {
                        "photoDetails"
                    },
                    params,
                )
                .await;
            let photo = match photo {
                Ok(photo) => photo,
                Err(e) => return operation_error(&e),
            };
            if path == "/api/preview" && photo.is_null() {
                return error(
                    StatusCode::NOT_FOUND,
                    "This photo no longer has preview access.",
                );
            }
            if photo["id"] != input.photo_id {
                return error(StatusCode::BAD_GATEWAY, "Invalid local photo response.");
            }
            if path == "/api/photo" {
                return json_response(project_photo(&photo, &state.previews, &state.scopes, true));
            }
            if !preview_access(&photo, &input.photo_id, true) {
                return error(
                    StatusCode::NOT_FOUND,
                    "Cached images are disconnected for this source.",
                );
            }
            let Some(file) = state.previews.reference(
                &photo,
                input.face_id.as_deref(),
                input.crop_id.as_deref(),
            ) else {
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
            let access = match state
                .worker
                .call("previewDetails", json!({"photoId":input.photo_id}))
                .await
            {
                Ok(access) => access,
                Err(e) => return operation_error(&e),
            };
            if !preview_access(&access, &input.photo_id, false) {
                return error(
                    StatusCode::NOT_FOUND,
                    "The current prepared photo is no longer available.",
                );
            }
            let current = state.worker.snapshot();
            if current[key]["jobID"] != input.job_id
                || current[key]["currentPhoto"] != story["currentPhoto"]
            {
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
            let Ok(v) = serde_json::from_slice::<Value>(bytes) else {
                return invalid();
            };
            if !v.is_object() {
                return invalid();
            }
            json_response(
                json!({"side":16,"beats":[[],[],[]],"available":false,"reason":"Linux font story rendering is not qualified."}),
            )
        }
        "/api/lifecycle/action" => {
            #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Maintenance {action:String}
            let Ok(input)=serde_json::from_slice::<Maintenance>(bytes)else{return invalid();};
            #[cfg(target_os = "linux")]
            {match lifecycle::schedule(&input.action,&state.data){Ok(())=>json_response(json!({"started":true})),Err(reason)=>operation_error(&reason)}}
            #[cfg(not(target_os = "linux"))]
            {let _=input;unsupported()}
        }
        "/api/uninstall/open" | "/api/source-candidate-preview" => unsupported(),
        "/api/review-files/check" => {
            let Ok(input) = serde_json::from_slice::<Value>(bytes) else {
                return invalid();
            };
            match operations::prepare("configurePhotoReview", input, &state.scopes, &state.picker) {
                Ok((_, changes)) => json_response(preview_features::review_check(&changes)),
                Err(e) => operation_error(&e),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "Unknown endpoint."),
    }
}
fn asset(path: &str, state: &State) -> Reply {
    return match path {
        "/" => response(
            StatusCode::OK,
            "text/html; charset=utf-8",
            appearance::html(
                include_str!("../../../web-companion/index.html"),
                state.appearance.lock().unwrap().mode().unwrap_or_default(),
            )
            .replacen(
                "<html lang=\"en\"",
                "<html lang=\"en\" data-platform=\"linux\"",
                1,
            )
            .replace("your Mac", "your computer")
            .replace("this Mac", "this computer")
            .replace("macOS development branch", "Linux preview"),
        ),
        "/ui-framework.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/ui-framework.js")),
        ),
        "/ui-framework.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/ui-framework.css")),
        ),
        "/ui-framework.NOTICES.txt" => response(
            StatusCode::OK,
            "text/plain; charset=utf-8",
            Bytes::from_static(include_bytes!(
                "../../../web-companion/ui-framework.NOTICES.txt"
            )),
        ),
        "/photo-presentation.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
            Bytes::from_static(include_bytes!(
                "../../../web-companion/photo-presentation.css"
            )),
        ),
        "/product-theme.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/product-theme.css")),
        ),
        "/design-system.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
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
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/model-monitor.js")),
        ),
        "/model-monitor.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/model-monitor.css")),
        ),
        "/onboarding.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/onboarding.js")),
        ),
        "/support.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            Bytes::from_static(include_bytes!("../../../web-companion/support.js")),
        ),
        "/support-qr.png" => response(
            StatusCode::OK,
            "image/png",
            Bytes::from_static(include_bytes!("../../../Resources/CoffeeSupportQR.png")),
        ),
        "/onboarding.css" => response(
            StatusCode::OK,
            "text/css; charset=utf-8",
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
        "/assets/untagged-landscape.jpg" => response(
            StatusCode::OK,
            "image/jpeg",
            Bytes::from_static(include_bytes!(
                "../../../web-companion/assets/untagged-landscape.jpg"
            )),
        ),
        "/assets/untagged-spaces.jpg" => response(
            StatusCode::OK,
            "image/jpeg",
            Bytes::from_static(include_bytes!(
                "../../../web-companion/assets/untagged-spaces.jpg"
            )),
        ),
        "/assets/untagged-shapes.jpg" => response(
            StatusCode::OK,
            "image/jpeg",
            Bytes::from_static(include_bytes!(
                "../../../web-companion/assets/untagged-shapes.jpg"
            )),
        ),
        _ => error(StatusCode::NOT_FOUND, "Unknown local asset."),
    };
}
/// The browser must name the library it last observed. A restore advances
/// the generation, so stale tabs cannot act on the replaced library.
fn generation_header(headers: &HeaderMap, current: u64) -> bool {
    one_header(headers, "x-photofindry-library-generation") == Some(current.to_string().as_str())
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
        return Ok(if request.method() == Method::GET {
            asset(&path, &state)
        } else {
            error(StatusCode::METHOD_NOT_ALLOWED, "Use GET for local assets.")
        });
    }
    if request.method() != Method::POST {
        return Ok(error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Use POST for local requests.",
        ));
    }
    let pairing = matches!(path.as_str(), "/api/pair" | "/api/bootstrap");
    if !pairing && !authorized(request.headers(), &state) {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "Pair with this companion first.",
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
    let headers = request.headers().clone();
    if matches!(
        path.as_str(),
        "/api/action" | "/api/model-files/check" | "/api/review-files/check"
    ) && !generation_header(&headers, state.replacement.generation())
    {
        return Ok(error(
            StatusCode::CONFLICT,
            "Refresh the current library generation before this action.",
        ));
    }
    let max = if path == "/api/action" {
        12 * 1024 * 1024
    } else {
        MAX_BODY
    };
    let body = match tokio::time::timeout(
        Duration::from_secs(10),
        Limited::new(request.into_body(), max).collect(),
    )
    .await
    {
        Ok(Ok(body)) => body.to_bytes(),
        Ok(Err(_)) => {
            return Ok(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Request body exceeds its limit.",
            ));
        }
        Err(_) => {
            return Ok(error(
                StatusCode::REQUEST_TIMEOUT,
                "Request body timed out.",
            ));
        }
    };
    if !pairing && !authorized(&headers, &state) {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "The local session was revoked.",
        ));
    }
    let reply = api(&path, &body, &state).await;
    if !matches!(
        path.as_str(),
        "/api/pair" | "/api/bootstrap" | "/api/disconnect" | "/api/quit"
    ) && !authorized(&headers, &state)
    {
        return Ok(error(
            StatusCode::UNAUTHORIZED,
            "The local session was revoked.",
        ));
    }
    Ok(reply)
}

struct Config {
    data: PathBuf,
    runtime: PathBuf,
    worker: PathBuf,
    port: u16,
    new: bool,
    open: bool,
    no_stdin: bool,
    launch_state: Option<PathBuf>,
}
fn directory(raw: &str) -> Result<PathBuf, String> {
    let p = Path::new(raw);
    if !p.is_absolute() || fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Use an absolute ordinary directory, without a symlink root.".into());
    }
    let p = fs::canonicalize(p).map_err(|_| "The selected directory is unavailable.")?;
    if !p.is_dir() {
        return Err("The selected location is not a directory.".into());
    }
    Ok(p)
}
fn default_data() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            return Ok(p.join("photofindry/Library"));
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("HOME must name an absolute directory.")?;
    Ok(home.join(".local/share/photofindry/Library"))
}
fn configuration(args: Vec<String>) -> Result<Config, String> {
    let new = args.first().is_some_and(|v| v == "new");
    if !new && !args.first().is_some_and(|v| v == "serve") {
        return Err("Expected new or serve. Use --help.".into());
    }
    let (mut data, mut runtime, mut worker, mut port, mut open, mut no_stdin) =
        (None, None, None, None, false, false);
    let mut launch_state = None;
    let mut seen = std::collections::HashSet::new();
    let mut options = args[1..].iter();
    while let Some(key) = options.next() {
        if !seen.insert(key) {
            return Err("Duplicate option.".into());
        }
        match key.as_str() {
            "--open" => open = true,
            "--no-stdin" => no_stdin = true,
            "--no-browser" => open = false,
            "--launch-state" => {
                let path = PathBuf::from(options.next().ok_or("Missing launch-state directory.")?);
                if !path.is_absolute() {
                    return Err("Use an absolute launch-state directory.".into());
                }
                launch_state = Some(path);
            }
            "--data-dir" => {
                data = Some(PathBuf::from(
                    options.next().ok_or("Missing data directory.")?,
                ))
            }
            "--runtime" => {
                runtime = Some(directory(
                    options.next().ok_or("Missing runtime directory.")?,
                )?)
            }
            "--worker" => {
                worker = Some(PathBuf::from(
                    options.next().ok_or("Missing worker executable.")?,
                ))
            }
            "--port" => {
                port = Some(
                    options
                        .next()
                        .ok_or("Missing port.")?
                        .parse::<u16>()
                        .map_err(|_| "Invalid port.")?,
                )
            }
            _ => return Err("Unknown option. Use --help.".into()),
        }
    }
    if seen.contains(&"--open".to_owned()) && seen.contains(&"--no-browser".to_owned()) {
        return Err("Choose either --open or --no-browser.".into());
    }
    let data = data.map(Ok).unwrap_or_else(default_data)?;
    if !data.is_absolute() {
        return Err("Use an absolute library path.".into());
    }
    let runtime = runtime.ok_or("Choose the private Linux runtime with --runtime.")?;
    let worker = worker.unwrap_or_else(|| runtime.join("PhotoWorker"));
    if !worker.is_absolute()
        || !worker.is_file()
        || fs::symlink_metadata(&worker).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Choose the ordinary Linux PhotoWorker executable.".into());
    }
    Ok(Config {
        data,
        runtime,
        worker,
        port: port.unwrap_or(0),
        new,
        open,
        no_stdin,
        launch_state,
    })
}
/// Create and mark a fresh private library directory; used by `new` and the
/// signed installer. Never applied to an existing directory.
fn create_library(data: &Path) -> Result<(), String> {
    if fs::symlink_metadata(data).is_ok() {
        return Err(
            "A new development library requires a new directory. Existing data is kept.".into(),
        );
    }
    let parent = data
        .parent()
        .ok_or("The library has no parent directory.")?;
    if !parent.is_dir() {
        return Err("Create the library parent directory first.".into());
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(data)
        .map_err(|_| "Cannot create the private Linux library.")?;
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(data.join(MARKER))
        .map_err(|_| "Cannot mark the new Linux development library.")?;
    marker
        .write_all(
            b"{\"version\":1,\"platform\":\"linux\",\"channel\":\"private-development\"}\n",
        )
        .map_err(|_| "Cannot record the Linux development library.")?;
    marker
        .sync_all()
        .map_err(|_| "Cannot save the library marker.")?;
    Ok(())
}
fn prepare_library(config: &Config) -> Result<PathBuf, String> {
    if config.new {
        create_library(&config.data)?;
    }
    let data = directory(config.data.to_str().ok_or("Library path is not UTF-8.")?)?;
    let meta = fs::metadata(&data).map_err(|_| "Cannot inspect the library.")?;
    if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(
            "The Linux development library must be owned by this user and private (mode 0700)."
                .into(),
        );
    }
    let marker=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(data.join(MARKER)).map_err(|_|"This directory is not a Linux development library. Use new with an isolated directory; do not add a marker to a customer library.")?;
    let meta = marker
        .metadata()
        .map_err(|_| "Cannot inspect the library marker.")?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() > 4096
    {
        return Err("Invalid Linux development library marker.".into());
    }
    let mut bytes = Vec::new();
    marker
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read the library marker.")?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid Linux development library marker.")?;
    if value != json!({"version":1,"platform":"linux","channel":"private-development"}) {
        return Err("Unsupported Linux development library marker.".into());
    }
    Ok(data)
}
fn library_lock(data: &Path) -> Result<std::fs::File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(data.join(".companion.lock"))
        .map_err(|_| "Cannot acquire this library's companion lock.")?;
    let m = file
        .metadata()
        .map_err(|_| "Cannot inspect the library lock.")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
        || unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err("Another companion owns this library, or its private lock is invalid.".into());
    }
    Ok(file)
}
fn observation_roots(snapshot: &Value) -> Vec<String> {
    let mut roots = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    for source in snapshot["sources"].as_array().into_iter().flatten() {
        if source["automaticLifecycleVersion"] != 1
            || source["accessOriginals"] == false
            || matches!(source["mode"].as_str(), Some("metadata" | "archived"))
        {
            continue;
        }
        for root in source["roots"].as_array().into_iter().flatten() {
            if root["active"] == false {
                continue;
            }
            if let Some(path) = root["path"]
                .as_str()
                .filter(|p| p.starts_with('/') && p.len() <= 16384 && !p.contains('\0'))
            {
                if !roots.contains(path) {
                    if bytes + path.len() > 4 * 1024 * 1024 {
                        return roots.into_iter().collect();
                    }
                    bytes += path.len();
                    roots.insert(path.to_owned());
                }
                if roots.len() >= 20000 {
                    return roots.into_iter().collect();
                }
            }
        }
    }
    roots.into_iter().collect()
}
async fn observe_sources(worker: worker::Worker) {
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut cursor = 0;
    loop {
        ticker.tick().await;
        let roots = observation_roots(&worker.snapshot());
        if roots.is_empty() {
            continue;
        }
        let mut batch = Vec::new();
        for _ in 0..roots.len().min(8) {
            batch.push(roots[cursor % roots.len()].clone());
            cursor = (cursor + 1) % roots.len();
        }
        let _ = tokio::time::timeout(
            Duration::from_secs(10),
            worker.call("observeSources", json!({"paths":batch,"automatic":true})),
        )
        .await;
    }
}
fn unsupported_jobs(snapshot: &Value) -> Vec<String> {
    snapshot["jobs"]["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|job| {
            matches!(
                job["kind"].as_str(),
                Some(kind) if preview_features::job_unavailable(kind, preview_features::current())
            ) && matches!(
                job["state"].as_str(),
                Some("queued" | "running" | "paused" | "pausing")
            )
        })
        .filter_map(|job| {
            job["id"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 128)
                .map(str::to_owned)
        })
        .take(100)
        .collect()
}
async fn apply_linux_capabilities(
    worker: &worker::Worker,
    initial: Value,
) -> Result<Value, String> {
    // The admission marker limits this policy to explicit Linux development
    // libraries. These RPCs change future platform work, never accepted facts,
    // names, originals or supported jobs. macOS does not call this function.
    if !preview_features::current().faces && initial["facePreferences"]["enabled"] != false {
        worker
            .call("configureFaceScan", json!({"enabled":false}))
            .await?;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let library = initial["libraryId"].clone();
    for _ in 0..100 {
        let snapshot = worker.call("state", json!({})).await?;
        if snapshot["libraryId"] != library {
            return Err("The Linux development library identity changed while checking platform capabilities.".into());
        }
        let ids = unsupported_jobs(&snapshot);
        if ids.is_empty() {
            return Ok(snapshot);
        }
        for id in ids {
            worker
                .call("jobAction", json!({"id":id,"action":"cancel"}))
                .await?;
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Unsupported work is still stopping in this Linux preview. Supported jobs and all saved progress are kept; reopen after it drains.".into())
}
async fn run(config: Config) -> Result<(), String> {
    preview_features::init(&config.runtime);
    let data = prepare_library(&config)?;
    let _ownership = library_lock(&data)?;
    let authority = Authority::new().map_err(str::to_owned)?;
    let previews = preview::Previews::open(&data)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))
        .await
        .map_err(|_| "Cannot bind the loopback companion port.")?;
    let host = format!(
        "127.0.0.1:{}",
        listener
            .local_addr()
            .map_err(|_| "Cannot inspect the local port.")?
            .port()
    );
    let mut terminated = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "Cannot register Linux shutdown.")?;
    let mut hung_up = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|_| "Cannot register terminal shutdown.")?;
    let (worker, mut worker_task) = worker::Worker::spawn(&config.worker, &data, &config.runtime)?;
    let ready = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(300),worker.call("state",json!({})))=>result,
        _=tokio::signal::ctrl_c()=>{worker.stop();let _=worker_task.await;return Err("Library opening cancelled. Saved work is kept.".into());},
        _=terminated.recv()=>{worker.stop();let _=worker_task.await;return Err("Library opening stopped. Saved work is kept.".into());},
        _=hung_up.recv()=>{worker.stop();let _=worker_task.await;return Err("The terminal closed while opening. Saved work is kept.".into());},
    };
    let initial = match ready {
        Ok(Ok(v))
            if v["libraryId"].as_str().is_some_and(|s| !s.is_empty())
                && v["loading"] != true
                && v["libraryError"].is_null() =>
        {
            v
        }
        other => {
            let stage = worker.startup_stage();
            worker.stop();
            let _ = worker_task.await;
            return Err(match other {
                Ok(Err(e)) => {
                    format!("The local worker failed while {stage}: {e}. Saved work is kept.")
                }
                _ => format!(
                    "The Linux library did not finish opening while {stage}. Saved work is kept."
                ),
            });
        }
    };
    let initial = match apply_linux_capabilities(&worker, initial).await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            worker.stop();
            let _ = worker_task.await;
            return Err(format!(
                "The Linux platform setup could not finish: {e}. Saved work is kept."
            ));
        }
    };
    let state = Arc::new(State {
        authority: Mutex::new(authority),
        origin: format!("http://{host}"),
        host,
        worker,
        previews,
        scopes: projection::Scopes::default(),
        picker: native_picker::Picker::new(&config.runtime),
        appearance: Mutex::new(appearance::Store::new(&data)),
        onboarding: Mutex::new(onboarding::Store::new(&data)),
        data,
        shutdown: tokio::sync::Notify::new(),
        closing: AtomicBool::new(false),
        cleanup: AtomicBool::new(false),
        model_checks: Arc::new(Semaphore::new(1)),
        replacement: library_restore::Replacement::default(),
    });
    let code = state
        .authority
        .lock()
        .unwrap()
        .issue(Instant::now())
        .map_err(str::to_owned)?;
    let local_entry = if let Some(directory) = config.launch_state.as_ref() {
        let instance = state.authority.lock().unwrap().instance_id().to_owned();
        let owner = state.clone();
        let mint = Arc::new(move || {
            if owner.closing.load(Ordering::Acquire) {
                return Err(
                    "The Linux companion is closing. Open it again after shutdown finishes."
                        .to_owned(),
                );
            }
            let snapshot = owner.worker.snapshot();
            if snapshot["libraryId"].is_null()
                || snapshot["loading"] == true
                || !snapshot["libraryError"].is_null()
            {
                return Err("The current library is not ready. Saved work is kept.".to_owned());
            }
            owner
                .authority
                .lock()
                .unwrap()
                .issue_bootstrap(Instant::now())
                .map_err(str::to_owned)
        });
        let port = listener
            .local_addr()
            .map_err(|_| "Cannot inspect the loopback port.")?
            .port();
        match entry::Entry::start(
            directory,
            port,
            &instance,
            initial["libraryId"].as_str().unwrap(),
            mint,
        )
        .await
        {
            Ok(entry) => Some(entry),
            Err(e) => {
                state.worker.stop();
                let _ = worker_task.await;
                return Err(e);
            }
        }
    } else {
        None
    };
    println!(
        "{}",
        json!({"event":"ready","platform":"linux","channel":"private-development","url":state.origin,"pairingCode":code,"libraryId":initial["libraryId"]})
    );
    if std::io::stdout().flush().is_err() {
        state.closing.store(true, Ordering::Release);
        if let Some(entry) = local_entry {
            entry.stop().await;
        }
        state.worker.stop();
        let _ = worker_task.await;
        return Err(
            "Cannot write the local launch status. This attempt stopped; saved work is kept."
                .into(),
        );
    }
    if config.open {
        let ticket = state
            .authority
            .lock()
            .unwrap()
            .issue_bootstrap(Instant::now())
            .map_err(str::to_owned)?;
        let url = format!("{}/#bootstrap={ticket}", state.origin);
        match tokio::process::Command::new("/usr/bin/xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
            Err(_) => eprintln!(
                "The Linux browser opener is unavailable. Open the printed loopback URL and pair using the code."
            ),
        }
    }
    let observer = tokio::spawn(observe_sources(state.worker.clone()));
    let (commands, mut command_rx) = mpsc::channel::<String>(4);
    let _keep_stdin = config.no_stdin.then(|| commands.clone());
    if !config.no_stdin {
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                let mut bytes = Vec::new();
                match (&mut input).take(32769).read_until(b'\n', &mut bytes) {
                    Ok(0) | Err(_) => {
                        let _ = commands.blocking_send("quit".into());
                        break;
                    }
                    Ok(_) if bytes.len() > 32768 => {
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
        });
    }
    let permits = Arc::new(Semaphore::new(16));
    let mut connections = JoinSet::new();
    let mut worker_finished = false;
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>break,_=terminated.recv()=>break,_=hung_up.recv()=>break,
            _=state.shutdown.notified()=>{tokio::time::sleep(Duration::from_millis(100)).await;break;},
            _=&mut worker_task=>{worker_finished=true;eprintln!("The owned Linux worker stopped. Closing the companion; saved work is kept.");break;},
            command=command_rx.recv()=>match command.as_deref(){Some("quit")|None=>break,Some("status")=>println!("{}",state.scopes.project(&state.worker.snapshot())),Some("pair")=>{state.picker.revoke();release_cleanup(&state);let code=state.authority.lock().unwrap().issue(Instant::now()).map_err(str::to_owned)?;println!("Pairing code: {code}");},Some(command)if command.starts_with("grant ")=>{let mut parts=command.splitn(3,' ');let _=parts.next();match state.picker.grant_path(parts.next().unwrap_or(""),parts.next().unwrap_or("")){Ok(v)=>println!("Selection: {v}"),Err(e)=>println!("Selection error: {e}")}},Some("")=>{},Some(_)=>println!("Commands: status, pair, grant KIND /absolute/path, quit")},
            Some(_)=connections.join_next(),if !connections.is_empty()=>{},
            accepted=listener.accept()=>{let Ok((stream,_))=accepted else{break;};let Ok(permit)=permits.clone().try_acquire_owned()else{drop(stream);continue;};let state=state.clone();connections.spawn(async move{let _permit=permit;let mut builder=hyper::server::conn::http1::Builder::new();builder.keep_alive(false).max_headers(32).max_buf_size(16*1024).timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5));let service=service_fn(move|r|handle(r,state.clone()));let _=tokio::time::timeout(Duration::from_secs(1800),builder.serve_connection(TokioIo::new(stream),service)).await;});}
        }
    }
    state.closing.store(true, Ordering::Release);
    state.authority.lock().unwrap().disconnect();
    state.picker.revoke();
    if let Some(entry) = local_entry {
        entry.stop().await;
    }
    observer.abort();
    let _ = observer.await;
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    if state.cleanup.swap(false, Ordering::AcqRel) {
        let _ = state.worker.call("endCleanupSession", json!({})).await;
    }
    state.worker.stop();
    if !worker_finished {
        let _ = worker_task.await;
    }
    if worker_finished {
        Err("The owned Linux worker stopped unexpectedly. Saved work is kept; inspect the selected runtime before reopening.".into())
    } else {
        Ok(())
    }
}
async fn open_command(args: &[String]) -> Result<(), String> {
    let mut path = None;
    let mut no_browser = false;
    let mut options = args[1..].iter();
    let mut seen = std::collections::HashSet::new();
    while let Some(option) = options.next() {
        if !seen.insert(option) {
            return Err("Duplicate open option.".into());
        }
        match option.as_str() {
            "--launch-state" => {
                path = Some(PathBuf::from(
                    options.next().ok_or("Missing launch-state directory.")?,
                ))
            }
            "--no-browser" => no_browser = true,
            _ => return Err(
                "Open accepts --launch-state /absolute/private/state and optional --no-browser."
                    .into(),
            ),
        }
    }
    #[cfg(target_os = "linux")]
    let path=match path{Some(path)=>path,None=>lifecycle::default_open().await?};
    #[cfg(not(target_os = "linux"))]
    let path=path.ok_or("Choose the private package's --launch-state directory.")?;
    let response=entry::open(&path).await?;
    if no_browser {
        println!("{}", response);
        return Ok(());
    }
    let url = response["url"]
        .as_str()
        .ok_or("The local entry did not return a browser ticket.")?;
    let mut child=tokio::process::Command::new("/usr/bin/xdg-open").arg(url).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The browser opener is unavailable. The current companion is still healthy. Use open --no-browser to obtain a local launch URL.")?;
    match tokio::time::timeout(Duration::from_secs(15),child.wait()).await{Ok(Ok(status))if status.success()=>{println!("{}",json!({"opened":true,"platform":"linux","libraryId":response["libraryId"]}));Ok(())},_=>Err("The Linux browser did not open. The current companion remains running. Use open --no-browser for a local launch URL.".into())}
}
#[tokio::main(worker_threads = 2)]
pub async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--capabilities"] {
        println!(
            "{}",
            json!({"capabilities": capabilities(), "commands":["new","serve","open","lifecycle","uninstall"], "options":["--data-dir","--runtime","--worker","--port","--open","--no-browser","--no-stdin","--launch-state"], "terminalCommands":["status","pair","grant KIND /absolute/path","quit"]})
        );
        return;
    }
    if args.first().is_some_and(|name| name == "open") {
        let result = open_command(&args).await;
        if let Err(e) = result {
            eprintln!("PhotoFindry Linux: {e}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(target_os = "linux")]
    if args.first().is_some_and(|name| lifecycle::handles(name)) {
        if let Err(e) = lifecycle::command(&args).await {
            eprintln!("PhotoFindry Linux: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().is_some_and(|name| {
        matches!(
            name.as_str(),
            "migration-status"
                | "migrate-library"
                | "prepare-app-update"
                | "register-launch"
                | "open-companion"
        )
    }) {
        eprintln!(
            "PhotoFindry Linux: macOS app migration and its background entry are not available on Linux. Use lifecycle install for the signed companion; libraries, originals and model files are kept."
        );
        std::process::exit(2);
    }
    if args.is_empty() || args == ["--help"] {
        println!(
            "PhotoFindry private Linux development companion\n  photofindry new --data-dir /absolute/new/library --runtime /absolute/runtime [--open]\n  photofindry serve --data-dir /absolute/linux/library --runtime /absolute/runtime [--open] [--no-stdin] [--launch-state /absolute/private/state]\n  photofindry open --launch-state /absolute/private/state [--no-browser]\n\nUses the existing Rust worker and authenticated loopback browser UI. Linux libraries are marked isolated development data. The normal default is $XDG_DATA_HOME/photofindry/Library (or ~/.local/share/photofindry/Library). Keep the terminal open, or explicitly use --no-stdin for supervision. Model and folder selections use zenity or terminal grant commands. The private launch-state entry holds a per-user Unix socket; each Open requests a one-use local browser ticket. Signed install/update: photofindry lifecycle install|status|check|update|rollback|recover|start|stop, and photofindry uninstall --confirm (keeps the library). macOS app migration is not available on Linux."
        );
        return;
    }
    let result = match configuration(args) {
        Ok(c) => run(c).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        eprintln!("PhotoFindry Linux: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_library_generation_refuses_stale_browser_actions() {
        let mut h = HeaderMap::new();
        assert!(!generation_header(&h, 0));
        h.insert("x-photofindry-library-generation", HeaderValue::from_static("0"));
        assert!(generation_header(&h, 0) && !generation_header(&h, 1));
        h.insert("x-photofindry-library-generation", HeaderValue::from_static("1"));
        assert!(generation_header(&h, 1));
        h.append("x-photofindry-library-generation", HeaderValue::from_static("1"));
        assert!(!generation_header(&h, 1), "duplicate headers stay ambiguous");
    }
    #[test]
    fn origin_and_headers_fail_closed() {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("127.0.0.1:8123"));
        h.insert("origin", HeaderValue::from_static("http://127.0.0.1:8123"));
        assert!(boundary(
            &h,
            "127.0.0.1:8123",
            "http://127.0.0.1:8123",
            true
        ));
        h.append("origin", HeaderValue::from_static("http://evil.test"));
        assert!(!boundary(
            &h,
            "127.0.0.1:8123",
            "http://127.0.0.1:8123",
            true
        ));
        h.remove("origin");
        assert!(!boundary(
            &h,
            "127.0.0.1:8123",
            "http://127.0.0.1:8123",
            true
        ));
        h.insert("origin", HeaderValue::from_static("http://127.0.0.1:8123"));
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!boundary(
            &h,
            "127.0.0.1:8123",
            "http://127.0.0.1:8123",
            true
        ));
    }
    #[test]
    fn unqualified_updater_and_replacement_are_explicitly_unavailable() {
        let c = capabilities();
        assert_eq!(c["signedUpdates"], true);
        assert_eq!(c["libraryReplacement"], false);
        assert_eq!(c["backupTransfer"], false);
        assert_eq!(c["libraryBackup"], true);
        for name in ["attachSourceArchive", "exportSourceArchive"] {
            assert!(!linux_method_supported(name));
        }
        for name in [
            "inspectBackup",
            "exportLibrary",
            "restoreLibrary",
            "configure",
            "startModel",
            "process",
            "searchInteractive",
            "startAISearch",
            "jobAction",
            "configurePhotoReview",
            "checkPhotoReview",
        ] {
            assert!(linux_method_supported(name));
        }
        // Metadata has a platform provider; People depends on packaged files.
        assert_eq!(c["metadata"], true);
        assert_eq!(c["photoReview"], true);
        assert_eq!(c["photoReviewProviders"], json!(["tagging-model"]));
        assert_eq!(c["faceDetection"], preview_features::current().faces);
        // Previews hide unreported features, so Linux states its supported deletion.
        assert_eq!(c["managedDeletion"], true);
        assert!(linux_method_supported("deleteRemovedEntries"));
    }
    #[test]
    fn capability_reconciliation_targets_only_unfinished_unsupported_jobs() {
        let snapshot = json!({"jobs":{"jobs":[{"id":"tag","kind":"tagging","state":"paused"},{"id":"ai","kind":"aiSearch","state":"running"},{"id":"face","kind":"faces","state":"paused"},{"id":"meta","kind":"metadata","state":"queued"},{"id":"history","kind":"faces","state":"completed"},{"id":"old-failure","kind":"improve","state":"failed"}]}});
        // Without a packaged FaceHelper only People work is stopped; metadata
        // and improvement now have providers on this platform.
        assert_eq!(unsupported_jobs(&snapshot), vec!["face"]);
        let performance = json!({"jobs":{"jobs":[{"id":"perf","kind":"performance","state":"queued"},{"id":"improve","kind":"improve","state":"queued"}]}});
        assert_eq!(unsupported_jobs(&performance), vec!["perf"]);
    }
    #[test]
    fn source_polling_uses_only_retained_capable_roots() {
        let s = json!({"sources":[{"automaticLifecycleVersion":1,"roots":[{"path":"/one","active":true},{"path":"relative"},{"path":"/inactive","active":false}]},{"automaticLifecycleVersion":1,"accessOriginals":false,"roots":[{"path":"/revoked"}]},{"roots":[{"path":"/legacy"}]},{"automaticLifecycleVersion":1,"mode":"archived","roots":[{"path":"/archive"}]}]});
        assert_eq!(observation_roots(&s), vec!["/one"]);
    }
    #[test]
    fn customer_library_is_never_opened_by_marker_guess() {
        let path = std::env::temp_dir().join(format!("pf-linux-admission-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let cfg = Config {
            data: path.clone(),
            runtime: path.clone(),
            worker: path.join("PhotoWorker"),
            port: 0,
            new: false,
            open: false,
            no_stdin: false,
            launch_state: None,
        };
        assert!(prepare_library(&cfg).is_err());
        assert!(!path.join(MARKER).exists());
        let mut new = cfg;
        new.new = true;
        assert!(prepare_library(&new).is_err());
        assert!(!path.join(MARKER).exists());
        fs::remove_dir_all(path).unwrap();
    }
}
