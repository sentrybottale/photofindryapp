//! Explicit user-authored feedback only. Fixed HTTPS destination, no redirects,
//! bounded response, and no server-supplied executable content or local authority.
use super::native_picker::Picker;
#[path = "feedback_diagnostics.rs"]
mod diagnostics;
pub use diagnostics::observe as observe_diagnostics;
pub use diagnostics::snapshot as diagnostic_snapshot;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    kind: String,
    area: String,
    summary: String,
    details: String,
    #[serde(default)]
    steps: String,
    #[serde(default)]
    expected: String,
    contact: String,
    #[serde(default)]
    include_environment: bool,
    #[serde(default)]
    submission_id: Option<String>,
    #[serde(default)]
    screenshot_grants: Vec<String>,
    #[serde(default)]
    diagnostics: Option<Value>,
    confirm_send: bool,
}
const MAX_IMAGE: usize = 5 * 1024 * 1024;
const MAX_PREPARED_FRAME: usize = 7 * 1024 * 1024 + 4096;
static IMAGE_SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static REVIEWED: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
fn validate_screenshot_grants(grants: &[String]) -> Result<(), String> {
    if grants.len() > 3
        || grants
            .iter()
            .any(|grant| grant.len() != 64 || !grant.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Choose up to three screenshots using the native file chooser.".into());
    }
    let unique: std::collections::HashSet<_> = grants.iter().collect();
    if unique.len() != grants.len() {
        return Err("The same screenshot was attached more than once.".into());
    }
    Ok(())
}
fn image_response(value: Value) -> Result<Value, String> {
    let encoded = value["pngBase64"]
        .as_str()
        .filter(|s| s.len() <= (MAX_IMAGE + 2) / 3 * 4)
        .ok_or("The screenshot helper returned invalid image data.")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "The screenshot helper returned invalid image data.")?;
    if bytes.is_empty()
        || bytes.len() > MAX_IMAGE
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || value["bytes"].as_u64() != Some(bytes.len() as u64)
    {
        return Err("The screenshot helper returned invalid image data.".into());
    }
    let width = value["width"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 2560)
        .ok_or("Invalid screenshot dimensions.")?;
    let height = value["height"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 2560)
        .ok_or("Invalid screenshot dimensions.")?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if value["sha256"] != digest {
        return Err("The prepared screenshot could not be verified.".into());
    }
    Ok(
        json!({"pngBase64":encoded,"width":width,"height":height,"bytes":bytes.len(),"sha256":digest}),
    )
}
async fn prepared(grant: &str, picker: &Picker) -> Result<Value, String> {
    validate_screenshot_grants(&[grant.to_owned()])?;
    let _slot = IMAGE_SLOT
        .acquire()
        .await
        .map_err(|_| "The screenshot decoder is unavailable.")?;
    let path = picker.resolve(grant, "screenshot")?;
    let executable = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("CompanionFeedbackImage")))
        .unwrap_or_else(|| PathBuf::from("/nonexistent/CompanionFeedbackImage"));
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "The screenshot decoder is unavailable. Rebuild the companion tools.")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("Screenshot decoder input is unavailable.")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Screenshot decoder output is unavailable.")?;
    let request = serde_json::to_vec(&json!({"path":path})).unwrap();
    let work = async {
        stdin
            .write_all(&request)
            .await
            .map_err(|_| "Screenshot decoder disconnected.")?;
        stdin
            .shutdown()
            .await
            .map_err(|_| "Screenshot decoder disconnected.")?;
        // Tokio pipe shutdown does not necessarily release the underlying fd.
        // The native bounded reader waits for EOF before decoding the request.
        drop(stdin);
        let mut bytes = Vec::new();
        stdout
            .take((MAX_PREPARED_FRAME + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Screenshot decoder disconnected.")?;
        if bytes.len() > MAX_PREPARED_FRAME {
            return Err("The screenshot decoder response exceeded its bound.");
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Screenshot decoder stopped.")?;
        let value = serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| "Invalid screenshot decoder response.")?;
        if !status.success() {
            return Err(
                "This file could not be prepared. Choose a single PNG or JPEG up to 20 MB and 48 megapixels.",
            );
        }
        Ok(value)
    };
    let value = match tokio::time::timeout(Duration::from_secs(30), work).await {
        Ok(v) => v.map_err(str::to_owned)?,
        Err(_) => {
            let _ = child.kill().await;
            return Err("Screenshot preparation timed out. Choose a smaller image.".into());
        }
    };
    // Pin both ends of decoding to the native selection's identity. No path is
    // returned to the browser or support server, and the original is never edited.
    if picker.resolve(grant, "screenshot")? != path {
        return Err("The selected screenshot changed. Choose it again.".into());
    }
    image_response(value)
}
pub fn clear_reviews() {
    if let Some(reviewed) = REVIEWED.get() {
        reviewed.lock().unwrap().clear();
    }
}
pub async fn prepare_screenshot(grant: &str, picker: &Picker) -> Result<Value, String> {
    let value = prepared(grant, picker).await?;
    let mut reviewed = REVIEWED
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    if reviewed.len() >= 256 && !reviewed.contains_key(grant) {
        return Err("Too many screenshot preparations. Restart the companion to clear the local review cache.".into());
    }
    reviewed.insert(grant.into(), value["sha256"].as_str().unwrap().into());
    Ok(value)
}
pub fn environment_snapshot() -> Value {
    let mut buffer = [0u8; 64]; let mut size = buffer.len();
    let version = unsafe { if libc::sysctlbyname(c"kern.osproductversion".as_ptr(), buffer.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) == 0 && size <= buffer.len() {
        String::from_utf8_lossy(&buffer[..size]).trim_end_matches('\0').chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').take(32).collect::<String>()
    } else { String::new() } };
    json!({"appVersion":format!("CLI companion {}",env!("CARGO_PKG_VERSION")),"systemVersion":if version.is_empty(){"macOS".to_owned()}else{format!("macOS {version}")},"architecture":std::env::consts::ARCH})
}
fn payload(input: Value) -> Result<Value, String> {
    let report: Report = serde_json::from_value(input).map_err(|_| "Invalid feedback fields.")?;
    validate_screenshot_grants(&report.screenshot_grants)?;
    if !report.confirm_send {
        return Err("Confirm sending this report to PhotoFindry support.".into());
    }
    if !["Bug report", "Suggestion", "Other feedback"].contains(&report.kind.as_str())
        || ![
            "General",
            "Library",
            "Tagging",
            "Search",
            "People",
            "Sources",
            "Jobs",
            "Model setup",
            "Backups",
            "Updates",
            "Other",
        ]
        .contains(&report.area.as_str())
    {
        return Err("Choose a feedback type and area.".into());
    }
    for (value, limit) in [
        (&report.summary, 120),
        (&report.details, 8000),
        (&report.steps, 4000),
        (&report.expected, 2000),
    ] {
        if value.chars().count() > limit {
            return Err("A feedback field exceeds its limit.".into());
        }
    }
    let parts: Vec<_> = report.contact.split('@').collect();
    if report.summary.trim().is_empty()
        || report.details.trim().is_empty()
        || report.contact.len() > 254
        || parts.len() != 2
        || parts[0].is_empty()
        || !parts[1].contains('.')
        || report
            .contact
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("Add a valid email, summary and details.".into());
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "The system random source is unavailable.")?;
    let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let id = report.submission_id.unwrap_or_else(|| {
        format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )
    });
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("Invalid feedback submission identity.".into());
    }
    let environment = if report.include_environment {
        environment_snapshot()
    } else {
        Value::Null
    };
    let mut result = json!({"schema":1,"submissionID":id,"report":{"kind":report.kind,"area":report.area,"summary":report.summary,"details":report.details,"steps":report.steps,"expected":report.expected,"contact":report.contact,"environment":environment},"screenshots":[]});
    if let Some(value) = report.diagnostics.filter(|v| !v.is_null()) { result["diagnostics"] = diagnostics::validate(&value)?; }
    Ok(result)
}
fn receipt(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > 8192 {
        return Err(
            "The support server response exceeded its limit. Your draft is still here.".into(),
        );
    }
    let split = bytes
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or("The support server response is invalid.")?;
    if ![b"200".as_slice(), b"201".as_slice()].contains(&&bytes[split + 1..]) {
        return Err(
            "The support server did not confirm this report. Your draft is still here.".into(),
        );
    }
    let value: Value = serde_json::from_slice(&bytes[..split])
        .map_err(|_| "The support server receipt is invalid.")?;
    let id = value["reportID"].as_str().unwrap_or("");
    let when = value["receivedAt"].as_str().unwrap_or("");
    if value["schema"] != 1
        || id.len() != 19
        || !id.starts_with("PF-")
        || !id[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        || when.len() > 64
        || !when.contains('T')
        || when.chars().any(char::is_control)
    {
        return Err("The support server receipt could not be confirmed.".into());
    }
    Ok(json!({"reportID":id,"receivedAt":when}))
}
struct Cancellation { cancelled: std::sync::atomic::AtomicBool, changed: tokio::sync::Notify }
static SUBMISSIONS: OnceLock<Mutex<HashMap<String, std::sync::Arc<Cancellation>>>> = OnceLock::new();
struct SubmissionGuard(String);
impl Drop for SubmissionGuard { fn drop(&mut self) { if let Some(active)=SUBMISSIONS.get() { active.lock().unwrap().remove(&self.0); } } }
pub fn cancel_submission(input: Value) -> Result<Value, String> {
    #[derive(Deserialize)] #[serde(rename_all="camelCase",deny_unknown_fields)] struct Cancel { submission_id:String }
    let request:Cancel=serde_json::from_value(input).map_err(|_|"Invalid feedback cancellation.")?;
    if request.submission_id.len()!=36 {return Err("Invalid feedback submission identity.".into());}
    let active=SUBMISSIONS.get_or_init(||Mutex::new(HashMap::new())).lock().unwrap();
    if let Some(signal)=active.get(&request.submission_id) {signal.cancelled.store(true,std::sync::atomic::Ordering::Release);signal.changed.notify_waiters();}
    Ok(json!({"cancelled":active.contains_key(&request.submission_id),"delivery":"unknown"}))
}
pub async fn submit(input: Value, picker: &Picker) -> Result<Value, String> {
    let report=payload(input.clone())?;
    let id=report["submissionID"].as_str().unwrap().to_owned();
    let signal=std::sync::Arc::new(Cancellation{cancelled:std::sync::atomic::AtomicBool::new(false),changed:tokio::sync::Notify::new()});
    {let mut active=SUBMISSIONS.get_or_init(||Mutex::new(HashMap::new())).lock().unwrap();
     if active.contains_key(&id){return Err("This report is already being sent. Wait for its receipt or cancel it first.".into());}
     if active.len()>=4{return Err("Too many feedback submissions are active.".into());}
     active.insert(id.clone(),signal.clone());}
    let _guard=SubmissionGuard(id);
    let cancelled=async {loop {let changed=signal.changed.notified();tokio::pin!(changed);changed.as_mut().enable();if signal.cancelled.load(std::sync::atomic::Ordering::Acquire){break;}changed.await;}};
    tokio::select! {biased; _=cancelled=>Err("Sending stopped. The server may already have received it; retry this unchanged draft to avoid a duplicate.".into()), result=submit_inner(input,picker,report)=>result}
}
async fn submit_inner(input:Value,picker:&Picker,mut report:Value)->Result<Value,String>{
    let grants: Vec<String> =
        serde_json::from_value(input.get("screenshotGrants").cloned().unwrap_or(json!([])))
            .map_err(|_| "Invalid screenshot selections.")?;
    let mut screenshots = Vec::with_capacity(grants.len());
    for grant in grants {
        let expected = REVIEWED
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap()
            .get(&grant)
            .cloned()
            .ok_or("Review each prepared screenshot before sending this report.")?;
        let screenshot = prepared(&grant, picker).await?;
        if screenshot["sha256"] != expected {
            return Err(
                "A screenshot changed after review. Remove it and choose it again before sending."
                    .into(),
            );
        }
        screenshots.push(screenshot["pngBase64"].as_str().unwrap().to_owned());
    }
    report["screenshots"] = json!(screenshots);
    let body = serde_json::to_vec(&report).map_err(|_| "Feedback could not be encoded.")?;
    if body.len() > 22 * 1024 * 1024 {
        return Err("The report is too large. Remove a screenshot and try again.".into());
    }
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--disable", // Ignore ~/.curlrc so fixed destination/no-redirect policy is authoritative.
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "90",
            "--max-filesize",
            "8192",
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--header",
            "Accept: application/json",
            "--data-binary",
            "@-",
            "--write-out",
            "\n%{http_code}",
            "https://photofindry.com/bugs/api/v1/reports",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "The system HTTPS client is unavailable.")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("Could not start feedback submission.")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Could not start feedback submission.")?;
    let work = async {
        stdin
            .write_all(&body)
            .await
            .map_err(|_| "Feedback connection failed.")?;
        stdin
            .shutdown()
            .await
            .map_err(|_| "Feedback connection failed.")?;
        // curl --data-binary @- consumes the request through EOF.
        drop(stdin);
        let mut bytes = Vec::new();
        stdout
            .take(8193)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Feedback connection failed.")?;
        if bytes.len() > 8192 {
            return Err("The support server response exceeded its bound.");
        }
        if !child
            .wait()
            .await
            .map_err(|_| "Feedback submission stopped.")?
            .success()
        {
            return Err("Feedback could not be delivered. Your draft is still here.");
        }
        Ok(bytes)
    };
    let bytes = match tokio::time::timeout(Duration::from_secs(95), work).await {
        Ok(result) => result.map_err(str::to_owned)?,
        Err(_) => {
            let _ = child.kill().await;
            return Err("Feedback timed out. Your draft is still here.".into());
        }
    };
    receipt(&bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_images_are_bounded_and_integrity_checked() {
        let bytes = b"\x89PNG\r\n\x1a\nfixture";
        let valid = json!({"pngBase64":STANDARD.encode(bytes),"bytes":bytes.len(),"width":20,"height":10,"sha256":format!("{:x}",Sha256::digest(bytes))});
        assert!(image_response(valid.clone()).is_ok());
        let mut changed = valid.clone();
        changed["sha256"] = json!("0".repeat(64));
        assert!(image_response(changed).is_err());
        let mut big = valid;
        big["width"] = json!(3000);
        assert!(image_response(big).is_err());
        assert!(validate_screenshot_grants(&["a".repeat(64), "a".repeat(64)]).is_err());
    }
    #[test]
    fn feedback_requires_explicit_user_content_and_bounded_receipt() {
        let report = json!({"kind":"Bug report","area":"Library","summary":"Test","details":"Test details","contact":"test@example.invalid","confirmSend":true});
        let result = payload(report.clone()).unwrap();
        assert_eq!(result["screenshots"], json!([]));
        assert!(result["report"]["environment"].is_null());
        let mut unconfirmed = report.clone();
        unconfirmed["confirmSend"] = json!(false);
        assert!(payload(unconfirmed).is_err());
        let mut too_many = report.clone();
        too_many["screenshotGrants"] = json!([
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64)
        ]);
        assert!(payload(too_many).is_err());
        let mut wrong_type = report.clone();
        wrong_type["screenshotGrants"] = json!(["/private/file.png"]);
        assert!(payload(wrong_type).is_err());
        let mut injected = report;
        injected["endpoint"] = json!("https://evil.invalid");
        assert!(payload(injected).is_err());
        assert!(receipt(b"{\"schema\":1,\"reportID\":\"PF-0123456789ABCDEF\",\"receivedAt\":\"2026-10-15T12:00:00Z\"}\n201").is_ok());
        assert!(receipt(b"<script>bad</script>\n200").is_err());
        assert!(receipt(b"{}\n302").is_err());
    }
}
