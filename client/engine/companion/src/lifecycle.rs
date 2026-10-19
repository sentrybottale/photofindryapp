//! Per-user CLI lifecycle. Program versions are separate from the photo library.
//! No website supplies a path, trust key, executable or installation decision.
use super::{cli_release as release, launch, uninstall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
type Result<T> = std::result::Result<T, String>;
const STATE: &str = "installation.json";
const JOURNAL: &str = "transaction.json";
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
    let mut args = vec![
        executable(root, cfg.current).to_string_lossy().into_owned(),
        "lifecycle".into(),
        operation.unwrap_or("background").into(),
        "--install-root".into(),
        root.to_string_lossy().into_owned(),
    ];
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
    Ok(format!("<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array>{args}</array><key>RunAtLoad</key><{} /><key>ProcessType</key><string>Background</string><key>ThrottleInterval</key><integer>30</integer><key>KeepAlive</key><{} /></dict></plist>",xml(&label),if operation.is_some()||cfg.autostart{"true"}else{"false"},"false").into_bytes())
}
fn write_agent(root: &Path, cfg: &Installation) -> Result<PathBuf> {
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
async fn health(cfg: &Installation) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(75);
    loop {
        if launch::request_health(&cfg.launch_state)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            return Err("The new companion did not become ready. The prior software will be restored; your library stays in place.".into());
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
    if let Ok(record)=launch::read_registration(&cfg.launch_state) {
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
    if root.join(STATE).exists() {
        let cfg=read_state(root)?;
        if cfg.launch_state!=state {return Err("The installed companion uses another launch registration. Open its workspace before changing setup.".into())}
        return Ok(cfg.data);
    }
    if let Some(record)=launch::recovery_registration(state)? {return Ok(record.data_dir);}
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
        if !registration_needs_repair(root,&cfg,&record)? {return Ok(())}
    }
    // --open-after runs while its updater still owns operation.lock. Read-only
    // current registration above must remain usable; only repair takes the lock.
    let _operation=launch::try_lock(root,"operation.lock")?.ok_or("Companion maintenance is still finishing. Wait a moment, then open PhotoFindry again.")?;
    let cfg=read_state(root)?;
    if cfg.launch_state!=directory {return Err("The installed companion's launch registration changed. Reopen its verified installer to review setup.".into())}
    checked_release(root,cfg.current,&verifier(&self_executable()?)?)?;
    repair_registration(root,&cfg)
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
        checked_release(root, before.current, helper)?;
        let mut before = before.clone();
        before.highest = before.highest.max(tx.after.highest);
        let before = &before;
        publish_registration(root, before, guard)?;
        save(root, STATE, before)?;
        write_agent(root, before)?;
        if restart {
            start_agent(root, before, true).await?;
            health(before).await?;
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
    if let Ok(record)=launch::read_registration(&before.launch_state) {registration_needs_repair(root,&before,&record)?;}
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
        start_agent(root, &after, true).await?;
        health(&after).await?;
        tx.committed = true;
        save(root, JOURNAL, &tx)?;
        clear(root, JOURNAL)?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(reason) = outcome {
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
async fn register_links(root: &Path, cfg: &Installation) -> Result<()> {
    if root != default_root()? || cfg.launch_state != launch::default_directory()? {
        return Ok(());
    }
    let mut child = launch::desktop_command(&launch::browser_opener(&executable(root, cfg.current))?)
        .arg("--register-links")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "The companion is installed, but its Open link could not be repaired. Reopen the verified PhotoFindry installer and choose Try again; saved work is kept.")?;
    let status=match tokio::time::timeout(Duration::from_secs(105),child.wait()).await {
        Ok(Ok(status))=>status,
        _=>{let _=child.kill().await;let _=child.wait().await;return Err("macOS link repair did not finish. Reopen the verified PhotoFindry installer and choose Try again; your library stays in place.".into())}
    };
    if !status.success() {
        return Err("The companion is running but macOS did not select its Open link. Reopen the verified PhotoFindry installer and choose Try again to repair it; saved work is kept.".into());
    }
    Ok(())
}
async fn download(url: &str, path: &Path, maximum: u64) -> Result<()> {
    if !(url == release::FEED
        || url.starts_with("https://github.com/sentrybottale/photofindryapp/releases/download/v"))
    {
        return Err("Unsupported update destination".into());
    }
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
            "15",
            "--max-time",
            "600",
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
    match tokio::time::timeout(Duration::from_secs(620), transfer).await {
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
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn commit_version(
    root: &Path,
    stage: &Path,
    release: &release::Release,
    envelope: &[u8],
) -> Result<()> {
    release::verify_package(&stage.join("package"), release)?;
    atomic(stage, "release.json", envelope)?;
    let target = root.join("versions").join(release.build.to_string());
    if target.exists() {
        let old = release::read(&target.join("release.json"), 131072)?;
        if old != envelope {
            return Err("This build already has a different signed identity. Keep the current version and contact support.".into());
        }
        release::verify_package(&target.join("package"), release)?;
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
    if next.build <= cfg.current || next.build < cfg.highest {
        return Err("No newer signed release is available. Current software is unchanged.".into());
    }
    let stage = Stage(stage_version(root)?);
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
    commit_version(root, &stage.0, &next, &bytes)?;
    switch(root, cfg, &next, false, open).await
}
pub fn status(root: &Path) -> Result<Value> {
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
    Ok(
        json!({"installed":true,"version":release.version,"build":cfg.current,"previousBuild":cfg.previous,"autostart":cfg.autostart,"automaticUpdates":cfg.automatic_updates,"status":detail,"available":available.as_ref().map(|v|&v["release"]),"recoveryPending":root.join(JOURNAL).exists(),"appCleanupPending":cfg.retire.is_some()}),
    )
}
pub async fn start_registered(root: &Path) -> Result<()> {
    recover(root, false).await?;
    let cfg = read_state(root)?;
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
async fn background(root: &Path) -> Result<()> {
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
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot start the background companion")?;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "Cannot handle background shutdown")?;
    let mut ticks = tokio::time::interval(Duration::from_secs(30));
    let mut last_check = now().saturating_sub(21540);
    let mut last_retire = now().saturating_sub(570);
    let mut check_child: Option<tokio::process::Child> = None;
    loop {
        tokio::select! {
         status=child.wait()=>{let status=status.map_err(|_|"Cannot inspect companion exit")?;if status.success(){return Ok(())}note(root,"error","The companion stopped unexpectedly. Open PhotoFindry to retry, or use lifecycle recover. Your saved work is kept.");return Err("The companion stopped unexpectedly; saved work is kept.".into())},
         _=term.recv()=>{let _=launch::request_stop(&cfg.launch_state).await;if tokio::time::timeout(Duration::from_secs(30),child.wait()).await.is_err(){let _=child.kill().await;let _=child.wait().await;}return Ok(())},
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
    if root.join(STATE).exists() {
        let cfg = read_state(root)?;
        if cfg.data != data || cfg.launch_state != state {
            return Err("Another library is registered for this installation. Keep it in place and open its workspace.".into());
        }
        if signed.build > cfg.current && signed.build >= cfg.highest {
            let stage = Stage(stage_version(root)?);
            release::copy_package(source, &stage.0.join("package"), &signed)?;
            commit_version(root, &stage.0, &signed, &bytes)?;
            switch(root, cfg, &signed, false, false).await?;
        } else {
            checked_release(root, cfg.current, &helper)?;
            if signed.build==cfg.current {
                let installed=release::read(&root.join("versions").join(cfg.current.to_string()).join("release.json"),131072)?;
                existing_release_identity(cfg.current,signed.build,&installed,&bytes)?;
            }
            repair_registration(root, &cfg)?;
            start_agent(root, &cfg, false).await?;
            health(&cfg).await?;
            register_links(root, &cfg).await?;
        }
        return status(root);
    }
    if let Some(prior)=launch::recovery_registration(state)? {
        if prior.data_dir!=data {return Err("Another library owns the current launch registration. Open the latest verified installer to keep using that library; no program or library was changed.".into())}
    }
    let stage = Stage(stage_version(root)?);
    release::copy_package(source, &stage.0.join("package"), &signed)?;
    commit_version(root, &stage.0, &signed, &bytes)?;
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
     _=cancelled(&mut cancellation)=>{if let Some(pid)=child.id(){unsafe{libc::kill(pid as i32,libc::SIGINT);}}if tokio::time::timeout(Duration::from_secs(30),child.wait()).await.is_err(){let _=child.kill().await;let _=child.wait().await;}let _=progress.await;let _=restore(root,&tx,&helper,true,None).await;return Err("Migration cancelled. The previous installation and current library are kept; retry when ready.".into())}
    };
    let output = progress
        .await
        .map_err(|_| "Migration progress reader stopped")?;
    if !migration_status.success() || output.is_err() {
        let _ = restore(root, &tx, &helper, true, None).await;
        return Err(output.err().unwrap_or_else(||"The library could not migrate. Keep the existing app and recovery copy, then retry. Your original library was not replaced.".into()));
    }
    let result = tokio::select! { result=async {
           save(root, STATE, &cfg)?;
           start_agent(root, &cfg, true).await?;
           health(&cfg).await?;
           register_links(root, &cfg).await?;
           tx.committed = true;
           save(root, JOURNAL, &tx)?;
           clear(root, JOURNAL)?;
           Ok::<(), String>(())
       }
    => result, _=cancelled(&mut cancellation)=>Err("Installation cancelled. Your library is kept; retry when ready.".into()) };
    if let Err(error) = result {
        let _ = restore(root, &tx, &helper, true, None).await;
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
        "Choose lifecycle install, status, check, update, rollback, recover or background",
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
    let mut it = args.iter().skip(2);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--open-after" => open = true,
            "--current-library" if !use_current_library => use_current_library = true,
            "--no-autostart" => autostart = false,
            "--no-update-checks" => automatic_updates = false,
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
    if (operation != "install"
        && (use_current_library || data.is_some()
            || rollback.is_some()
            || state.is_some()
            || envelope.is_some()
            || !autostart
            || !automatic_updates))
        || (local.is_some() && operation != "update")
        || (open && !matches!(operation, "update" | "rollback"))
        || (enabled.is_some() && !matches!(operation, "autostart" | "automatic-updates"))
    {
        return Err("These options do not belong to this lifecycle action".into());
    }
    let root = match root {
        Some(root) => root,
        None => installed_root(&self_executable()?)
            .map(Ok)
            .unwrap_or_else(default_root)?,
    };
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
        .await?;
        println!(
            "{}",
            json!({"ok":true,"state":"ready","installation":result})
        );
        return Ok(());
    }
    if operation == "background" {
        return background(&root).await;
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
        checked_release(&root, cfg.current, &verifier(&self_executable()?)?)?;
        repair_registration(&root,&cfg)?;
        start_agent(&root, &cfg, false).await?;
        health(&cfg).await?;
        register_links(&root, &cfg).await?;
        note(
            &root,
            "ready",
            "The background companion is ready. Your current library was kept in place.",
        );
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
        note(
            &root,
            "error",
            &format!("{error} Your library is kept. Retry, or run lifecycle recover."),
        );
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
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
