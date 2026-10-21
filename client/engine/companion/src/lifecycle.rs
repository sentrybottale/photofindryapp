//! Per-user CLI lifecycle. Program versions are separate from the photo library.
//! No website supplies a path, trust key, executable or installation decision.
use super::{cli_release as release, launch, uninstall, retained_access_owner};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
type Result<T> = std::result::Result<T, String>;
const STATE: &str = "installation.json";
const JOURNAL: &str = "transaction.json";
const OPEN_REQUEST: &str = "open-update-request.json";
const LAST_FAILURE: &str = "last-failure.json";
const ACCESS_RECEIPT: &str = "access-check.json";
const OPEN_OPERATION_TIMEOUT: Duration = Duration::from_secs(1800);
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OpenRequest {
    schema: u32,
    id: String,
    complete: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    pid: Option<u32>,
    updated_at: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Installation {
    pub schema: u32,
    pub current: u64,
    pub previous: Option<u64>,
    pub highest: u64,
    pub data: PathBuf,
    pub launch_state: PathBuf,
    pub autostart: bool,
    pub automatic_updates: bool,
    #[serde(default)]
    pub retire: Option<PathBuf>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Transaction {
    schema: u32,
    before: Option<Installation>,
    after: Installation,
    registration: Option<launch::Registration>,
    committed: bool,
}
pub fn default_root() -> Result<PathBuf> {
    let launch = launch::default_directory()?;
    Ok(launch
        .parent()
        .and_then(Path::parent)
        .ok_or("Cannot locate account application support")?
        .join("PhotoFindry Companion"))
}
pub fn installed_root(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let package = bin.parent()?;
    let version = package.parent()?;
    let versions = version.parent()?;
    if bin.file_name()? == "bin"
        && package.file_name()? == "package"
        && versions.file_name()? == "versions"
        && version.file_name()?.to_str()?.parse::<u64>().is_ok()
    {
        Some(versions.parent()?.to_path_buf())
    } else {
        None
    }
}
fn package(root: &Path, build: u64) -> PathBuf {
    root.join("versions")
        .join(build.to_string())
        .join("package")
}
fn executable(root: &Path, build: u64) -> PathBuf {
    package(root, build).join("bin/photofindry")
}
fn verifier(exe: &Path) -> Result<PathBuf> {
    launch::browser_opener(exe)?;
    launch::canonical(
        &exe.parent()
            .ok_or("Missing companion directory")?
            .join("CompanionReleaseVerify"),
        false,
    )
}
fn self_executable() -> Result<PathBuf> {
    fs::canonicalize(std::env::current_exe().map_err(|_| "Cannot locate companion")?)
        .map_err(|_| "Cannot resolve companion executable".into())
}
/// Browser transport calls this on a blocking task before each private helper
/// launch. Managed installations never silently fall back to path-only grants.
pub fn source_access_helper_for_current_executable() -> Result<Option<PathBuf>> {
    let exe=self_executable()?;
    let Some(root)=installed_root(&exe) else{return Ok(None)};
    let cfg=read_state(&root)?;
    if executable(&root,cfg.current)!=exe{return Err("Only the active installed CLI may select original-folder access.".into());}
    Ok(Some(retained_access_owner::verified(&root,&verifier(&exe)?)?))
}
fn provision_source_access_owner(root:&Path,cfg:&Installation)->Result<()> {
    let source=package(root,cfg.current);
    // Retained older signed packages predate this protocol. They remain valid
    // migration/rollback software; new packages require this helper in packing.
    if fs::symlink_metadata(retained_access_owner::directory(root)).is_err()
        && !source.join("bin/CompanionSourceAccess.app").exists(){return Ok(());}
    retained_access_owner::provision(root,&source,&release::read(&root.join("versions").join(cfg.current.to_string()).join("release.json"),131072)?,&verifier(&self_executable()?)?)?;
    Ok(())
}
fn read_state(root: &Path) -> Result<Installation> {
    launch::private_directory(root, false)?;
    let state: Installation = serde_json::from_slice(&release::read(&root.join(STATE), 16384)?)
        .map_err(|_| {
            "Companion installation state is invalid. Run lifecycle recover; your library is kept."
        })?;
    validate_state(root, &state)?;
    Ok(state)
}
fn validate_state(root: &Path, state: &Installation) -> Result<()> {
    if state.schema != 1
        || state.current == 0
        || state.highest < state.current
        || state.previous == Some(state.current)
        || !state.data.is_absolute()
        || !state.launch_state.is_absolute()
        || state.data.starts_with(root)
        || root.starts_with(&state.data)
        || state.launch_state.starts_with(root)
        || root.starts_with(&state.launch_state)
        || state.previous.is_some_and(|v| v == 0 || v > state.highest)
    {
        return Err("Companion installation paths or version state are invalid".into());
    }
    Ok(())
}
fn atomic(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let owner = launch::private_directory(root, false)?;
    let target = root.join(name);
    if target.exists() {
        let _ = launch::private_file(&target, false)?;
    }
    let mut entropy = [0u8; 8];
    getrandom::fill(&mut entropy).map_err(|_| "Cannot stage private lifecycle state")?;
    let temp = root.join(format!(
        ".write-{}",
        entropy
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)
            .map_err(|_| "Cannot stage lifecycle state")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot flush lifecycle state")?;
        launch::same_directory(root, &owner)?;
        fs::rename(&temp, &target).map_err(|_| "Cannot publish lifecycle state")?;
        owner
            .sync_all()
            .map_err(|_| "Cannot flush installation directory")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn save(root: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    atomic(
        root,
        name,
        &serde_json::to_vec(value).map_err(|_| "Cannot encode lifecycle state")?,
    )
}
fn clear(root: &Path, name: &str) -> Result<()> {
    if root.join(name).exists() {
        let _ = launch::private_file(&root.join(name), false)?;
        fs::remove_file(root.join(name)).map_err(|_| "Cannot finish lifecycle transaction")?;
        launch::private_directory(root, false)?
            .sync_all()
            .map_err(|_| "Cannot flush lifecycle completion")?;
    }
    Ok(())
}
fn note(root: &Path, phase: &str, message: &str) {
    let _ = save(
        root,
        "status.json",
        &json!({"phase":phase,"message":message,"updatedAt":now()}),
    );
}
fn failure(root: &Path, operation: &str, message: &str) {
    let bounded: String = message.chars().filter(|c| !c.is_control()).take(2048).collect();
    let current = read_state(root).ok().map(|state| state.current);
    let _ = save(root, LAST_FAILURE, &json!({"schema":1,"operation":operation,"build":current,"message":bounded,"updatedAt":now()}));
    note(root, "error", &bounded);
}
pub(super) fn note_opening_stage(root: &Path, stage: &'static str) {
    note(root, "opening", &format!("Opening the local library: {stage}. Saved work stays in place."));
}
pub fn startup_error(root: &Path) -> Option<String> {
    let bytes = release::read(&root.join("status.json"), 16384).ok()?;
    let status: Value = serde_json::from_slice(&bytes).ok()?;
    if status["phase"] != "error" { return None; }
    status["message"].as_str().filter(|message| message.len() <= 2048).map(str::to_owned)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn checked_release(root: &Path, build: u64, helper: &Path) -> Result<release::Release> {
    let envelope = release::read(
        &root
            .join("versions")
            .join(build.to_string())
            .join("release.json"),
        131072,
    )?;
    let result = release::verify(&envelope, helper)?;
    if result.build != build {
        return Err("Installed version identity changed".into());
    }
    release::verify_package(&package(root, build), &result)?;
    Ok(result)
}
fn label(root: &Path) -> Result<String> {
    Ok(if root == default_root()? {
        "com.photofindry.companion".into()
    } else {
        format!(
            "com.photofindry.companion.test.{}",
            &release::digest(root.as_os_str().as_encoded_bytes())[..16]
        )
    })
}
fn agent_path(root: &Path) -> Result<PathBuf> {
    if root == default_root()? {
        let launch = launch::default_directory()?;
        Ok(launch
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("Cannot locate account Library")?
            .join("LaunchAgents/com.photofindry.companion.plist"))
    } else {
        Ok(root.join("agent.plist"))
    }
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn plist(root: &Path, cfg: &Installation, operation: Option<&str>) -> Result<Vec<u8>> {
    plist_with_request(root, cfg, operation, None)
}
fn plist_with_request(root: &Path, cfg: &Installation, operation: Option<&str>, request: Option<&str>) -> Result<Vec<u8>> {
    let mut args = vec![
        executable(root, cfg.current).to_string_lossy().into_owned(),
        "lifecycle".into(),
        operation.unwrap_or("background").into(),
        "--install-root".into(),
        root.to_string_lossy().into_owned(),
    ];
    if let Some(id) = request {
        if operation != Some("open-update") || !request_id(id) {
            return Err("Invalid companion Open request identity".into());
        }
        args.extend(["--open-request".into(), id.into()]);
    }
    if matches!(operation, Some("update" | "rollback")) {
        args.push("--open-after".into());
    }
    let label = format!(
        "{}{}",
        label(root)?,
        if operation.is_some() {
            ".maintenance"
        } else {
            ""
        }
    );
    let args = args
        .iter()
        .map(|s| format!("<string>{}</string>", xml(s)))
        .collect::<String>();
    // Keep maintenance throttled, but an explicit Open after Quit must not
    // inherit a 30-second delay. KeepAlive is false: failed background jobs
    // are never restarted unattended.
    let throttle = if operation.is_some() { 30 } else { 1 };
    let attribution=if retained_access_owner::application(root).is_dir(){format!("<key>AssociatedBundleIdentifiers</key><array><string>{}</string></array>",retained_access_owner::IDENTIFIER)}else{String::new()};
    Ok(format!("<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array>{args}</array>{attribution}<key>RunAtLoad</key><{} /><key>ProcessType</key><string>Background</string><key>ThrottleInterval</key><integer>{throttle}</integer><key>KeepAlive</key><{} /></dict></plist>",xml(&label),if operation.is_some()||cfg.autostart{"true"}else{"false"},"false").into_bytes())
}
fn write_agent(root: &Path, cfg: &Installation) -> Result<PathBuf> {
    provision_source_access_owner(root,cfg)?;
    let path = agent_path(root)?;
    let parent = path.parent().ok_or("Missing LaunchAgent directory")?;
    if !parent.exists() {
        fs::create_dir(parent)
            .map_err(|_| "Cannot create this account's LaunchAgents directory")?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect LaunchAgents directory")?;
    }
    launch::canonical(parent, true)?;
    if path.exists() {
        let old = release::read(&path, 16384)?;
        let text = std::str::from_utf8(&old).map_err(|_| "Invalid existing LaunchAgent")?;
        if !text.contains(&format!("<string>{}</string>", xml(&label(root)?)))
            || !text.contains(&xml(&root.to_string_lossy()))
        {
            return Err(
                "A different job owns the PhotoFindry LaunchAgent name. No job was changed.".into(),
            );
        }
    }
    if parent == root {
        atomic(root, "agent.plist", &plist(root, cfg, None)?)?;
    } else {
        let temp = parent.join(format!(".photofindry-{}.plist", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)
            .map_err(|_| "Cannot stage autostart settings")?;
        let result: Result<()> = (|| {
            file.write_all(&plist(root, cfg, None)?)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Cannot flush autostart settings")?;
            fs::rename(&temp, &path).map_err(|_| "Cannot publish autostart settings")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result?;
    }
    Ok(path)
}
/// Confirm access in launchd's actual user bootstrap before stopping a usable
/// host. A Terminal/native caller can have TCC access that its LaunchAgent lacks.
fn check_library_access(data: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let inaccessible = "The background companion cannot inspect this library folder. Check macOS Privacy & Security before retrying. The current program and library are kept.";
    let (directory, present) = match fs::symlink_metadata(data) {
        Ok(_) => (data, true),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound => {
            let mut parent=None;
            for path in data.ancestors().skip(1) {
                match fs::symlink_metadata(path) {
                    Ok(_) => {parent=Some(path);break;},
                    Err(error) if error.kind()==std::io::ErrorKind::NotFound => {},
                    Err(_) => return Err(inaccessible.into()),
                }
            }
            (parent.ok_or("The library disk is unavailable. Reconnect it before setup.")?, false)
        },
        Err(_) => return Err(inaccessible.into()),
    };
    launch::canonical(directory, true).map_err(|_| "The background companion cannot access this library folder. Check macOS Privacy & Security or explicitly choose an accessible library copy. The current program and library are kept.")?;
    let mut entries = fs::read_dir(directory).map_err(|_| "The background companion cannot access this library folder. Allow PhotoFindry access in macOS Privacy & Security, or explicitly choose an accessible library copy. The existing program and library are kept.")?;
    let _ = entries.next().transpose().map_err(|_| "The background companion cannot list this library folder. Check macOS Privacy & Security before retrying.")?;
    let path=std::ffi::CString::new(directory.as_os_str().as_bytes()).map_err(|_|"The library folder path is invalid")?;
    if unsafe {libc::access(path.as_ptr(),libc::W_OK|libc::X_OK)}!=0 {
        return Err("The background companion cannot write in this library folder. Allow PhotoFindry access in macOS Privacy & Security, then retry. The current program and library are kept.".into());
    }
    if !present {return Ok(());}
    // Fresh and legacy libraries may not have every lazy store yet. Admission
    // owns format validation; this probe tests OS access without creating files.
    for name in ["catalog.json", "catalog.sqlite", "jobs.sqlite", "people.sqlite", "source-host.sqlite"] {
        let path = data.join(name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind()==std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("The background companion cannot inspect the saved library catalog. Check macOS folder access before retrying.".into()),
            Ok(_) => {},
        }
        let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&path).map_err(|_| "The background companion cannot read the library's saved catalog. Allow PhotoFindry access in macOS Privacy & Security, or explicitly choose an accessible library copy. No library was moved or replaced.")?;
        if !file.metadata().map_err(|_| "Cannot inspect the saved library catalog")?.is_file() {
            return Err("The library catalog is not an ordinary file. Keep it in place for recovery.".into());
        }
        let mut prefix = [0u8; 32];
        let _ = file.read(&mut prefix).map_err(|_| "The background companion could not read the saved library catalog. Check macOS folder access before retrying.")?;
    }
    Ok(())
}
fn access_check_plist(root: &Path, data: &Path, exe: &Path, id: &str) -> Result<Vec<u8>> {
    if !request_id(id) { return Err("Invalid background access request identity".into()); }
    let args = [exe.to_string_lossy().into_owned(), "lifecycle".into(), "access-check".into(), "--install-root".into(), root.to_string_lossy().into_owned(), "--data-dir".into(), data.to_string_lossy().into_owned(), "--open-request".into(), id.into()]
        .iter().map(|s| format!("<string>{}</string>", xml(s))).collect::<String>();
    let attribution=if retained_access_owner::application(root).is_dir(){format!("<key>AssociatedBundleIdentifiers</key><array><string>{}</string></array>",retained_access_owner::IDENTIFIER)}else{String::new()};
    Ok(format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>Label</key><string>{}.access-check</string><key>ProgramArguments</key><array>{args}</array>{attribution}<key>RunAtLoad</key><true/><key>ProcessType</key><string>Background</string><key>KeepAlive</key><false/></dict></plist>", xml(&label(root)?)).into_bytes())
}
/// launchd cannot inherit a Terminal parent's Documents/Downloads permission.
/// Copy only the caller-authenticated controller into the private program store
/// before launching the read-only probe; the library remains untouched.
fn stage_access_controller(root: &Path, exe: &Path, id: &str) -> Result<PathBuf> {
    let source=launch::canonical(exe,false)?;
    let expected=release::file_hash(&source,64*1024*1024)?;
    let directory=root.join("access-controller");launch::private_directory(&directory,true)?;
    let stage=Stage::new(directory.join(format!(".stage-{id}")));launch::private_directory(&stage.0,true)?;
    let copied=stage.0.join("photofindry");
    // Materialize bytes rather than cloning/copying provenance attributes from
    // a protected Documents/Downloads build. The embedded code seal is retained.
    let mut input=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK).open(&source)
        .map_err(|_|"Cannot read the authenticated background-access controller")?;
    let mut output=OpenOptions::new().write(true).create_new(true).mode(0o700).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(&copied)
        .map_err(|_|"Cannot stage the authenticated background-access controller")?;
    let mut buffer=[0u8;65536];let mut total=0u64;
    loop {
        let count=input.read(&mut buffer).map_err(|_|"Cannot read the background-access controller")?;
        if count==0 {break;}
        total=total.checked_add(count as u64).ok_or("Background-access controller exceeds its bound")?;
        if total>64*1024*1024 {return Err("Background-access controller exceeds its bound".into());}
        output.write_all(&buffer[..count]).map_err(|_|"Cannot stage the background-access controller")?;
    }
    output.sync_all().map_err(|_|"Cannot flush the background-access controller")?;
    drop(output);drop(input);
    if release::file_hash(&copied,64*1024*1024)?!=expected {return Err("The background-access controller changed while being staged. The current program and library are kept.".into());}
    let target=directory.join("photofindry");
    if target.symlink_metadata().is_ok() {launch::private_file(&target,false)?;}
    fs::rename(&copied,&target).map_err(|_|"Cannot publish the authenticated background-access controller")?;
    launch::private_directory(&directory,false)?.sync_all().map_err(|_|"Cannot flush the background-access controller directory")?;
    Ok(target)
}
fn access_timeout(receipt: &OpenRequest) -> String {
    if receipt.pid.is_none() {
        "The background companion did not report startup from the verified program store within 15 seconds. Rerun verified setup; the current program and library are kept.".into()
    } else {
        "The background companion started but could not finish checking the library folder within 15 seconds. Reconnect its disk or check macOS folder access, then retry. The current program and library are kept.".into()
    }
}
async fn preflight_background_access(root: &Path, data: &Path, exe: &Path) -> Result<()> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|_| "Cannot identify background library access check")?;
    let id = release::digest(&nonce);
    let controller=stage_access_controller(root,exe,&id)?;
    save(root, ACCESS_RECEIPT, &OpenRequest { schema: 1, id: id.clone(), complete: false, error: None, pid: None, updated_at: now() })?;
    atomic(root, "access-check.plist", &access_check_plist(root, data, &controller, &id)?)?;
    let job = format!("{}/{}.access-check", domain(), label(root)?);
    unload(job.clone()).await?;
    launchctl(&["bootstrap".into(), domain(), root.join("access-check.plist").to_string_lossy().into_owned()], true).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let result = loop {
        let receipt = match read_request(root, ACCESS_RECEIPT, &id) { Ok(value) => value, Err(reason) => break Err(reason) };
        if receipt.complete { break receipt.error.map_or(Ok(()), Err); }
        if tokio::time::Instant::now() >= deadline {
            break Err(access_timeout(&receipt));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let _ = unload(job).await;
    if let Err(reason) = &result { failure(root, "background-access", reason); }
    result
}
fn finish_access_check(root: &Path, data: &Path, id: &str) -> Result<()> {
    let mut receipt = read_request(root, ACCESS_RECEIPT, id)?;
    if receipt.complete { return Err("This background access check has already finished.".into()); }
    receipt.pid=Some(std::process::id());receipt.updated_at=now();
    save(root, ACCESS_RECEIPT, &receipt)?;
    let result = check_library_access(data);
    receipt.complete = true;
    receipt.error = result.as_ref().err().cloned();
    receipt.updated_at = now();
    save(root, ACCESS_RECEIPT, &receipt)?;
    result
}

async fn launchctl(args: &[String], required: bool) -> Result<()> {
    let status = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("/bin/launchctl")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| "macOS background-service operation timed out. Use lifecycle recover.")?
    .map_err(|_| "Cannot contact macOS background services")?;
    if required && !status.success() {
        return Err("macOS could not start the background companion. Run lifecycle recover, or log out and back in; your library is preserved.".into());
    }
    Ok(())
}
fn domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}
async fn job_present(service: &str) -> Result<bool> {
    let status = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("/bin/launchctl")
            .args(["print", service])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| "macOS background service status timed out")?
    .map_err(|_| "Cannot inspect macOS background service")?;
    Ok(status.success())
}
async fn unload(service: String) -> Result<()> {
    launchctl(&["bootout".into(), service.clone()], false).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while job_present(&service).await? {
        if tokio::time::Instant::now() > deadline {
            return Err(
                "The old background service is still closing. Wait, then choose Recover companion."
                    .into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
async fn start_agent(root: &Path, cfg: &Installation, reload: bool) -> Result<()> {
    let path = write_agent(root, cfg)?;
    let service = format!("{}/{}", domain(), label(root)?);
    if reload {
        unload(service.clone()).await?;
    }
    if !job_present(&service).await? {
        launchctl(
            &[
                "bootstrap".into(),
                domain(),
                path.to_string_lossy().into_owned(),
            ],
            true,
        )
        .await?;
    }
    launchctl(&["kickstart".into(), service], true).await
}
async fn stop_agent(root: &Path) -> Result<()> {
    unload(format!("{}/{}", domain(), label(root)?)).await
}
async fn health(root: &Path, cfg: &Installation) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(390);
    loop {
        if launch::request_health(&cfg.launch_state)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        if let Some(detail) = startup_error(root) {
            return Err(detail);
        }
        if tokio::time::Instant::now() > deadline {
            return Err("The companion did not become ready. The library stays in place; recovery will restore prior software when available.".into());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}
async fn drain(cfg: &Installation) -> Result<()> {
    let mut requested = launch::request_update_stop(&cfg.launch_state).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if !requested {
            requested = launch::request_update_stop(&cfg.launch_state).await?;
        }
        if let Some(_broker) = launch::try_lock(&cfg.launch_state, "broker.lock")? {
            let exe = launch::read_registration(&cfg.launch_state)?.executable;
            if let Some(_lease) = uninstall::try_update_runtime(&exe)? {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() > deadline {
            return Err(
                "The companion is still saving work. Wait, then retry; the old version is kept."
                    .into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn publish_registration(
    root: &Path,
    cfg: &Installation,
    guard: Option<&launch::LifecycleGuard>,
) -> Result<()> {
    if let Some(guard) = guard {
        guard.register(
            &cfg.launch_state,
            &cfg.data,
            &package(root, cfg.current).join("runtime"),
            &executable(root, cfg.current),
        )?;
    } else {
        launch::register(
            &cfg.launch_state,
            &cfg.data,
            &package(root, cfg.current).join("runtime"),
            &executable(root, cfg.current),
        )?;
    }
    Ok(())
}
// Installed signed state is authoritative for the same library. An idle legacy
// registration may be repaired; active work and a different library are never adopted.
fn registration_needs_repair(root: &Path, cfg: &Installation, record: &launch::Registration) -> Result<bool> {
    if record.data_dir != cfg.data {return Err("Another library owns the local registration. Open its workspace before changing installations.".into())}
    Ok(!record.matches(&cfg.data, &package(root,cfg.current).join("runtime"), &executable(root,cfg.current)))
}
fn repair_registration(root: &Path, cfg: &Installation) -> Result<()> {
    let _owner = launch::private_directory(&cfg.launch_state, true)?;
    launch::ensure_available(&cfg.launch_state)?;
    let guard = launch::LifecycleGuard::acquire(&cfg.launch_state)?;
    if let Some(record)=launch::recovery_registration(&cfg.launch_state)? {
        if !registration_needs_repair(root,cfg,&record)? {return Ok(())}
    }
    publish_registration(root,cfg,Some(&guard)).map_err(|_| "An older PhotoFindry copy is still using this library, or its launch state cannot be repaired. Quit that copy, then open the verified installer again. Your saved work is kept.".into())
}
fn existing_release_identity(current:u64, incoming:u64, installed:&[u8], offered:&[u8]) -> Result<()> {
    if current==incoming && installed!=offered {
        return Err("This build number already has different signed program bytes. Install a newer verified PhotoFindry release; the current companion and your library are kept.".into())
    }
    Ok(())
}

/// The installed launcher explicitly follows this account's current library.
/// No public URL or browser request can supply or change the selected path.
pub fn current_library(requested: &Path, state: Option<&Path>) -> Result<PathBuf> {
    let directory=launch::default_directory()?;
    if requested!=directory.parent().ok_or(INSTALL_NEXT_ACTION)? || state.is_some_and(|p|p!=directory) {
        return Err("Current-library selection is only available to this account's installed launcher.".into());
    }
    current_library_at(&default_root()?,&directory,requested)
}
fn current_library_at(root: &Path, state: &Path, fallback: &Path) -> Result<PathBuf> {
    launch::ensure_available(state)?;
    if root.join(STATE).exists() {
        let cfg=read_state(root)?;
        if cfg.launch_state!=state {return Err("The installed companion uses another launch registration. Open its workspace before changing setup.".into())}
        return Ok(cfg.data);
    }
    if let Some(record)=launch::recovery_registration(state)? {return Ok(record.data_dir);}
    if let Some(data)=uninstall::kept_library(state)? {return Ok(data);}
    Ok(fallback.to_path_buf())
}
/// Public launch prefers the verified standalone installation, even if an older
/// application rewrote the legacy launch record. Custom qualification states stay isolated.
const INSTALL_NEXT_ACTION: &str = "The standalone PhotoFindry companion is not ready. Download and open the latest verified installer from photofindry.com once, then return to Open PhotoFindry. Your saved library is kept.";
pub fn require_installed(directory: &Path) -> Result<()> {
    let root=default_root()?;
    if directory!=launch::default_directory()? {return Err(INSTALL_NEXT_ACTION.into())}
    require_installed_at(&root,directory).map(|_|())
}
fn require_installed_at(root: &Path, directory: &Path) -> Result<Installation> {
    if !root.join(STATE).exists() {return Err(INSTALL_NEXT_ACTION.into())}
    let cfg=read_state(root)?;
    if cfg.launch_state!=directory {return Err(INSTALL_NEXT_ACTION.into())}
    Ok(cfg)
}
pub fn require_current_registration(directory: &Path, record: &launch::Registration) -> Result<()> {
    let root=default_root()?;
    if directory!=launch::default_directory()? {return Err(INSTALL_NEXT_ACTION.into())}
    require_current_registration_at(&root,directory,record)
}
fn require_current_registration_at(root: &Path, directory: &Path, record: &launch::Registration) -> Result<()> {
    let cfg=require_installed_at(root,directory)?;
    if registration_needs_repair(root,&cfg,record)? {return Err("The launch registration changed while opening. Open PhotoFindry again to repair it, or run the latest verified installer. Your library is kept.".into())}
    Ok(())
}
pub async fn prefer_installed(directory: &Path) -> Result<()> {
    if directory != launch::default_directory()? {return Ok(())}
    let root = default_root()?;
    prefer_installed_at(&root,directory).await
}
async fn prefer_installed_at(root: &Path, directory: &Path) -> Result<()> {
    if !root.join(STATE).exists() {return Ok(())}
    recover(root,false).await?;
    let cfg=read_state(root)?;
    if cfg.launch_state!=directory {return Err("The installed companion has another launch registration. Open its verified installer to review this account's setup.".into())}
    // A healthy current broker needs no package rehash or restart. Starting its
    // service later still performs the normal signed-package verification.
    if let Ok(record)=launch::read_registration(directory) {
        if !registration_needs_repair(root,&cfg,&record)? {
            if root.join("link-repair.json").exists() || !entry_application(root).is_dir() {
                // A URL-preference failure must not stop a healthy data plane.
                // An explicit later Open is a bounded opportunity to repair it.
                if let Some(operation)=launch::try_lock(root,"operation.lock")? {
                    register_links(root,&cfg).await?;
                    drop(operation);
                }
            }
            return Ok(())
        }
    }
    // --open-after runs while its updater still owns operation.lock. Read-only
    // current registration above must remain usable; only repair takes the lock.
    let _operation=launch::try_lock(root,"operation.lock")?.ok_or("Companion maintenance is still finishing. Wait a moment, then open PhotoFindry again.")?;
    let cfg=read_state(root)?;
    if cfg.launch_state!=directory {return Err("The installed companion's launch registration changed. Reopen its verified installer to review setup.".into())}
    checked_release(root,cfg.current,&verifier(&self_executable()?)?)?;
    repair_registration(root,&cfg)?;
    register_links(root,&cfg).await
}
async fn restore(
    root: &Path,
    tx: &Transaction,
    helper: &Path,
    restart: bool,
    guard: Option<&launch::LifecycleGuard>,
) -> Result<()> {
    if let Ok(current) = launch::read_registration(&tx.after.launch_state) {
        if current.executable == executable(root, tx.after.current) {
            drain(&tx.after).await?;
        }
    }
    if restart {
        stop_agent(root).await?;
    }
    if let Some(before) = &tx.before {
        let mut before = if checked_release(root, before.current, helper).is_ok() {
            before.clone()
        } else {
            // An explicit corrupt-current recovery cannot restore the broken
            // executable it is replacing. Complete its independently verified
            // recovery target while retaining the same library and watermark.
            checked_release(root, tx.after.current, helper)?;
            tx.after.clone()
        };
        before.highest = before.highest.max(tx.after.highest);
        let before = &before;
        publish_registration(root, before, guard)?;
        save(root, STATE, before)?;
        write_agent(root, before)?;
        if restart {
            note(root, "opening", "Restarting the retained companion with the current library.");
            start_agent(root, before, true).await?;
            health(root, before).await?;
        }
    } else {
        if let Some(prior) = &tx.registration {
            if prior.data_dir != tx.after.data {
                return Err("Recovery registration belongs to another library. No registration was restored.".into());
            }
            if let Some(guard) = guard {
                guard.restore_registration(&tx.after.launch_state, prior)?;
            } else {
                let guard = launch::LifecycleGuard::acquire(&tx.after.launch_state)?;
                guard.restore_registration(&tx.after.launch_state, prior)?;
            }
        } else if let Ok(current) = launch::read_registration(&tx.after.launch_state) {
            if current.executable == executable(root, tx.after.current) {
                let _ =
                    launch::private_file(&tx.after.launch_state.join(launch::REGISTRATION), false)?;
                fs::remove_file(tx.after.launch_state.join(launch::REGISTRATION))
                    .map_err(|_| "Cannot recover prior launch state")?;
            }
        }
        clear(root, STATE)?;
        let path = agent_path(root)?;
        if path.exists() {
            let _ = release::read(&path, 16384)?;
            fs::remove_file(path).map_err(|_| "Cannot recover autostart state")?;
        }
    }
    clear(root, JOURNAL)?;
    note(root,"recovered","Prior software restored. Your current library and saved work were not rolled back. Open PhotoFindry or retry the update.");
    Ok(())
}
pub async fn recover(root: &Path, restart: bool) -> Result<()> {
    if !root.join(JOURNAL).exists() {
        return Ok(());
    }
    let _owner = launch::private_directory(root, false)?;
    let _lock = launch::try_lock(root, "operation.lock")?.ok_or(
        "A companion update is still running. Wait for it to finish, then open PhotoFindry again.",
    )?;
    let tx: Transaction = serde_json::from_slice(&release::read(&root.join(JOURNAL), 65536)?)
        .map_err(|_| {
            "Recovery metadata is invalid. Keep the library and reinstall the verified companion."
        })?;
    if tx.schema != 1 {
        return Err("Unsupported recovery transaction".into());
    }
    validate_state(root, &tx.after)?;
    if let Some(before) = &tx.before {
        validate_state(root, before)?;
        if before.data != tx.after.data || before.launch_state != tx.after.launch_state {
            return Err("Recovery paths disagree; reinstall the verified companion.".into());
        }
    }
    let _guard = launch::LifecycleGuard::acquire(&tx.after.launch_state)?;
    let helper = verifier(&self_executable()?)?;
    if tx.committed {
        let current = read_state(root)?;
        if current != tx.after {
            return Err(
                "Committed update state changed; use the verified installer to recover.".into(),
            );
        }
        checked_release(root, current.current, &helper)?;
        clear(root, JOURNAL)?;
        return Ok(());
    }
    restore(root, &tx, &helper, restart, Some(&_guard)).await
}
/// Recovery is permitted to replace corrupt/missing current program bytes with
/// an independently authenticated retained version. No old executable or library
/// backup is needed, and the anti-replay watermark never decreases.
async fn recover_broken_current(root: &Path, before: Installation, reason: &str) -> Result<()> {
    let helper = verifier(&self_executable()?)?;
    let previous = before.previous.filter(|build| *build != before.current)
        .ok_or_else(||format!("The current companion cannot be authenticated: {reason} No earlier signed program is available. Run verified setup again; your library is kept."))?;
    let prior=checked_release(root,previous,&helper).map_err(|error|format!("The current companion cannot be authenticated: {reason} Retained recovery software also failed verification: {error} Run verified setup again; your library is kept."))?;
    launch::ensure_available(&before.launch_state)?;
    if let Some(record)=launch::recovery_registration(&before.launch_state)? {
        if record.data_dir!=before.data {return Err("Another library owns the launch registration. No recovery program was selected.".into());}
    }
    preflight_background_access(root,&before.data,&self_executable()?).await?;
    let guard=launch::LifecycleGuard::acquire(&before.launch_state)?;
    // A valid modern broker can still drain its owned runtime even if someone
    // removed its executable on disk. Do not force-kill a busy host or its model.
    launch::request_update_stop(&before.launch_state).await?;
    let deadline=tokio::time::Instant::now()+Duration::from_secs(60);
    loop {
        if let Some(broker)=launch::try_lock(&before.launch_state,"broker.lock")? {
            if super::handover::validate_admission(&before.data).is_ok() {drop(broker);break;}
        }
        if tokio::time::Instant::now()>=deadline {return Err("The existing library host is still saving work. Wait before recovery; no program or library was replaced.".into());}
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut after=before.clone();after.current=prior.build;after.previous=Some(before.current);after.highest=before.highest.max(prior.build);
    let mut tx=Transaction{schema:1,before:Some(before.clone()),after:after.clone(),registration:launch::recovery_registration(&before.launch_state)?,committed:false};
    save(root,JOURNAL,&tx)?;
    failure(root,"corrupt-current",reason);
    let result=async {
        stop_agent(root).await?;
        publish_registration(root,&after,Some(&guard))?;
        save(root,STATE,&after)?;
        note(root,"opening","Opening retained verified software with the current library. Saved work stays in place.");
        start_agent(root,&after,true).await?;health(root,&after).await?;
        tx.committed=true;save(root,JOURNAL,&tx)?;clear(root,JOURNAL)?;
        Ok::<(),String>(())
    }.await;
    if let Err(error)=result {failure(root,"corrupt-current-recovery",&error);return Err(format!("The retained verified program could not finish opening: {error} Recovery remains recorded; retry Recover or verified setup. Your library stays in place."));}
    drop(guard);register_links(root,&after).await?;
    note(root,"ready","Retained verified companion software is ready. Your current library and saved work were kept.");
    Ok(())
}
async fn switch(
    root: &Path,
    before: Installation,
    release: &release::Release,
    rollback: bool,
    open: bool,
) -> Result<()> {
    let helper = verifier(&self_executable()?)?;
    checked_release(root, release.build, &helper)?;
    if !rollback && (release.build <= before.current || release.build < before.highest) {
        return Err("The update is older than this installation. Current software is kept.".into());
    }
    let mut after = before.clone();
    after.previous = Some(before.current);
    after.current = release.build;
    after.highest = before.highest.max(release.build);
    // Repair an idle stale/missing link before draining the installed service.
    // Only verified installed software may replace that registration; the
    // recovery reader still rejects a different retained library even if its
    // old executable has been removed. An active older host is never rebound.
    checked_release(root, before.current, &helper)?;
    repair_registration(root, &before)?;
    preflight_background_access(root, &before.data, &self_executable()?).await?;
    // Busy transfer/job refusal precedes the durable transition and all launch changes.
    let guard = launch::LifecycleGuard::acquire(&before.launch_state)?;
    note(
        root,
        "waiting",
        "Finishing owned work before updating. Pause busy work in Jobs if requested.",
    );
    drain(&before).await?;
    let mut tx = Transaction {
        schema: 1,
        registration: Some(launch::read_registration(&before.launch_state)?),
        before: Some(before.clone()),
        after: after.clone(),
        committed: false,
    };
    save(root, JOURNAL, &tx)?;
    let outcome = async {
        stop_agent(root).await?;
        publish_registration(root, &after, Some(&guard))?;
        save(root, STATE, &after)?;
        note(root, "opening", "Opening the updated library in the background. Saved work remains in place.");
        start_agent(root, &after, true).await?;
        health(root, &after).await?;
        tx.committed = true;
        save(root, JOURNAL, &tx)?;
        clear(root, JOURNAL)?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(reason) = outcome {
        failure(root, "update", &reason);
        if let Err(recovery) = restore(root, &tx, &helper, true, Some(&guard)).await {
            return Err(format!("{reason} Recovery is pending: {recovery} Run lifecycle recover; do not restore or delete your library."));
        }
        return Err(reason);
    }
    drop(guard);
    register_links(root, &after).await?;
    note(
        root,
        "ready",
        "Companion updated. Your library, original photographs and selected models are kept.",
    );
    if open {
        let _ = tokio::process::Command::new(executable(root, after.current))
            .args(["open", "--launch-state"])
            .arg(&after.launch_state)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    Ok(())
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EntryIdentity { schema: u32, id: String }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EntryState { schema: u32, build: u64, helper_sha256: String }
fn entry_application(root: &Path) -> PathBuf { root.join("entry/PhotoFindry.app") }
fn entry_identity(root: &Path) -> Result<String> {
    if root.join("entry-identity.json").exists() {
        let raw=release::read(&root.join("entry-identity.json"),4096)?;
        if let Ok(identity)=serde_json::from_slice::<EntryIdentity>(&raw) {
            if identity.schema==1 && identity.id.len()==32 && identity.id.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)) {return Ok(identity.id);}
        }
        // This is disposable program registration metadata. Preserve damaged
        // bytes before issuing a fresh identity; no library selection changes.
        launch::private_file(&root.join("entry-identity.json"),false)?;
        let mut nonce=[0u8;16];getrandom::fill(&mut nonce).map_err(|_|"Cannot preserve damaged Open entry metadata")?;
        fs::rename(root.join("entry-identity.json"),root.join(format!(".damaged-entry-identity-{}.json",release::digest(&nonce)))).map_err(|_|"Cannot preserve damaged Open entry metadata")?;
    }
    let mut nonce = [0u8; 16]; getrandom::fill(&mut nonce).map_err(|_| "Cannot identify the installed Open entry")?;
    let id = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    save(root, "entry-identity.json", &EntryIdentity {schema: 1, id: id.clone()})?;
    Ok(id)
}
fn entry_info(id: &str, build: u64) -> Vec<u8> {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>com.photofindry.companion.entry.{id}</string><key>CFBundleName</key><string>Open PhotoFindry</string><key>CFBundleDisplayName</key><string>Open PhotoFindry</string><key>CFBundleExecutable</key><string>CompanionLauncher</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>{build}</string><key>LSUIElement</key><true/><key>LSMinimumSystemVersion</key><string>14.0</string><key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>PhotoFindry local launch</string><key>CFBundleURLSchemes</key><array><string>photofindry-companion</string><string>photofindry-local</string></array><key>CFBundleTypeRole</key><string>Viewer</string></dict><dict><key>CFBundleURLName</key><string>PhotoFindry archive navigation</string><key>CFBundleURLSchemes</key><array><string>photofindry</string></array><key>CFBundleTypeRole</key><string>Viewer</string></dict></array></dict></plist>").into_bytes()
}
async fn sealed_entry(path: &Path) -> bool {
    tokio::time::timeout(Duration::from_secs(15), tokio::process::Command::new("/usr/bin/codesign").args(["--verify", "--strict"]).arg(path)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).status())
        .await.is_ok_and(|result| result.is_ok_and(|status| status.success()))
}
async fn provision_entry(root: &Path, cfg: &Installation) -> Result<PathBuf> {
    let id = entry_identity(root)?;
    let app = entry_application(root);
    let existing: Option<EntryState> = release::read(&root.join("entry-state.json"), 4096).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let source_build = existing.as_ref().filter(|state| state.schema == 1 && state.build <= cfg.highest).map(|state| state.build.max(cfg.current)).unwrap_or(cfg.current);
    if let Some(existing) = &existing {
        if existing.schema == 1 && existing.build == source_build && app.is_dir()
            && release::file_hash(&app.join("Contents/MacOS/CompanionLauncher"), 32*1024*1024).ok().as_deref() == Some(existing.helper_sha256.as_str())
            && release::read(&app.join("Contents/Resources/installed-entry.json"),4096).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok()) == Some(json!({"schema":1,"installRoot":root}))
            && sealed_entry(&app).await { return Ok(app); }
    }
    let helper = verifier(&self_executable()?)?;
    let envelope = release::read(&root.join("versions").join(source_build.to_string()).join("release.json"), 131072)?;
    let signed = release::verify(&envelope, &helper)?;
    if signed.build != source_build { return Err("The installed Open entry source version changed".into()); }
    let source_root = package(root, source_build);
    let source_name = Path::new("bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher");
    let inventory = release::inventory(&source_root, &signed)?;
    let expected = inventory.get(source_name).ok_or("The signed release has no native Open entry")?;
    let source = launch::canonical(&source_root.join(source_name), false)?;
    if release::file_hash(&source, 32*1024*1024)? != *expected { return Err("The native Open entry does not match its publisher-signed inventory".into()); }
    let entry_root = root.join("entry"); launch::private_directory(&entry_root, true)?;
    let mut nonce=[0u8;16];getrandom::fill(&mut nonce).map_err(|_|"Cannot stage installed Open entry")?;
    let stage=Stage::new(entry_root.join(format!(".stage-{}",release::digest(&nonce))));
    let staged=stage.0.join("PhotoFindry.app");
    for path in [staged.join("Contents/MacOS"),staged.join("Contents/Resources")] {launch::private_directory(&path,true)?;}
    let bootstrap=staged.join("Contents/Resources/bootstrap");launch::private_directory(&bootstrap,true)?;
    for name in ["photofindry","CompanionReleaseVerify","CompanionLauncher"] {
        let relative=PathBuf::from(format!("bin/{name}"));
        let expected=inventory.get(&relative).ok_or("The signed release has no complete recovery controller")?;
        let original=launch::canonical(&source_root.join(&relative),false)?;
        if release::file_hash(&original,64*1024*1024)?!=*expected {return Err("A recovery controller does not match the publisher-signed inventory".into());}
        let copied=bootstrap.join(name);fs::copy(&original,&copied).map_err(|_|"Cannot preserve the authenticated recovery controller")?;
        fs::set_permissions(&copied,fs::Permissions::from_mode(0o700)).map_err(|_|"Cannot protect the recovery controller")?;
        fs::File::open(&copied).and_then(|file|file.sync_all()).map_err(|_|"Cannot flush the recovery controller")?;
    }
    atomic(&staged.join("Contents/Resources"),"bootstrap-release.json",&envelope)?;
    atomic(&staged.join("Contents/Resources"),"bootstrap-SHA256SUMS.txt",&release::read(&source_root.join("SHA256SUMS.txt"),2*1024*1024)?)?;
    let binary=staged.join("Contents/MacOS/CompanionLauncher");
    fs::copy(&source,&binary).map_err(|_|"Cannot copy the authenticated native Open entry")?;
    fs::set_permissions(&binary,fs::Permissions::from_mode(0o700)).map_err(|_|"Cannot protect the installed Open entry")?;
    atomic(&staged.join("Contents"),"Info.plist",&entry_info(&id,source_build))?;
    save(&staged.join("Contents/Resources"),"installed-entry.json",&json!({"schema":1,"installRoot":root}))?;
    let signed_status=tokio::time::timeout(Duration::from_secs(20),tokio::process::Command::new("/usr/bin/codesign").args(["--force","--sign","-"]).arg(&staged)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).status()).await
        .map_err(|_|"Sealing the installed Open entry timed out")?.map_err(|_|"Cannot seal the installed Open entry")?;
    if !signed_status.success() || !sealed_entry(&staged).await {return Err("The locally generated Open entry did not pass its code seal. The signed program is kept.".into());}
    for path in [&binary,&staged.join("Contents/_CodeSignature/CodeResources")] {fs::File::open(path).and_then(|file|file.sync_all()).map_err(|_|"Cannot flush the generated entry seal")?;}
    let final_hash=release::file_hash(&binary,32*1024*1024)?;
    if app.exists() {
        launch::private_directory(&app,false)?;
        let config=release::read(&app.join("Contents/Resources/installed-entry.json"),4096).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
        if config.as_ref().is_some_and(|config|config.get("installRoot").is_some_and(|path|path!=root.to_string_lossy().as_ref())) {return Err("Another installation owns the Open entry. No application was replaced.".into());}
        swap_paths(&staged,&app)?;
        // Keep prior program contents without an app bundle layout that
        // LaunchServices could rediscover under the same URL-handler identity.
        let quarantine=entry_root.join(format!(".damaged-entry-{}",release::digest(&nonce)));
        if fs::rename(&staged,&quarantine).is_err() {
            if swap_paths(&staged,&app).is_err() {stage.preserve();return Err(format!("Open-entry repair could not restore its original directory. Retained program bytes stay at {}. Rerun verified setup; your library was not changed.",stage.0.display()));}
            return Err("Cannot preserve the prior Open entry for recovery. It was kept in place.".into());
        }
        let old_info=quarantine.join("Contents/Info.plist");
        if old_info.exists() {fs::rename(&old_info,quarantine.join("prior-Info.plist")).map_err(|_|"The prior Open entry is preserved but its old registration metadata could not be retired. Rerun verified setup.")?;}
    } else {fs::rename(&staged,&app).map_err(|_|"Cannot publish the installed Open entry")?;}
    launch::private_directory(&entry_root,false)?.sync_all().map_err(|_|"Cannot flush the installed Open entry")?;
    save(root,"entry-state.json",&EntryState{schema:1,build:source_build,helper_sha256:final_hash})?;
    Ok(app)
}
async fn register_links(root: &Path, cfg: &Installation) -> Result<()> {
    if root != default_root()? || cfg.launch_state != launch::default_directory()? { return Ok(()); }
    let Some(_entry)=launch::try_lock(root,"entry.lock")? else {return Ok(());};
    let outcome=async {
        let app=provision_entry(root,cfg).await?;
        let mut child=launch::desktop_command(&app.join("Contents/MacOS/CompanionLauncher")).arg("--register-links")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(|_|"The installed Open entry could not start")?;
        let status=match tokio::time::timeout(Duration::from_secs(105),child.wait()).await {
            Ok(Ok(status))=>status,
            _=>{let _=child.kill().await;let _=child.wait().await;return Err("macOS Open-link repair timed out. The verified companion remains ready.".into())}
        };
        if !status.success() {return Err("macOS did not select the installed Open entry. The verified companion remains ready; rerun verified setup to repair its Open link.".into());}
        clear(root,"link-repair.json")?;
        Ok::<(),String>(())
    }.await;
    if let Err(reason)=outcome {
        failure(root,"link-registration",&reason);
        save(root,"link-repair.json",&json!({"message":reason,"updatedAt":now()}))?;
        note(root,"ready-with-link-repair","The verified companion is ready. Its macOS Open link needs repair; rerun verified setup. Your current library stays open.");
    }
    Ok(())
}
async fn download(url: &str, path: &Path, maximum: u64) -> Result<()> {
    if !(url == release::FEED
        || url.starts_with("https://github.com/sentrybottale/photofindryapp/releases/download/v"))
    {
        return Err("Unsupported update destination".into());
    }
    let feed=url==release::FEED;
    let mut child = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--connect-timeout",
            if feed {"4"} else {"15"},
            "--max-time",
            if feed {"8"} else {"600"},
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot download the signed update. Check your connection and retry.")?;
    let mut output = child
        .stdout
        .take()
        .ok_or("Missing update download stream")?;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .await
        .map_err(|_| "Cannot create private download file")?;
    let transfer = async {
        let mut buffer = [0u8; 65536];
        let mut total = 0u64;
        loop {
            let n = output
                .read(&mut buffer)
                .await
                .map_err(|_| "Update download was interrupted")?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > maximum {
                return Err("Update download exceeded its signed bound".to_owned());
            }
            file.write_all(&buffer[..n])
                .await
                .map_err(|_| "Cannot save update; check free disk space")?;
        }
        file.sync_all()
            .await
            .map_err(|_| "Cannot flush update download")?;
        let status = child
            .wait()
            .await
            .map_err(|_| "Cannot finish update download")?;
        if !status.success() {
            return Err("Could not download the update. Your current version is kept; check your connection and retry.".into());
        }
        Ok::<(), String>(())
    };
    match tokio::time::timeout(if feed {Duration::from_secs(12)} else {Duration::from_secs(620)}, transfer).await {
        Ok(Ok(())) => Ok(()),
        result => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = fs::remove_file(path);
            Err(result
                .ok()
                .and_then(|r| r.err())
                .unwrap_or_else(|| "Update download timed out; retry when connected.".into()))
        }
    }
}
async fn fetch(root: &Path, helper: &Path) -> Result<(release::Release, Vec<u8>)> {
    let path = root.join(format!("feed-{}.tmp", std::process::id()));
    download(release::FEED, &path, 131072).await?;
    let bytes = release::read(&path, 131072);
    let _ = fs::remove_file(path);
    let bytes = bytes?;
    let value = release::verify(&bytes, helper)?;
    Ok((value, bytes))
}
async fn check(root: &Path) -> Result<Value> {
    let cfg = read_state(root)?;
    let helper = verifier(&self_executable()?)?;
    let (latest, envelope) = fetch(root, &helper).await?;
    if latest.build < cfg.highest {
        return Err(
            "The server offered an older release. Current software is kept; check again later."
                .into(),
        );
    }
    save(
        root,
        "available.json",
        &json!({"release":latest,"envelope":serde_json::from_slice::<Value>(&envelope).map_err(|_|"Invalid verified envelope")?,"checkedAt":now()}),
    )?;
    note(
        root,
        if latest.build > cfg.current {
            "available"
        } else {
            "ready"
        },
        if latest.build > cfg.current {
            "A signed companion update is available. Install when ready; your library stays in place."
        } else {
            "Your companion is up to date."
        },
    );
    status(root)
}
fn stage_version(root: &Path) -> Result<PathBuf> {
    let versions = root.join("versions");
    if !versions.exists() {
        fs::create_dir(&versions).map_err(|_| "Cannot create version store")?;
        fs::set_permissions(&versions, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect version store")?;
    }
    launch::private_directory(&versions, false)?;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| "Cannot stage update")?;
    let stage = versions.join(format!(".stage-{}", release::digest(&nonce)));
    fs::create_dir(&stage).map_err(|_| "Cannot create update stage")?;
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot protect update stage")?;
    Ok(stage)
}
struct Stage(PathBuf, std::cell::Cell<bool>);
impl Stage {
    fn new(path: PathBuf) -> Self {Self(path,std::cell::Cell::new(true))}
    fn preserve(&self) {self.1.set(false);}
}
impl Drop for Stage {
    fn drop(&mut self) {
        if self.1.get() {let _ = fs::remove_dir_all(&self.0);}
    }
}
fn swap_paths(first: &Path, second: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let first=std::ffi::CString::new(first.as_os_str().as_bytes()).map_err(|_|"Invalid program path")?;
    let second=std::ffi::CString::new(second.as_os_str().as_bytes()).map_err(|_|"Invalid program path")?;
    if unsafe{libc::renameatx_np(libc::AT_FDCWD,first.as_ptr(),libc::AT_FDCWD,second.as_ptr(),libc::RENAME_SWAP)}!=0 {
        return Err("Cannot atomically replace the verified program; the prior program is kept.".into());
    }
    Ok(())
}
fn commit_version(
    root: &Path,
    staged: &Stage,
    release: &release::Release,
    envelope: &[u8],
) -> Result<()> {
    let stage=&staged.0;
    release::verify_package(&stage.join("package"), release)?;
    atomic(stage, "release.json", envelope)?;
    let target = root.join("versions").join(release.build.to_string());
    if target.exists() {
        let old = release::read(&target.join("release.json"), 131072).ok();
        if let Some(old)=&old {
            if old!=envelope && release::verify(old,&verifier(&self_executable()?)?).is_ok() {
                return Err("This build already has a different signed identity. Keep the current version and contact support.".into());
            }
        }
        if old.as_deref()==Some(envelope) && release::verify_package(&target.join("package"), release).is_ok() {return Ok(());}
        // Same signed program bytes can be repaired without changing the
        // selected build or interrupting an already-running owned job. Keep all
        // damaged/unexpected program contents in quarantine; never remove data.
        launch::private_directory(&target,false)?;
        swap_paths(stage,&target)?;
        let quarantine=root.join("versions").join(format!(".damaged-{}-{}",release.build,stage.file_name().ok_or("Invalid repair stage")?.to_string_lossy()));
        if let Err(_)=fs::rename(stage,&quarantine) {
            if swap_paths(stage,&target).is_err() {
                staged.preserve();
                return Err(format!("Program repair could not restore its original directory. Damaged bytes are retained at {}. Keep that directory and rerun verified setup; your library was not changed.",stage.display()));
            }
            return Err("Cannot preserve the damaged program for recovery. The previous version directory is kept.".into());
        }
        launch::private_directory(&root.join("versions"),false)?.sync_all().map_err(|_|"Cannot flush repaired program version")?;
        save(root,"program-repair.json",&json!({"build":release.build,"inventorySHA256":release.inventory_sha256,"updatedAt":now()}))?;
        return Ok(());
    }
    fs::rename(stage, &target)
        .map_err(|_| "Cannot publish verified program version; current software is kept")?;
    launch::private_directory(&root.join("versions"), false)?
        .sync_all()
        .map_err(|_| "Cannot flush verified version")?;
    Ok(())
}
async fn update(root: &Path, local: Option<&Path>, open: bool) -> Result<()> {
    let cfg = read_state(root)?;
    let helper = verifier(&self_executable()?)?;
    let (next, bytes) = if let Some(path) = local {
        let raw = release::read(&path.join("release.json"), 131072)?;
        (release::verify(&raw, &helper)?, raw)
    } else {
        fetch(root, &helper).await?
    };
    update_verified(root,cfg,next,bytes,local,open).await
}
async fn update_verified(root:&Path,cfg:Installation,next:release::Release,bytes:Vec<u8>,local:Option<&Path>,open:bool)->Result<()> {
    if next.build <= cfg.current || next.build < cfg.highest {
        return Err("No newer signed release is available. Current software is unchanged.".into());
    }
    let retained=root.join("versions").join(next.build.to_string());
    if retained.exists() {
        note(root,"verifying","Verifying retained signed companion software before reopening the library.");
        let helper=verifier(&self_executable()?)?;
        if let Ok(installed)=release::read(&retained.join("release.json"),131072) {
            if installed!=bytes && release::verify(&installed,&helper).is_ok() {existing_release_identity(next.build,next.build,&installed,&bytes)?;}
            if installed==bytes && checked_release(root,next.build,&helper).is_ok() {return switch(root,cfg,&next,false,open).await;}
        }
        note(root,"verifying","Retained program bytes need repair. Fetching a fresh authenticated copy; the current library remains available.");
    }
    let stage = Stage::new(stage_version(root)?);
    if let Some(path) = local {
        release::copy_package(&path.join("package"), &stage.0.join("package"), &next)?;
    } else {
        let zip = stage.0.join("download.zip");
        note(
            root,
            "downloading",
            "Downloading a signed companion update. The current library remains available.",
        );
        download(&next.archive_url, &zip, next.archive_bytes).await?;
        release::extract(&zip, &stage.0.join("package"), &next)?;
        fs::remove_file(zip).map_err(|_| "Cannot finish private update download")?;
    }
    commit_version(root, &stage, &next, &bytes)?;
    switch(root, cfg, &next, false, open).await
}
/// An explicit Open attempts an authenticated update before choosing the local
/// UI. Offline or busy work must never strand a usable verified installation.
/// Background-check preferences affect only the six-hour unattended check.
pub async fn refresh_on_open(directory:&Path)->Result<()> {
    if directory!=launch::default_directory()? {return Ok(())}
    let root=default_root()?;
    independent_refresh_on_open(&root).await
}

fn request_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read_request(root: &Path, name: &str, expected: &str) -> Result<OpenRequest> {
    let request: OpenRequest = serde_json::from_slice(&release::read(&root.join(name), 16384)?)
        .map_err(|_| "The companion update receipt is invalid. Open PhotoFindry again to recover.")?;
    if request.schema != 1 || !request_id(&request.id) || request.id != expected {
        return Err("The companion update request changed. Open PhotoFindry again to use its current state.".into());
    }
    Ok(request)
}
async fn finish_open_request(root: &Path, expected: &str) -> Result<()> {
    let mut receipt = read_request(root, OPEN_REQUEST, expected)?;
    if receipt.complete { return Err("This companion update request has already finished.".into()); }
    receipt.pid = Some(std::process::id());
    receipt.updated_at = now();
    save(root, OPEN_REQUEST, &receipt)?;
    let result = refresh_on_open_at(root).await;
    if let Err(reason) = &result { failure(root, "open-update", reason); }
    receipt.complete = true;
    receipt.updated_at = now();
    receipt.error = result.as_ref().err().cloned();
    save(root, OPEN_REQUEST, &receipt)?;
    result
}
async fn independent_refresh_on_open(root: &Path) -> Result<()> {
    if !root.join(STATE).exists() { return Ok(()); }
    let _owner = launch::private_directory(root, false)?;
    let deadline = tokio::time::Instant::now() + OPEN_OPERATION_TIMEOUT;
    let _schedule = loop {
        if let Some(lock) = launch::try_lock(root, "schedule.lock")? { break lock; }
        if tokio::time::Instant::now() >= deadline {
            return Err("The signed update is still finishing independently. Your library is kept; open PhotoFindry again after it completes.".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let operation = launch::try_lock(root, "operation.lock")?
        .ok_or("Companion maintenance is active. Wait for it to finish, then open PhotoFindry again.")?;
    let cfg = read_state(root)?;
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|_| "Cannot identify the signed update operation")?;
    let id = release::digest(&nonce);
    let receipt = OpenRequest { schema: 1, id: id.clone(), complete: false, error: None, pid: None, updated_at: now() };
    save(root, OPEN_REQUEST, &receipt)?;
    atomic(root, "maintenance.plist", &plist_with_request(root, &cfg, Some("open-update"), Some(&id))?)?;
    // launchd owns the maintenance process, independently of the URL helper and
    // foreground Open process. A closed helper cannot kill an authenticated switch.
    unload(format!("{}/{}.maintenance", domain(), label(root)?)).await?;
    drop(operation);
    if let Err(reason) = launchctl(&["bootstrap".into(), domain(), root.join("maintenance.plist").to_string_lossy().into_owned()], true).await {
        failure(root, "open-update-start", &reason);
        open_update_warning(root, "The signed update could not start. The installed verified companion is opening; retry Open PhotoFindry later.");
        return Ok(());
    }
    loop {
        let receipt = read_request(root, OPEN_REQUEST, &id)?;
        if receipt.complete { return receipt.error.map_or(Ok(()), Err); }
        let exited = receipt.pid.is_some_and(|pid| {
            (unsafe { libc::kill(pid as libc::pid_t, 0) }) != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        });
        if exited {
            // SIGKILL/power interruption cannot write a completion receipt. The
            // durable transaction remains authoritative; recover software and
            // continue Open instead of leaving the user waiting for a dead job.
            tokio::time::sleep(Duration::from_millis(100)).await;
            if read_request(root, OPEN_REQUEST, &id)?.complete {continue;}
            failure(root,"open-update","The update process stopped before reporting completion. Recovering verified software; the current library is kept.");
            recover(root,false).await?;
            let current=read_state(root)?;
            checked_release(root,current.current,&verifier(&self_executable()?)?)?;
            open_update_warning(root,"The update process stopped. Verified software was recovered and your current library is opening; retry Open PhotoFindry later for the update.");
            return Ok(());
        }
        if receipt.pid.is_none() && now().saturating_sub(receipt.updated_at) >= 45 {
            open_update_warning(root,"macOS has not started the signed update yet. The verified companion is opening while the requested update remains queued.");
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("The signed update is still finishing independently. Your library is kept; open PhotoFindry again after it completes.".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn wait_for_open_maintenance(directory:&Path)->Result<()> {
    if directory!=launch::default_directory()? {return Ok(())}
    let root=default_root()?;
    if !root.exists() {return Ok(())}
    wait_for_open_maintenance_at(&root).await
}
async fn wait_for_open_maintenance_at(root:&Path)->Result<()> {
    let _owner=launch::private_directory(root,false)?;
    let deadline=tokio::time::Instant::now()+OPEN_OPERATION_TIMEOUT;
    loop {
        if let Some(lock)=launch::try_lock(root,"operation.lock")? {drop(lock);return Ok(())}
        if tokio::time::Instant::now()>=deadline {return Err("The companion update is still finishing. Your library is kept; open PhotoFindry again after it completes.".into())}
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn refresh_on_open_at(root:&Path)->Result<()> {
    if !root.join(STATE).exists() {return Ok(())}
    let _owner=launch::private_directory(root,false)?;
    let deadline=tokio::time::Instant::now()+OPEN_OPERATION_TIMEOUT;
    let _lock=loop {
        if let Some(lock)=launch::try_lock(root,"operation.lock")? {break lock}
        if tokio::time::Instant::now()>=deadline {
            return Err("Another companion update is still running. Wait for it to finish, then open PhotoFindry again.".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    recover_locked_before_open(root).await?;
    let cfg=read_state(root)?;
    let helper=verifier(&self_executable()?)?;
    let fetched=fetch(root,&helper).await;
    let (next,bytes)=match fetched {
        Ok(value)=>value,
        Err(error)=>{open_update_warning(root,&format!("Update check could not finish: {error} The installed verified companion is opening."));return Ok(())}
    };
    if next.build<cfg.highest {
        open_update_warning(root,"The update feed offered an older build. The installed verified companion is opening.");
        return Ok(());
    }
    if next.build<=cfg.current {clear(root,"open-update.json")?;return Ok(())}
    let _=save(root,"available.json",&json!({"release":&next,"envelope":serde_json::from_slice::<Value>(&bytes).map_err(|_|"Invalid verified envelope")?,"checkedAt":now()}));
    match launch::registered_work_busy(&cfg.launch_state,&cfg.data).await {
        Ok(true)=>{open_update_warning(root,"A newer signed companion is ready, but current work is active. Pause it in Jobs or open again after it finishes; this verified version is opening now.");return Ok(())},
        Err(error)=>{open_update_warning(root,&format!("Update readiness could not be checked: {error} The installed verified companion is opening."));return Ok(())},
        Ok(false)=>{},
    }
    note(root,"verifying","A newer signed companion is available. Verifying it before opening the library.");
    match update_verified(root,cfg,next,bytes,None,false).await {
        Ok(())=>{clear(root,"open-update.json")?;Ok(())},
        Err(error)=>{
            if root.join(JOURNAL).exists() {return Err(error)}
            let current=read_state(root)?;
            checked_release(root,current.current,&helper)?;
            open_update_warning(root,&format!("Signed update did not finish: {error} The verified companion is opening with saved work kept."));
            Ok(())
        }
    }
}
fn open_update_warning(root:&Path,message:&str) {
    let bounded:String=message.chars().filter(|c|!c.is_control()).take(512).collect();
    let _=save(root,"open-update.json",&json!({"message":bounded,"checkedAt":now()}));
}
async fn recover_locked_before_open(root:&Path)->Result<()> {
    // The ordinary Open path already calls recover() after this lock is
    // released. Do not recursively acquire operation.lock here.
    if root.join(JOURNAL).exists() {return Err("Companion recovery is pending. Choose Recover and open; your library is kept.".into())}
    Ok(())
}
pub fn status(root: &Path) -> Result<Value> {
    let _owner = launch::private_directory(root, false)?;
    let detail = release::read(&root.join("status.json"), 16384).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let last_failure = release::read(&root.join(LAST_FAILURE), 16384).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    if !root.join(STATE).exists() {
        return Ok(json!({"installed":false,"status":detail,"lastFailure":last_failure,"recoveryPending":root.join(JOURNAL).exists()}));
    }
    let cfg = read_state(root)?;
    let helper = verifier(&self_executable()?)?;
    let release = release::verify(
        &release::read(
            &root
                .join("versions")
                .join(cfg.current.to_string())
                .join("release.json"),
            131072,
        )?,
        &helper,
    )?;
    let detail = release::read(&root.join("status.json"), 16384)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let available = release::read(&root.join("available.json"), 131072)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let open_update=release::read(&root.join("open-update.json"),4096)
        .ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok());
    Ok(
        json!({"installed":true,"entryRepairPending":root.join("link-repair.json").exists(),"linkRepair":release::read(&root.join("link-repair.json"),4096).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok()),"lastFailure":last_failure,"version":release.version,"build":cfg.current,"previousBuild":cfg.previous,"autostart":cfg.autostart,"automaticUpdates":cfg.automatic_updates,"status":detail,"available":available.as_ref().map(|v|&v["release"]),"openUpdate":open_update,"recoveryPending":root.join(JOURNAL).exists(),"appCleanupPending":cfg.retire.is_some()}),
    )
}
pub async fn start_registered(root: &Path) -> Result<()> {
    recover(root, false).await?;
    let cfg = read_state(root)?;
    note(root, "opening", "Opening the registered companion with the current library.");
    start_agent(root, &cfg, false).await
}
pub async fn schedule(root: &Path, operation: &str) -> Result<()> {
    if !matches!(
        operation,
        "update"
            | "rollback"
            | "recover"
            | "uninstall"
            | "check"
            | "enable-autostart"
            | "disable-autostart"
            | "enable-update-checks"
            | "disable-update-checks"
    ) {
        return Err("Unsupported companion operation".into());
    }
    let _owner = launch::private_directory(root, false)?;
    let schedule_lock = launch::try_lock(root, "schedule.lock")?
        .ok_or("A companion operation is already starting. Wait for it to finish.")?;
    let operation_lock = launch::try_lock(root, "operation.lock")?
        .ok_or("A companion operation is already running. Wait for it to finish.")?;
    let cfg = read_state(root)?;
    atomic(
        root,
        "maintenance.plist",
        &plist(root, &cfg, Some(operation))?,
    )?;
    let job = format!("{}/{}.maintenance", domain(), label(root)?);
    unload(job).await?;
    note(
        root,
        "starting",
        "Starting the requested companion operation. Saved work is kept.",
    );
    drop(operation_lock);
    let _schedule_lock = schedule_lock;
    launchctl(
        &[
            "bootstrap".into(),
            domain(),
            root.join("maintenance.plist")
                .to_string_lossy()
                .into_owned(),
        ],
        true,
    )
    .await
}
async fn retire_migration(root: &Path) -> Result<()> {
    let Some(_lock) = launch::try_lock(root, "operation.lock")? else {
        return Ok(());
    };
    let mut cfg = read_state(root)?;
    let Some(app) = cfg.retire.as_ref() else {
        return Ok(());
    };
    if !app.exists() {
        cfg.retire = None;
        return save(root, STATE, &cfg);
    }
    if app.starts_with(root)
        || root.starts_with(app)
        || cfg.data.starts_with(app)
        || app.starts_with(&cfg.data)
    {
        return Err(
            "Keep the migration installer in place; its location overlaps saved data.".into(),
        );
    }
    let embedded = app.join("Contents/Resources/companion/bin/photofindry");
    if super::browser_package::enclosing(&embedded)?.as_ref() != Some(app) {
        return Err(
            "The migration installer changed. Remove it manually only after checking its contents."
                .into(),
        );
    }
    super::browser_package::verify_layout(app)?;
    let envelope = release::read(&app.join("Contents/Resources/cli-release.json"), 131072)?;
    let identity = release::verify(&envelope, &verifier(&self_executable()?)?)?;
    release::verify_package(&app.join("Contents/Resources/companion"), &identity)?;
    let input = serde_json::to_vec(&json!({"packagePath":app,"libraryPath":cfg.data}))
        .map_err(|_| "Cannot prepare installer cleanup")?;
    let mut child =
        launch::desktop_command(&launch::browser_opener(&executable(root, cfg.current))?)
            .arg("--retire-migration-app")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "Cannot finish installer cleanup")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&input)
            .await
            .map_err(|_| "Cannot request installer cleanup")?;
    }
    let status = tokio::time::timeout(Duration::from_secs(40), child.wait())
        .await
        .map_err(|_| "Installer cleanup is still pending")?
        .map_err(|_| "Installer cleanup did not finish")?;
    if status.success() && !app.exists() {
        cfg.retire = None;
        save(root, STATE, &cfg)?;
    }
    Ok(())
}
/// Recover an unexpected runtime exit twice after it has demonstrated readiness.
/// Never loop on first-start failures, user Quit, or an updater-owned shutdown.
/// Every reopened worker retains the ordinary durable-queue pause contract.
#[derive(Default)]
struct RestartBudget(u8);
impl RestartBudget {
    fn next(&mut self, ready: bool, maintenance: bool, uptime: Duration) -> Option<Duration> {
        if !ready || maintenance { return None; }
        if uptime >= Duration::from_secs(300) { self.0 = 0; }
        if self.0 >= 2 { return None; }
        self.0 += 1;
        Some(Duration::from_secs(self.0.into()))
    }
}
async fn background(root: &Path) -> Result<()> {
    let mut restarts = RestartBudget::default();
    loop {
        let ready = AtomicBool::new(false);
        let started = tokio::time::Instant::now();
        let result = background_once(root, &ready).await;
        let Err(reason) = result else { return Ok(()); };
        failure(root, "runtime", &reason);
        let operation = launch::try_lock(root, "operation.lock")?;
        let maintenance = operation.is_none() || root.join(JOURNAL).exists();
        drop(operation);
        let Some(delay) = restarts.next(ready.load(Ordering::Acquire), maintenance, started.elapsed()) else { return Err(reason); };
        note(root, "recovering", "The companion stopped unexpectedly. Restarting verified software with your current library; processing stays paused.");
        tokio::time::sleep(delay).await;
    }
}
async fn background_once(root: &Path, ready: &AtomicBool) -> Result<()> {
    let _owner = launch::private_directory(root, false)?;
    // During a live switch, the updater holds operation.lock and has already
    // published the candidate registration. A later startup recovers an abandoned switch.
    if let Some(guard) = launch::try_lock(root, "operation.lock")? {
        drop(guard);
        recover(root, false).await?;
    }
    let cfg = read_state(root)?;
    let helper = verifier(&self_executable()?)?;
    checked_release(root, cfg.current, &helper)?;
    let registered = launch::read_registration(&cfg.launch_state)?;
    if registered.executable != executable(root, cfg.current) || registered.data_dir != cfg.data {
        return Err(
            "Companion registration changed. Run lifecycle recover; your library is kept.".into(),
        );
    }
    let mut child = tokio::process::Command::new(&registered.executable)
        .args(["serve", "--managed", "--data-dir"])
        .arg(&cfg.data)
        .arg("--runtime")
        .arg(&registered.runtime)
        .arg("--launch-state")
        .arg(&cfg.launch_state)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot start the background companion")?;
    let mut stderr = child.stderr.take().ok_or("Cannot read companion startup diagnostics")?;
    let mut diagnostics = Some(tokio::spawn(async move {
        let mut tail = Vec::new();
        let mut buffer = [0u8; 1024];
        while let Ok(size) = stderr.read(&mut buffer).await {
            if size == 0 { break; }
            tail.extend_from_slice(&buffer[..size]);
            if tail.len() > 2048 { tail.drain(..tail.len() - 2048); }
        }
        String::from_utf8_lossy(&tail).chars().filter(|c| !c.is_control() || *c == '\n')
            .take(512).collect::<String>().trim().to_owned()
    }));
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "Cannot handle background shutdown")?;
    let mut ticks = tokio::time::interval(Duration::from_secs(30));
    let mut readiness = tokio::time::interval(Duration::from_millis(500));
    let mut reported_ready = false;
    let mut last_check = now().saturating_sub(21540);
    let mut last_retire = now().saturating_sub(570);
    let mut check_child: Option<tokio::process::Child> = None;
    loop {
        tokio::select! {
         status=child.wait()=>{let status=status.map_err(|_|"Cannot inspect companion exit")?;if status.success(){return Ok(())}let detail=if let Some(mut task)=diagnostics.take(){match tokio::time::timeout(Duration::from_secs(2),&mut task).await{Ok(Ok(value))=>value,_=>{task.abort();String::new()}}}else{String::new()};let detail=detail.trim_start_matches("PhotoFindry: ");let reason=if detail.is_empty(){"The companion stopped unexpectedly. Your library and saved work are kept.".to_owned()}else{detail.to_owned()};return Err(reason)},
         _=term.recv()=>{let _=launch::request_stop(&cfg.launch_state).await;if tokio::time::timeout(Duration::from_secs(30),child.wait()).await.is_err(){let _=child.kill().await;let _=child.wait().await;}return Ok(())},
         _=readiness.tick(),if !reported_ready=>{
          if launch::request_health(&cfg.launch_state).await.unwrap_or(false){
           reported_ready=true;
           ready.store(true, Ordering::Release);
           let phase=release::read(&root.join("status.json"),16384).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
           if phase.as_ref().is_some_and(|value|value["phase"]=="opening"||value["phase"]=="recovering"){
            note(root,"ready","The local library is open. Saved work is kept; the queue retains its pause.");
           }
          }
         },
         _=ticks.tick()=>{if now().saturating_sub(last_retire)>=600{last_retire=now();let _=retire_migration(root).await;}if let Some(c)=check_child.as_mut(){if c.try_wait().ok().flatten().is_some(){check_child=None;}}
          if let Ok(current)=read_state(root){if current.automatic_updates&&now().saturating_sub(last_check)>=21600&&check_child.is_none(){last_check=now();check_child=tokio::process::Command::new(executable(root,current.current)).args(["lifecycle","check","--install-root"]).arg(root).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn().ok();}}
         }
        }
    }
}
struct SignalWatch(tokio::task::JoinHandle<()>);
impl Drop for SignalWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn watch_cancellation() -> Result<(tokio::sync::watch::Receiver<bool>, SignalWatch)> {
    let (send, receive) = tokio::sync::watch::channel(false);
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "Cannot protect installer cancellation")?;
    let watch = tokio::spawn(async move {
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
        let _ = send.send(true);
    });
    Ok((receive, SignalWatch(watch)))
}
async fn cancelled(signal: &mut tokio::sync::watch::Receiver<bool>) {
    while !*signal.borrow_and_update() {
        if signal.changed().await.is_err() {
            return;
        }
    }
}
async fn install(
    root: &Path,
    source: &Path,
    envelope: &Path,
    data: &Path,
    rollback: &Path,
    state: &Path,
    autostart: bool,
    automatic_updates: bool,
) -> Result<Value> {
    let (mut cancellation, _signals) = watch_cancellation()?;
    let _owner = launch::private_directory(root, true)?;
    let _lock = launch::try_lock(root, "operation.lock")?
        .ok_or("Another companion installation is running")?;
    if root.starts_with(data) || data.starts_with(root) || state.starts_with(root) {
        return Err(
            "The program installation must be separate from the library and launch state".into(),
        );
    }
    let helper = verifier(&self_executable()?)?;
    let bytes = release::read(envelope, 131072)?;
    let signed = release::verify(&bytes, &helper)?;
    release::verify_package(source, &signed)?;
    preflight_background_access(root, data, &self_executable()?).await?;
    if root.join(STATE).exists() {
        let cfg = read_state(root)?;
        if cfg.data != data || cfg.launch_state != state {
            return Err("Another library is registered for this installation. Keep it in place and open its workspace.".into());
        }
        if signed.build > cfg.current && signed.build >= cfg.highest {
            let stage = Stage::new(stage_version(root)?);
            release::copy_package(source, &stage.0.join("package"), &signed)?;
            commit_version(root, &stage, &signed, &bytes)?;
            switch(root, cfg, &signed, false, false).await?;
        } else {
            if signed.build==cfg.current {
                let installed=release::read(&root.join("versions").join(cfg.current.to_string()).join("release.json"),131072).ok();
                if let Some(installed)=&installed {
                    if installed!=&bytes && release::verify(installed,&helper).is_ok() {existing_release_identity(cfg.current,signed.build,installed,&bytes)?;}
                }
                if checked_release(root,cfg.current,&helper).is_err() {
                    let stage=Stage::new(stage_version(root)?);
                    release::copy_package(source,&stage.0.join("package"),&signed)?;
                    commit_version(root,&stage,&signed,&bytes)?;
                }
            }
            checked_release(root,cfg.current,&helper)?;
            repair_registration(root, &cfg)?;
            note(root, "opening", "Opening the installed companion with the current library.");
            start_agent(root, &cfg, false).await?;
            health(root, &cfg).await?;
            register_links(root, &cfg).await?;
        }
        return status(root);
    }
    if let Some(prior)=launch::recovery_registration(state)? {
        if prior.data_dir!=data {return Err("Another library owns the current launch registration. Open the latest verified installer to keep using that library; no program or library was changed.".into())}
    }
    let stage = Stage::new(stage_version(root)?);
    release::copy_package(source, &stage.0.join("package"), &signed)?;
    commit_version(root, &stage, &signed, &bytes)?;
    if *cancellation.borrow() {
        return Err("Installation cancelled before opening the library. Retry when ready.".into());
    }
    // The existing migration command owns admission/verified recovery, never the installer.
    let exe = executable(root, signed.build);
    let prior = launch::recovery_registration(state)?;
    if let Some(prior)=&prior {
        if prior.data_dir!=data {return Err("The registered library changed during setup. Reopen the installer to use its current location; saved work is kept.".into())}
        launch::request_update_stop(state).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while launch::try_lock(state, "broker.lock")?.is_none() {
            if tokio::time::Instant::now() > deadline {
                return Err(
                    "The previous companion is still closing. Wait, then retry migration.".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let args = vec![
        "migrate-library".to_owned(),
        "--data-dir".into(),
        data.to_string_lossy().into_owned(),
        "--runtime".into(),
        package(root, signed.build)
            .join("runtime")
            .to_string_lossy()
            .into_owned(),
        "--rollback-copy".into(),
        rollback.to_string_lossy().into_owned(),
        "--launch-state".into(),
        state.to_string_lossy().into_owned(),
    ];
    let cfg = Installation {
        schema: 1,
        current: signed.build,
        previous: None,
        highest: signed.build,
        data: data.to_path_buf(),
        launch_state: state.to_path_buf(),
        autostart,
        automatic_updates,
        retire: super::browser_package::enclosing(&self_executable()?)?,
    };
    let mut tx = Transaction {
        schema: 1,
        before: None,
        after: cfg.clone(),
        registration: prior,
        committed: false,
    };
    save(root, JOURNAL, &tx)?;
    let mut child = tokio::process::Command::new(&exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot migrate the local library")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Missing migration progress stream")?;
    let progress = tokio::spawn(async move {
        let mut stdout = stdout;
        let mut buffer = [0u8; 4096];
        let mut pending = Vec::new();
        let mut final_ready = false;
        let mut failure = None;
        loop {
            let n = stdout
                .read(&mut buffer)
                .await
                .map_err(|_| "Cannot read migration progress")?;
            if n == 0 {
                break;
            }
            for byte in &buffer[..n] {
                if *byte == b'\n' {
                    if !pending.is_empty() {
                        let v: Value = serde_json::from_slice(&pending)
                            .map_err(|_| "Invalid migration progress")?;
                        if v["event"] == "progress" {
                            println!("{}", v);
                        } else {
                            final_ready = v["ok"] == true && v["state"] == "ready";
                            if !final_ready {
                                failure = Some(
                                    format!(
                                        "{} {}",
                                        v["message"].as_str().unwrap_or("Migration did not finish"),
                                        v["nextAction"].as_str().unwrap_or("Retry when ready")
                                    )
                                    .chars()
                                    .take(4096)
                                    .collect::<String>(),
                                );
                            }
                        }
                    }
                    pending.clear();
                } else {
                    if pending.len() >= 65536 {
                        return Err("Migration progress exceeded its bound".to_owned());
                    }
                    pending.push(*byte);
                }
            }
        }
        if !pending.is_empty() || !final_ready {
            return Err(failure.unwrap_or_else(|| {
                "Migration did not confirm readiness. Retry when ready.".into()
            }));
        }
        Ok::<(), String>(())
    });
    let migration_status = tokio::select! {
     status=child.wait()=>status.map_err(|_|"Migration process stopped unexpectedly")?,
     _=cancelled(&mut cancellation)=>{if let Some(pid)=child.id(){unsafe{libc::kill(pid as i32,libc::SIGINT);}}if tokio::time::timeout(Duration::from_secs(30),child.wait()).await.is_err(){let _=child.kill().await;let _=child.wait().await;}let _=progress.await;if let Err(recovery)=restore(root,&tx,&helper,true,None).await{return Err(format!("Migration cancelled. Recovery is pending: {recovery} Keep the library and recovery copy in place."));}return Err("Migration cancelled. The previous installation and current library are kept; retry when ready.".into())}
    };
    let output = progress
        .await
        .map_err(|_| "Migration progress reader stopped")?;
    if !migration_status.success() || output.is_err() {
        let reason = output.err().unwrap_or_else(||"The library could not migrate. Keep the existing app and recovery copy, then retry. Your original library was not replaced.".into());
        if let Err(recovery) = restore(root, &tx, &helper, true, None).await {
            return Err(format!("{reason} Recovery is pending: {recovery} Keep the library and recovery copy in place."));
        }
        return Err(reason);
    }
    let result = tokio::select! { result=async {
           save(root, STATE, &cfg)?;
           note(root, "opening", "Opening the saved library in the background. Processing remains paused.");
           start_agent(root, &cfg, true).await?;
           health(root, &cfg).await?;
           register_links(root, &cfg).await?;
           tx.committed = true;
           save(root, JOURNAL, &tx)?;
           clear(root, JOURNAL)?;
           Ok::<(), String>(())
       }
    => result, _=cancelled(&mut cancellation)=>Err("Installation cancelled. Your library is kept; retry when ready.".into()) };
    if let Err(error) = result {
        if let Err(recovery) = restore(root, &tx, &helper, true, None).await {
            return Err(format!("{error} Recovery is pending: {recovery} Keep the library and recovery copy in place."));
        }
        return Err(error);
    }
    note(root,"ready","Background companion installed. Your library is kept; processing remains paused until you start it.");
    status(root)
}
/// Called only after local uninstall confirmation and the owned worker drain.
pub async fn disable_for_uninstall(exe: &Path) -> Result<()> {
    let Some(root) = installed_root(exe) else {
        return Ok(());
    };
    let mut cfg = read_state(&root)?;
    if executable(&root, cfg.current) != exe {
        return Err("Only the active CLI installation can be removed".into());
    }
    cfg.autostart = false;
    cfg.automatic_updates = false;
    save(&root, STATE, &cfg)?;
    stop_agent(&root).await?;
    retained_access_owner::uninstall(&root,&verifier(exe)?)?;
    let probe=root.join("access-controller");
    if probe.exists() {launch::private_directory(&probe,false)?;fs::remove_dir_all(&probe).map_err(|_|"Cannot remove the background-access controller; retry uninstall")?;}
    if root == default_root()? {
        let app = entry_application(&root);
        if app.exists() {
            let config: Value = serde_json::from_slice(&release::read(&app.join("Contents/Resources/installed-entry.json"),4096)?)
                .map_err(|_| "The generated Open entry changed. Keep it in place and retry verified setup before uninstalling.")?;
            if config != json!({"schema":1,"installRoot":root}) { return Err("Another installation owns the Open entry. No application was removed.".into()); }
            let unregistered = tokio::time::timeout(Duration::from_secs(15),tokio::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
                .arg("-u").arg(&app).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).status()).await
                .map_err(|_|"Removing the installed Open link timed out; retry uninstall")?.map_err(|_|"Cannot remove the installed Open link")?;
            if !unregistered.success() {return Err("macOS could not remove the installed Open link. Retry uninstall; your library is kept.".into());}
            fs::remove_dir_all(&app).map_err(|_|"Cannot remove the generated Open entry; retry uninstall")?;
        }
    }
    let path = agent_path(&root)?;
    if path.exists() {
        release::read(&path, 16384)?;
        fs::remove_file(path)
            .map_err(|_| "Cannot remove autostart registration; retry uninstall")?;
    }
    note(
        &root,
        "uninstalling",
        "Autostart is disabled. Completing removal; all saved library data is kept.",
    );
    Ok(())
}
pub fn finish_uninstall(exe: &Path) -> Result<()> {
    if let Some(root) = installed_root(exe) {
        clear(&root, STATE)?;
        note(&root,"uninstalled","Companion removed and autostart disabled. Your library is kept. Reinstall PhotoFindry to reopen it.");
    }
    Ok(())
}
pub async fn command(args: &[String]) -> Result<()> {
    let operation = args.get(1).map(String::as_str).ok_or(
        "Choose lifecycle install, status, check, open-update, update, rollback, recover or background",
    )?;
    let mut root = None;
    let mut data = None;
    let mut rollback = None;
    let mut state = None;
    let mut envelope = None;
    let mut local = None;
    let mut open = false;
    let mut use_current_library = false;
    let mut autostart = true;
    let mut automatic_updates = true;
    let mut enabled = None;
    let mut open_request = None;
    let mut it = args.iter().skip(2);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--open-after" => open = true,
            "--current-library" if !use_current_library => use_current_library = true,
            "--no-autostart" => autostart = false,
            "--no-update-checks" => automatic_updates = false,
            "--open-request" => {
                let id = it.next().ok_or("Choose the companion Open request identity")?;
                if !request_id(id) || open_request.replace(id.clone()).is_some() {
                    return Err("Invalid companion Open request identity".into());
                }
            }
            "--enabled" => {
                enabled = Some(match it.next().map(String::as_str) {
                    Some("true") => true,
                    Some("false") => false,
                    _ => return Err("Choose true or false".into()),
                });
            }
            _ => {
                let slot = match arg.as_str() {
                    "--install-root" => &mut root,
                    "--data-dir" => &mut data,
                    "--rollback-copy" => &mut rollback,
                    "--launch-state" => &mut state,
                    "--release" => &mut envelope,
                    "--from-directory" => &mut local,
                    _ => return Err("Unknown lifecycle option".into()),
                };
                let path = PathBuf::from(it.next().ok_or("Missing lifecycle path")?);
                if !path.is_absolute()
                    || path.components().any(|c| {
                        matches!(
                            c,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )
                    })
                    || slot.replace(path).is_some()
                {
                    return Err("Use one absolute local path per option".into());
                }
            }
        }
    }
    if (!matches!(operation, "install" | "access-check")
        && (use_current_library || data.is_some()
            || rollback.is_some()
            || state.is_some()
            || envelope.is_some()
            || !autostart
            || !automatic_updates))
        || (local.is_some() && operation != "update")
        || (open && !matches!(operation, "update" | "rollback"))
        || (enabled.is_some() && !matches!(operation, "autostart" | "automatic-updates"))
        || (open_request.is_some() && !matches!(operation, "open-update" | "access-check"))
        || (operation == "access-check" && (use_current_library || rollback.is_some() || state.is_some() || envelope.is_some() || !autostart || !automatic_updates || open_request.is_none()))
    {
        return Err("These options do not belong to this lifecycle action".into());
    }
    let root = match root {
        Some(root) => root,
        None => installed_root(&self_executable()?)
            .map(Ok)
            .unwrap_or_else(default_root)?,
    };
    if operation == "access-check" {
        return finish_access_check(&root, &data.ok_or("Choose the library folder for its background access check")?, &open_request.ok_or("Missing background access request identity")?);
    }
    if operation == "install" && root.join(JOURNAL).exists() {
        recover(&root, true).await?;
    }
    if operation == "install" {
        let exe = self_executable()?;
        let source = exe
            .parent()
            .and_then(Path::parent)
            .ok_or("Cannot locate package")?;
        let state=state.map(Ok).unwrap_or_else(launch::default_directory)?;
        let requested=data.ok_or("Choose the existing library")?;
        let selected=if use_current_library {
            if root!=default_root()? {return Err("The installed launcher cannot select another program installation.".into())}
            current_library(&requested,Some(&state))?
        }else{requested};
        let result = install(
            &root,
            source,
            &envelope.ok_or("Choose the signed CLI release envelope")?,
            &selected,
            &rollback.ok_or("Choose a new recovery-copy location")?,
            &state,
            autostart,
            automatic_updates,
        )
        .await;
        if let Err(reason) = &result { failure(&root, "install", reason); }
        let result = result?;
        println!(
            "{}",
            json!({"ok":true,"state":"ready","installation":result})
        );
        return Ok(());
    }
    if operation == "background" {
        let result = background(&root).await;
        if let Err(error) = &result { failure(&root, "background", error); }
        return result;
    }
    if operation == "status" {
        println!("{}", status(&root)?);
        return Ok(());
    }
    if operation == "recover" {
        recover(&root, true).await?;
        let _owner = launch::private_directory(&root, false)?;
        let _operation=launch::try_lock(&root,"operation.lock")?.ok_or("Another companion operation is still running. Wait, then choose Recover and open again.")?;
        let cfg = read_state(&root)?;
        if let Err(reason)=checked_release(&root,cfg.current,&verifier(&self_executable()?)?) {
            return recover_broken_current(&root,cfg,&reason).await;
        }
        repair_registration(&root,&cfg)?;
        note(&root, "opening", "Recovering the background companion with the current library.");
        let current_start = async {
            start_agent(&root, &cfg, false).await?;
            health(&root, &cfg).await
        }.await;
        if let Err(current_error) = current_start {
            if let Some(previous) = cfg.previous.filter(|previous| *previous < cfg.current) {
                let prior = checked_release(&root, previous, &verifier(&self_executable()?)?)
                    .map_err(|error| format!("The current companion did not open: {current_error} Previous software cannot be authenticated: {error} Your library and saved work are kept."))?;
                note(&root, "recovering", "The current companion did not open. Trying the previous verified software; the library stays in place.");
                if let Err(rollback_error) = switch(&root, cfg, &prior, true, false).await {
                    return Err(format!("The current companion did not open: {current_error} The previous verified version could not be restored: {rollback_error} Your library and saved work are kept."));
                }
                note(&root, "ready", "Previous verified companion software is open. Your current library and saved work were kept.");
                return Ok(());
            }
            return Err(format!("{current_error} No earlier verified companion version is available for automatic software recovery. Your library is kept."));
        }
        register_links(&root, &cfg).await?;
        note(
            &root,
            "ready",
            "The background companion is ready. Your current library was kept in place.",
        );
        return Ok(());
    }
    if operation == "repair-entry" {
        let _owner=launch::private_directory(&root,false)?;
        let cfg=read_state(&root)?;
        checked_release(&root,cfg.current,&verifier(&self_executable()?)?)?;
        register_links(&root,&cfg).await?;
        println!("Installed Open entry repair checked. Use lifecycle status for any local warning.");
        return Ok(());
    }
    if operation == "open-update" {
        if let Some(id) = open_request { finish_open_request(&root, &id).await?; }
        else { refresh_on_open_at(&root).await?; }
        println!("Signed update check finished. Use lifecycle status for local details.");
        return Ok(());
    }
    let _owner = launch::private_directory(&root, false)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let _scheduled = loop {
        if let Some(lock) = launch::try_lock(&root, "schedule.lock")? {
            break lock;
        }
        if tokio::time::Instant::now() > deadline {
            return Err("The requested operation is still starting; retry shortly.".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let _lock = launch::try_lock(&root, "operation.lock")?
        .ok_or("A companion operation is already running. Wait, then retry.")?;
    let result = match operation {
        "check" => check(&root).await.map(|value| println!("{value}")),
        "update" => update(&root, local.as_deref(), open).await,
        "rollback" => {
            let cfg = read_state(&root)?;
            let previous=cfg.previous.ok_or("No earlier CLI version is installed. Reinstall the verified companion; your library is kept.")?;
            let signed = checked_release(&root, previous, &verifier(&self_executable()?)?)?;
            switch(&root, cfg, &signed, true, open).await
        }
        "uninstall" => {
            let cfg = read_state(&root)?;
            let outcome = uninstall::run(
                &cfg.launch_state,
                &executable(&root, cfg.current),
                uninstall::Mode::Interactive,
            )
            .await?;
            println!(
                "{}",
                serde_json::to_string(&outcome).map_err(|_| "Cannot report uninstall")?
            );
            Ok(())
        }
        "enable-autostart"
        | "disable-autostart"
        | "enable-update-checks"
        | "disable-update-checks" => {
            let mut cfg = read_state(&root)?;
            let enabled = operation.starts_with("enable-");
            if operation.ends_with("autostart") {
                cfg.autostart = enabled;
                write_agent(&root, &cfg)?;
            } else {
                cfg.automatic_updates = enabled
            }
            save(&root, STATE, &cfg)?;
            note(&root, "ready", "Companion preferences saved.");
            Ok(())
        }
        "autostart" | "automatic-updates" => {
            let mut cfg = read_state(&root)?;
            let enabled = enabled.ok_or("Choose --enabled true or false")?;
            if operation == "autostart" {
                cfg.autostart = enabled;
                write_agent(&root, &cfg)?;
            } else {
                cfg.automatic_updates = enabled
            }
            save(&root, STATE, &cfg)?;
            note(&root, "ready", "Companion preferences saved.");
            Ok(())
        }
        _ => Err("Unsupported lifecycle action".into()),
    };
    if let Err(error) = &result {
        failure(&root, operation, &format!("{error} Your library is kept. Retry, or run lifecycle recover."));
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn private_fixture(name: &str) -> PathBuf {
        let mut nonce = [0u8; 8]; getrandom::fill(&mut nonce).unwrap();
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("pf-{name}-{}", &release::digest(&nonce)[..12]));
        launch::private_directory(&root, true).unwrap();
        root
    }
    #[test]
    fn reinstall_follows_the_completed_uninstall_library_without_restoring_launch_authority() {
        let root=private_fixture("reinstall-program");let data=private_fixture("kept-library");let state=private_fixture("kept-launch");let fallback=root.join("unused-empty-default");
        let receipt=json!({"version":1,"status":"complete","registration":{"version":1,"dataDir":data,"runtime":"/removed/program/runtime","executable":"/removed/program/photofindry"},"manifestSha256":"a".repeat(64),"trashedPath":"/removed/Trash/PhotoFindry"});
        launch::write_uninstall_receipt(&state,&receipt).unwrap();
        assert_eq!(current_library_at(&root,&state,&fallback).unwrap(),data);
        assert!(launch::read_registration(&state).is_err());assert!(!state.join("registration.json").exists());assert!(!root.join(STATE).exists());assert!(!fallback.exists());
        // A later installation/current registration remains authoritative over
        // an older completed receipt; the retained executable is never run.
        let other=private_fixture("later-library");let cfg=Installation{schema:1,current:121,previous:None,highest:121,data:other.clone(),launch_state:state.clone(),autostart:false,automatic_updates:false,retire:None};
        save(&root,STATE,&cfg).unwrap();assert_eq!(current_library_at(&root,&state,&fallback).unwrap(),other);
        fs::remove_file(root.join(STATE)).unwrap();
        let record=launch::Registration{version:1,data_dir:other.clone(),runtime:PathBuf::from("/removed/other/runtime"),executable:PathBuf::from("/removed/other/photofindry")};
        atomic(&state,"registration.json",&serde_json::to_vec(&record).unwrap()).unwrap();assert_eq!(current_library_at(&root,&state,&fallback).unwrap(),other);
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(data).unwrap();fs::remove_dir_all(state).unwrap();fs::remove_dir_all(other).unwrap();
    }
    #[test]
    fn unavailable_unsafe_malformed_or_pending_kept_library_never_falls_back_to_empty() {
        let root=private_fixture("unavailable-program");let data=private_fixture("unavailable-library");let state=private_fixture("unavailable-launch");let fallback=root.join("must-not-create");
        let receipt=|path:&Path,status:&str|json!({"version":1,"status":status,"registration":{"version":1,"dataDir":path,"runtime":"/removed/runtime","executable":"/removed/photofindry"},"manifestSha256":"a".repeat(64),"trashedPath":"/removed/Trash/PhotoFindry"});
        launch::write_uninstall_receipt(&state,&receipt(&root.join("missing-custom-library"),"complete")).unwrap();
        assert!(current_library_at(&root,&state,&fallback).unwrap_err().contains("kept library folder is unavailable"));
        let linked=root.join("linked-library");std::os::unix::fs::symlink(&data,&linked).unwrap();launch::write_uninstall_receipt(&state,&receipt(&linked,"complete")).unwrap();assert!(current_library_at(&root,&state,&fallback).is_err());
        launch::write_uninstall_receipt(&state,&json!({"version":1,"status":"complete","registration":{"dataDir":data}})).unwrap();assert!(current_library_at(&root,&state,&fallback).is_err());
        launch::write_uninstall_receipt(&state,&receipt(&data,"revoked")).unwrap();assert!(current_library_at(&root,&state,&fallback).unwrap_err().contains("uninstall is pending"));
        assert!(!fallback.exists());assert!(!root.join(STATE).exists());assert!(!state.join("registration.json").exists());
        fs::remove_file(state.join("uninstall.json")).unwrap();assert_eq!(current_library_at(&root,&state,&fallback).unwrap(),fallback);assert!(!fallback.exists());
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(data).unwrap();fs::remove_dir_all(state).unwrap();
    }
    #[test]
    fn authenticated_same_build_repair_quarantines_damaged_program_and_keeps_library_and_state() {
        let root=private_fixture("program-repair");let data=private_fixture("repair-library");let state=private_fixture("repair-launch");
        launch::private_directory(&root.join("versions"),true).unwrap();
        let cfg=Installation{schema:1,current:120,previous:Some(119),highest:122,data:data.clone(),launch_state:state.clone(),autostart:true,automatic_updates:false,retire:None};
        save(&root,STATE,&cfg).unwrap();fs::write(data.join("original-do-not-change"),b"saved original bytes").unwrap();
        let make_stage=|name:&str| {
            let stage=root.join("versions").join(name);let package=stage.join("package");launch::private_directory(&package,true).unwrap();
            let files=["bin/photofindry","bin/CompanionReleaseVerify","bin/CompanionLauncher","bin/CompanionLauncher.app/Contents/Info.plist","runtime/PhotoWorker","runtime/PhotoEngine","LICENSE","README.txt"];
            let mut inventory=String::new();
            for relative in files {let path=package.join(relative);launch::private_directory(path.parent().unwrap(),true).unwrap();fs::write(&path,b"authenticated fixture bytes").unwrap();inventory.push_str(&format!("{}  {relative}\n",release::digest(b"authenticated fixture bytes")));}
            fs::write(package.join("SHA256SUMS.txt"),&inventory).unwrap();
            let signed=release::Release{schema:1,kind:"photofindry-cli-release".into(),version:"0.2.0-beta.22".into(),build:120,platform:"macos-arm64".into(),data_contract:"photofindry-library-compatible-v1".into(),inventory_sha256:release::digest(inventory.as_bytes()),archive_sha256:"a".repeat(64),archive_bytes:1,archive_url:"https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.22/PhotoFindry-0.2.0-beta.22-arm64-companion.zip".into()};
            (Stage::new(stage),signed)
        };
        let (first,signed)=make_stage(".stage-original");commit_version(&root,&first,&signed,b"same already-authenticated envelope").unwrap();
        fs::write(package(&root,120).join("bin/photofindry"),b"corrupted").unwrap();fs::write(package(&root,120).join("unexpected-file-kept"),b"keep damaged contents").unwrap();
        let (replacement,signed)=make_stage(".stage-replacement");commit_version(&root,&replacement,&signed,b"same already-authenticated envelope").unwrap();
        release::verify_package(&package(&root,120),&signed).unwrap();
        let quarantined=fs::read_dir(root.join("versions")).unwrap().map(|item|item.unwrap().path()).find(|path|path.file_name().unwrap().to_string_lossy().starts_with(".damaged-")).unwrap();
        assert_eq!(fs::read(quarantined.join("package/unexpected-file-kept")).unwrap(),b"keep damaged contents");
        assert_eq!(read_state(&root).unwrap(),cfg);assert_eq!(fs::read(data.join("original-do-not-change")).unwrap(),b"saved original bytes");
        fs::remove_file(root.join("versions/120/release.json")).unwrap();
        let (replacement,signed)=make_stage(".stage-missing-metadata");commit_version(&root,&replacement,&signed,b"same already-authenticated envelope").unwrap();
        release::verify_package(&package(&root,120),&signed).unwrap();assert_eq!(read_state(&root).unwrap(),cfg);
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(data).unwrap();fs::remove_dir_all(state).unwrap();
    }
    #[test]
    fn installation_entry_identity_is_stable_and_differs_between_installations() {
        let first=private_fixture("entry-a");let second=private_fixture("entry-b");
        let identity=entry_identity(&first).unwrap();
        assert_eq!(identity,entry_identity(&first).unwrap());
        assert_ne!(identity,entry_identity(&second).unwrap());
        let info=String::from_utf8(entry_info(&identity,120)).unwrap();
        assert!(info.contains(&format!("com.photofindry.companion.entry.{identity}")));
        for scheme in ["photofindry-companion","photofindry-local","photofindry"] {assert!(info.contains(&format!("<string>{scheme}</string>")));}
        assert!(!info.contains(".b120"));
        save(&first,"entry-identity.json",&json!({"schema":1,"id":"another.application"})).unwrap();
        let repaired=entry_identity(&first).unwrap();assert_ne!(repaired,identity);assert_eq!(repaired,entry_identity(&first).unwrap());
        assert!(fs::read_dir(&first).unwrap().any(|item|item.unwrap().file_name().to_string_lossy().starts_with(".damaged-entry-identity-")));
        fs::remove_dir_all(first).unwrap();fs::remove_dir_all(second).unwrap();
    }
    #[test]
    fn failure_is_readable_after_a_failed_first_install_and_survives_recovery_status() {
        let root = private_fixture("failure");
        failure(&root, "install", "Background catalog access was denied.\nSaved work stays in place.");
        note(&root, "recovered", "Prior software restored.");
        let value = status(&root).unwrap();
        assert_eq!(value["installed"], false);
        assert_eq!(value["status"]["phase"], "recovered");
        assert_eq!(value["lastFailure"]["operation"], "install");
        assert!(value["lastFailure"]["message"].as_str().unwrap().contains("access was denied"));
        assert!(!value["lastFailure"]["message"].as_str().unwrap().contains('\n'));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restart_budget_never_restarts_first_start_failure_or_maintenance_and_bounds_crashes() {
        let mut budget = RestartBudget::default();
        assert_eq!(budget.next(false, false, Duration::from_secs(4)), None);
        assert_eq!(budget.next(true, true, Duration::from_secs(4)), None);
        assert_eq!(budget.next(true, false, Duration::from_secs(4)), Some(Duration::from_secs(1)));
        assert_eq!(budget.next(true, false, Duration::from_secs(4)), Some(Duration::from_secs(2)));
        assert_eq!(budget.next(true, false, Duration::from_secs(4)), None);
        assert_eq!(budget.next(true, false, Duration::from_secs(301)), Some(Duration::from_secs(1)));
    }
    #[test]
    fn only_the_matching_pending_open_receipt_can_be_completed() {
        let root = private_fixture("receipt"); let id = "a".repeat(64);
        save(&root, OPEN_REQUEST, &OpenRequest { schema: 1, id: id.clone(), complete: false, error: None, pid: None, updated_at: now() }).unwrap();
        assert!(!read_request(&root, OPEN_REQUEST, &id).unwrap().complete);
        assert!(read_request(&root, OPEN_REQUEST, &"b".repeat(64)).is_err());
        assert!(!request_id("../../other"));
        let cfg = Installation { schema: 1, current: 120, previous: Some(119), highest: 120, data: root.join("external-library"), launch_state: root.join("external-state"), autostart: true, automatic_updates: true, retire: None };
        let text = String::from_utf8(plist_with_request(&root, &cfg, Some("open-update"), Some(&id)).unwrap()).unwrap();
        assert!(text.contains("<string>open-update</string>"));
        assert!(text.contains("<string>--open-request</string>"));
        assert!(text.contains(&id));
        assert!(!text.contains("/bin/sh"));
        assert!(plist_with_request(&root, &cfg, Some("uninstall"), Some(&id)).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn access_probe_records_startup_before_data_checks_and_names_the_correct_timeout() {
        let root=private_fixture("probe-startup");let data=private_fixture("probe-startup-data");let id="a".repeat(64);
        let pending=OpenRequest{schema:1,id:id.clone(),complete:false,error:None,pid:None,updated_at:now()};
        assert!(access_timeout(&pending).contains("did not report startup"));assert!(!access_timeout(&pending).contains("library folder"));
        save(&root,ACCESS_RECEIPT,&pending).unwrap();finish_access_check(&root,&data,&id).unwrap();
        let completed=read_request(&root,ACCESS_RECEIPT,&id).unwrap();assert_eq!(completed.pid,Some(std::process::id()));assert!(completed.complete);
        assert!(access_timeout(&completed).contains("started but could not finish checking the library folder"));
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(data).unwrap();
    }
    #[test]
    fn background_access_is_read_only_and_rejects_missing_and_linked_catalogs() {
        let root = private_fixture("access");
        check_library_access(&root).unwrap();
        check_library_access(&root.join("not-created-yet/new-library")).unwrap();
        assert!(!root.join("not-created-yet").exists());
        for name in ["catalog.json", "catalog.sqlite", "jobs.sqlite", "people.sqlite", "source-host.sqlite"] { fs::write(root.join(name), b"readable saved fixture; not opened as a database").unwrap(); }
        check_library_access(&root).unwrap();
        let before = fs::read(root.join("catalog.sqlite")).unwrap();
        let names: Vec<_> = fs::read_dir(&root).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        check_library_access(&root).unwrap();
        assert_eq!(fs::read(root.join("catalog.sqlite")).unwrap(), before);
        assert_eq!(fs::read_dir(&root).unwrap().count(), names.len());
        fs::remove_file(root.join("jobs.sqlite")).unwrap();
        std::os::unix::fs::symlink(root.join("catalog.sqlite"), root.join("jobs.sqlite")).unwrap();
        assert!(check_library_access(&root).unwrap_err().contains("cannot read"));
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    #[ignore = "real launchd access test; set PHOTOFINDRY_ACCESS_CHECK_CLI to an already built candidate"]
    async fn launchd_access_preflight_reports_readable_and_inaccessible_fixture_before_service_changes() {
        let exe = PathBuf::from(std::env::var_os("PHOTOFINDRY_ACCESS_CHECK_CLI").expect("built candidate CLI path"));
        let root = private_fixture("agent-access"); let data = private_fixture("agent-library");
        for name in ["catalog.json", "catalog.sqlite", "jobs.sqlite", "people.sqlite", "source-host.sqlite"] { fs::write(data.join(name), b"readable saved fixture; not opened as a database").unwrap(); }
        preflight_background_access(&root, &data, &exe).await.unwrap();
        let plist=String::from_utf8(release::read(&root.join("access-check.plist"),16384).unwrap()).unwrap();
        assert!(plist.contains(&xml(&root.join("access-controller/photofindry").to_string_lossy())));
        assert_eq!(release::file_hash(&root.join("access-controller/photofindry"),64*1024*1024).unwrap(),release::file_hash(&exe,64*1024*1024).unwrap());
        fs::remove_file(data.join("catalog.sqlite")).unwrap();
        std::os::unix::fs::symlink(data.join("catalog.json"), data.join("catalog.sqlite")).unwrap();
        let error = preflight_background_access(&root, &data, &exe).await.unwrap_err();
        assert!(error.contains("cannot read the library"));
        assert_eq!(status(&root).unwrap()["lastFailure"]["operation"], "background-access");
        assert!(!root.join(STATE).exists()); assert!(!root.join(JOURNAL).exists());
        fs::remove_dir_all(root).unwrap(); fs::remove_dir_all(data).unwrap();
    }
    #[tokio::test]
    #[ignore = "real launchd maintenance-ownership fixture; uses isolated program and no library API"]
    async fn closing_the_open_requester_does_not_kill_its_maintenance_job() {
        let root = private_fixture("agent-open"); let data = private_fixture("agent-data"); let state = private_fixture("agent-state");
        let cfg = Installation { schema: 1, current: 120, previous: Some(119), highest: 120, data: data.clone(), launch_state: state.clone(), autostart: true, automatic_updates: false, retire: None };
        let exe = executable(&root, cfg.current); fs::create_dir_all(exe.parent().unwrap()).unwrap();
        let script = r#"#!/usr/bin/python3
import json,os,pathlib,sys,time
root=pathlib.Path(sys.argv[sys.argv.index('--install-root')+1])
ident=sys.argv[sys.argv.index('--open-request')+1]
request=json.loads((root/'open-update-request.json').read_text())
assert request['id']==ident
(root/'maintenance-started').write_text('owned by launchd')
time.sleep(1.2)
request.update(complete=True,updatedAt=int(time.time()))
target=root/'receipt-tmp'
fd=os.open(target,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
with os.fdopen(fd,'w') as stream:
 json.dump(request,stream);stream.flush();os.fsync(stream.fileno())
os.replace(target,root/'open-update-request.json')
"#;
        fs::write(&exe, script).unwrap(); fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap(); save(&root, STATE, &cfg).unwrap();
        let opening = tokio::spawn({let root=root.clone();async move {independent_refresh_on_open(&root).await}});
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while !root.join("maintenance-started").exists() { assert!(tokio::time::Instant::now() < deadline, "maintenance did not start"); tokio::time::sleep(Duration::from_millis(50)).await; }
        opening.abort(); let _ = opening.await;
        let id: String = serde_json::from_slice::<Value>(&release::read(&root.join(OPEN_REQUEST), 16384).unwrap()).unwrap()["id"].as_str().unwrap().into();
        loop { if read_request(&root, OPEN_REQUEST, &id).unwrap().complete {break;} assert!(tokio::time::Instant::now() < deadline,"maintenance died with requester"); tokio::time::sleep(Duration::from_millis(50)).await; }
        assert_eq!(read_state(&root).unwrap(), cfg); assert!(!root.join(JOURNAL).exists());
        unload(format!("{}/{}.maintenance",domain(),label(&root).unwrap())).await.unwrap();
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(data).unwrap();fs::remove_dir_all(state).unwrap();
    }
    #[tokio::test]
    async fn a_second_open_waits_for_the_owned_update_instead_of_failing_during_its_switch() {
        let root=std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("pf-open-maintenance-{}-{}",std::process::id(),now()));
        launch::private_directory(&root,true).unwrap();
        let held=launch::try_lock(&root,"operation.lock").unwrap().unwrap();
        let waiting=tokio::spawn({let root=root.clone();async move{wait_for_open_maintenance_at(&root).await}});
        tokio::time::sleep(Duration::from_millis(220)).await;
        assert!(!waiting.is_finished());
        drop(held);
        tokio::time::timeout(Duration::from_secs(3),waiting).await.unwrap().unwrap().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn readiness_reports_a_failed_background_launch_without_waiting_for_the_full_deadline() {
        let base=std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("pf-health-{}-{}",std::process::id(),now()));
        std::fs::create_dir(&base).unwrap();std::fs::set_permissions(&base,std::fs::Permissions::from_mode(0o700)).unwrap();
        let root=base.join("Program");let data=base.join("Library");let state=base.join("Launch");
        for path in [&root,&data,&state] {std::fs::create_dir(path).unwrap();std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o700)).unwrap();}
        let cfg=Installation{schema:1,current:117,previous:Some(116),highest:117,data,launch_state:state,autostart:true,automatic_updates:false,retire:None};
        note(&root,"opening","Opening the saved library.");
        note(&root,"error","The local worker could not open its saved library.");
        let error=health(&root,&cfg).await.unwrap_err();
        assert!(error.contains("The local worker could not open"));
        std::fs::remove_dir_all(base).unwrap();
    }
    #[tokio::test]
    async fn current_open_remains_read_only_while_updater_finishes() {
        let base=std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("pf-current-open-{}-{}",std::process::id(),now()));
        std::fs::create_dir(&base).unwrap();std::fs::set_permissions(&base,std::fs::Permissions::from_mode(0o700)).unwrap();
        let root=base.join("Program");let data=base.join("Library");let state=base.join("Launch");
        for p in [&root,&data,&state] {std::fs::create_dir(p).unwrap();std::fs::set_permissions(p,std::fs::Permissions::from_mode(0o700)).unwrap();}
        let cfg=Installation{schema:1,current:114,previous:Some(113),highest:114,data:data.clone(),launch_state:state.clone(),autostart:true,automatic_updates:true,retire:None};
        let runtime=package(&root,114).join("runtime");std::fs::create_dir_all(&runtime).unwrap();let exe=executable(&root,114);std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        for p in [&exe,&runtime.join("PhotoWorker"),&runtime.join("PhotoEngine")] {std::fs::write(p,b"not executed").unwrap();std::fs::set_permissions(p,std::fs::Permissions::from_mode(0o700)).unwrap();}
        save(&root,STATE,&cfg).unwrap();let record=launch::Registration{version:1,data_dir:data,runtime,executable:exe};
        atomic(&state,"registration.json",&serde_json::to_vec(&record).unwrap()).unwrap();
        let before=std::fs::read(state.join("registration.json")).unwrap();
        let held=launch::try_lock(&root,"operation.lock").unwrap().unwrap();
        assert_eq!(current_library_at(&root,&state,&base.join("UnusedDefault")).unwrap(),cfg.data);
        prefer_installed_at(&root,&state).await.unwrap();
        require_current_registration_at(&root,&state,&record).unwrap();
        let mut stale=record.clone();stale.executable=PathBuf::from("/Applications/PhotoFindry-previous.app/old");
        assert!(require_current_registration_at(&root,&state,&stale).unwrap_err().contains("registration changed"));
        std::fs::remove_file(root.join(STATE)).unwrap();
        assert_eq!(current_library_at(&root,&state,&base.join("UnusedDefault")).unwrap(),record.data_dir);
        assert!(require_installed_at(&root,&state).unwrap_err().contains("installer"));
        assert!(require_current_registration_at(&root,&state,&record).unwrap_err().contains("installer"));
        assert_eq!(std::fs::read(state.join("registration.json")).unwrap(),before);
        drop(held);
        // A removed native/browser package must not hide a still-present custom library.
        std::fs::remove_file(&record.executable).unwrap();
        std::fs::remove_dir_all(&record.runtime).unwrap();
        assert!(launch::read_registration(&state).is_err());
        assert_eq!(current_library_at(&root,&state,&base.join("UnusedDefault")).unwrap(),record.data_dir);
        let other=base.join("OtherLibrary");std::fs::create_dir(&other).unwrap();
        let mut foreign=record.clone();foreign.data_dir=other;
        atomic(&state,"registration.json",&serde_json::to_vec(&foreign).unwrap()).unwrap();
        assert!(repair_registration(&root,&cfg).unwrap_err().contains("Another library"));
        assert_eq!(launch::recovery_registration(&state).unwrap(),Some(foreign));
        atomic(&state,"registration.json",&serde_json::to_vec(&record).unwrap()).unwrap();
        let tx=Transaction{schema:1,before:None,after:cfg.clone(),registration:Some(record.clone()),committed:false};
        save(&root,JOURNAL,&tx).unwrap();
        std::fs::remove_file(state.join("registration.json")).unwrap();
        restore(&root,&tx,Path::new("/unused-verifier"),false,None).await.unwrap();
        assert_eq!(current_library_at(&root,&state,&base.join("UnusedDefault")).unwrap(),record.data_dir);
        assert!(!root.join(JOURNAL).exists());
        assert!(launch::read_registration(&state).is_err());
        let mut missing=record.clone();missing.data_dir=base.join("unavailable-library");
        atomic(&state,"registration.json",&serde_json::to_vec(&missing).unwrap()).unwrap();
        assert!(current_library_at(&root,&state,&base.join("UnusedDefault")).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn same_build_cannot_silently_ignore_different_signed_installer_bytes() {
        assert!(existing_release_identity(113,113,b"same signed envelope",b"same signed envelope").is_ok());
        assert!(existing_release_identity(113,113,b"installed",b"different").unwrap_err().contains("newer verified"));
        assert!(existing_release_identity(114,113,b"current",b"older installer").is_ok());
    }
    #[test]
    fn installed_launch_repairs_only_its_own_library_and_exact_current_runtime() {
        let root=Path::new("/private/program");
        let cfg=Installation{schema:1,current:114,previous:Some(113),highest:114,data:PathBuf::from("/private/library"),launch_state:PathBuf::from("/private/state"),autostart:true,automatic_updates:true,retire:None};
        let mut record=launch::Registration{version:1,data_dir:cfg.data.clone(),runtime:package(root,114).join("runtime"),executable:executable(root,114)};
        assert_eq!(registration_needs_repair(root,&cfg,&record).unwrap(),false);
        record.executable=PathBuf::from("/Applications/PhotoFindry-previous.app/Contents/Resources/companion/bin/photofindry");
        assert_eq!(registration_needs_repair(root,&cfg,&record).unwrap(),true);
        record.executable=executable(root,114);record.runtime=package(root,113).join("runtime");
        assert_eq!(registration_needs_repair(root,&cfg,&record).unwrap(),true);
        record.data_dir=PathBuf::from("/private/another-library");
        assert!(registration_needs_repair(root,&cfg,&record).unwrap_err().contains("Another library"));
    }
    #[test]
    fn installed_paths_are_structural() {
        assert_eq!(
            installed_root(Path::new(
                "/private/test/versions/113/package/bin/photofindry"
            )),
            Some(PathBuf::from("/private/test"))
        );
        for p in [
            "/Applications/PhotoFindry.app/Contents/Resources/companion/bin/photofindry",
            "/private/test/versions/no/package/bin/photofindry",
            "/private/test/current/bin/photofindry",
        ] {
            assert!(installed_root(Path::new(p)).is_none());
        }
    }
    #[test]
    fn autostart_plist_cannot_execute_path_text() {
        let root = Path::new("/private/test/a&b");
        let cfg = Installation {
            schema: 1,
            current: 113,
            previous: None,
            highest: 113,
            data: PathBuf::from("/private/library"),
            launch_state: PathBuf::from("/private/launch"),
            autostart: true,
            automatic_updates: true,
            retire: None,
        };
        let text = String::from_utf8(plist(root, &cfg, None).unwrap()).unwrap();
        assert!(text.contains("a&amp;b"));
        assert!(!text.contains("/bin/sh"));
        assert!(!text.contains("StandardOutPath"));
        assert!(text.contains("<key>RunAtLoad</key><true />"));
    }
}
