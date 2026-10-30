//! Signed per-user install, update, rollback, recovery and uninstall for the
//! Linux and Windows CLI companion. Same contract as the macOS lifecycle:
//! program versions live apart from the library, only publisher-signed packages
//! for this platform are admitted, and `highest` refuses replayed older builds.
//! Updates replace program files only; the library is never restored or moved.
//! Linux runs the companion as a `systemd --user` unit; Windows as a per-user
//! Scheduled Task at logon. Neither needs administrator rights.
use super::cli_release as release;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
type Result<T> = std::result::Result<T, String>;
const STATE: &str = "installation.json";
const JOURNAL: &str = "transaction.json";
const EXE: &str = if cfg!(windows) { "photofindry.exe" } else { "photofindry" };
const VERIFIER: &str = if cfg!(windows) { "CompanionReleaseVerify.exe" } else { "CompanionReleaseVerify" };
const CURL: &str = if cfg!(windows) { "C:\\Windows\\System32\\curl.exe" } else { "/usr/bin/curl" };
const READY_TIMEOUT: Duration = Duration::from_secs(180);

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
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Transaction {
    schema: u32,
    before: Option<Installation>,
    after: Installation,
    committed: bool,
}

fn home_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(variable).map(PathBuf::from).filter(|p| p.is_absolute()) {
        return Ok(path);
    }
    if cfg!(windows) {
        return Err(format!("{variable} must name an absolute current-user directory."));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join(fallback))
        .ok_or_else(|| "HOME must name an absolute directory.".into())
}
pub fn default_root() -> Result<PathBuf> {
    Ok(if cfg!(windows) { home_dir("LOCALAPPDATA", "")? } else { home_dir("XDG_DATA_HOME", ".local/share")? }.join("PhotoFindry Companion"))
}
fn default_launch_state() -> Result<PathBuf> {
    Ok(if cfg!(windows) {
        home_dir("LOCALAPPDATA", "")?.join("PhotoRecall").join("Launch")
    } else {
        home_dir("XDG_STATE_HOME", ".local/state")?.join("photofindry").join("launch")
    })
}
/// `\\?\C:\...` from canonicalization becomes `C:\...` so paths compare equal
/// to configured ones and can be passed to the Task Scheduler and shell APIs.
fn plain(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            if rest.as_bytes().get(1) == Some(&b':') {
                return PathBuf::from(rest);
            }
        }
    }
    path
}
fn self_executable() -> Result<PathBuf> {
    Ok(plain(fs::canonicalize(std::env::current_exe().map_err(|_| "Cannot locate the companion")?).map_err(|_| "Cannot resolve the companion executable")?))
}
fn installed_root(exe: &Path) -> Option<PathBuf> {
    let package = exe.parent()?.parent()?;
    let version = package.parent()?;
    let versions = version.parent()?;
    (exe.parent()?.file_name()? == "bin"
        && package.file_name()? == "package"
        && versions.file_name()? == "versions"
        && version.file_name()?.to_str()?.parse::<u64>().is_ok())
    .then(|| versions.parent().map(Path::to_path_buf))
    .flatten()
}
fn package(root: &Path, build: u64) -> PathBuf {
    root.join("versions").join(build.to_string()).join("package")
}
fn executable(root: &Path, build: u64) -> PathBuf {
    package(root, build).join("bin").join(EXE)
}
fn verifier(exe: &Path) -> Result<PathBuf> {
    release::canonical(&exe.parent().ok_or("Missing companion directory")?.join(VERIFIER), false)
}
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
fn nonce() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Cannot create a private staging name")?;
    Ok(release::digest(&bytes)[..24].to_owned())
}

// ---- Private files and locks ------------------------------------------------

#[cfg(unix)]
fn private_dir(path: &Path, create: bool) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if !path.is_absolute() {
        return Err("Private companion state requires an absolute directory.".into());
    }
    if create && fs::symlink_metadata(path).is_err() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "Cannot create the parent of private companion state.")?;
        }
        fs::DirBuilder::new().mode(0o700).create(path).map_err(|_| "Cannot create private companion state.")?;
    }
    let m = fs::symlink_metadata(path).map_err(|_| "Private companion state is unavailable.")?;
    if m.file_type().is_symlink() || !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("Private companion state must be a directory owned by this account with mode 0700.".into());
    }
    Ok(())
}
#[cfg(windows)]
fn private_dir(path: &Path, create: bool) -> Result<()> {
    if create && fs::symlink_metadata(path).is_err() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "Cannot create the parent of private companion state.")?;
        }
    }
    photofindry_platform::fs::private_dir(path, create)
        .map(drop)
        .map_err(|e| format!("Private companion state must be restricted to this account: {e}"))
}
#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().ok_or("Missing private state parent")?;
    private_dir(parent, false)?;
    let temp = parent.join(format!(".write-{}", nonce()?));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp)
            .map_err(|_| "Cannot stage private companion state")?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| "Cannot flush private companion state")?;
        fs::rename(&temp, path).map_err(|_| "Cannot publish private companion state")?;
        fs::File::open(parent).and_then(|d| d.sync_all()).map_err(|_| "Cannot flush the companion directory")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
#[cfg(windows)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    photofindry_platform::fs::atomic_write_private(path, bytes).map_err(|e| format!("Cannot save private companion state: {e}"))
}
#[cfg(unix)]
struct Lock(#[allow(dead_code)] fs::File);
#[cfg(unix)]
fn try_lock(root: &Path, name: &str) -> Result<Option<Lock>> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::fd::AsRawFd;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root.join(name))
        .map_err(|_| "Cannot open the companion operation lock")?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(Lock(file)));
    }
    Ok(None)
}
#[cfg(windows)]
struct Lock(#[allow(dead_code)] photofindry_platform::fs::ExclusiveLock);
#[cfg(windows)]
fn try_lock(root: &Path, name: &str) -> Result<Option<Lock>> {
    match photofindry_platform::fs::ExclusiveLock::acquire(&root.join(name)) {
        Ok(lock) => Ok(Some(Lock(lock))),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(format!("Cannot lock companion state: {e}")),
    }
}
fn operation_lock(root: &Path) -> Result<Lock> {
    try_lock(root, "operation.lock")?.ok_or_else(|| "Another companion install, update or uninstall is running. Wait for it to finish, then retry.".into())
}
fn save(root: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    write_private(&root.join(name), &serde_json::to_vec_pretty(value).map_err(|_| "Cannot encode companion state")?)
}
fn clear(root: &Path, name: &str) -> Result<()> {
    match fs::remove_file(root.join(name)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot finish the companion transaction".into()),
    }
}
fn note(root: &Path, phase: &str, message: &str) {
    let _ = save(root, "status.json", &json!({"phase":phase,"message":message,"updatedAt":now()}));
}

// ---- Installation state ------------------------------------------------------

fn validate_state(root: &Path, state: &Installation) -> Result<()> {
    if state.schema != 1
        || state.current == 0
        || state.highest < state.current
        || state.previous == Some(state.current)
        || state.previous.is_some_and(|v| v == 0 || v > state.highest)
        || !state.data.is_absolute()
        || !state.launch_state.is_absolute()
        || state.data.starts_with(root)
        || root.starts_with(&state.data)
        || state.launch_state.starts_with(root)
        || root.starts_with(&state.launch_state)
    {
        return Err("Companion installation paths or version state are invalid".into());
    }
    Ok(())
}
fn read_state(root: &Path) -> Result<Installation> {
    private_dir(root, false)?;
    let state: Installation = serde_json::from_slice(&release::read(&root.join(STATE), 16384)?)
        .map_err(|_| "Companion installation state is invalid. Run lifecycle recover; your library is kept.")?;
    validate_state(root, &state)?;
    Ok(state)
}
fn checked_release(root: &Path, build: u64, helper: &Path) -> Result<release::Release> {
    let envelope = release::read(&root.join("versions").join(build.to_string()).join("release.json"), 131072)?;
    let signed = release::verify(&envelope, helper)?;
    if signed.build != build {
        return Err("Installed version identity changed".into());
    }
    release::verify_package(&package(root, build), &signed)?;
    Ok(signed)
}

// ---- Background service ---------------------------------------------------------

fn service_arguments(root: &Path, cfg: &Installation) -> Vec<String> {
    let lossy = |p: &Path| p.to_string_lossy().into_owned();
    vec![
        "serve".into(),
        "--data-dir".into(),
        lossy(&cfg.data),
        "--runtime".into(),
        lossy(&package(root, cfg.current).join("runtime")),
        "--no-stdin".into(),
        "--launch-state".into(),
        lossy(&cfg.launch_state),
    ]
}
fn identifier(root: &Path) -> Result<Option<String>> {
    Ok((root != default_root()?).then(|| release::digest(root.as_os_str().as_encoded_bytes())[..16].to_owned()))
}
fn run(program: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.output().map_err(|_| format!("Cannot run {program}"))
}
fn require(program: &str, args: &[&str], failure: &str) -> Result<()> {
    let output = run(program, args)?;
    if output.status.success() {
        return Ok(());
    }
    let detail: String = String::from_utf8_lossy(&output.stderr).chars().filter(|c| !c.is_control()).take(400).collect();
    Err(format!("{failure} {detail}").trim().to_owned())
}

#[cfg(target_os = "linux")]
mod service {
    use super::*;
    const SYSTEMCTL: &str = "/usr/bin/systemctl";
    pub fn unit(root: &Path) -> Result<String> {
        Ok(match identifier(root)? {
            None => "photofindry-companion.service".into(),
            Some(id) => format!("photofindry-companion-test-{id}.service"),
        })
    }
    fn unit_path(root: &Path) -> Result<PathBuf> {
        Ok(home_dir("XDG_CONFIG_HOME", ".config")?.join("systemd/user").join(unit(root)?))
    }
    /// systemd command-line quoting; control characters are refused outright.
    fn quote(value: &str) -> Result<String> {
        if value.chars().any(char::is_control) {
            return Err("Installed paths cannot contain control characters".into());
        }
        Ok(format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%").replace('$', "$$")))
    }
    pub fn render(root: &Path, cfg: &Installation) -> Result<String> {
        let mut command = vec![quote(&executable(root, cfg.current).to_string_lossy())?];
        for arg in service_arguments(root, cfg) {
            command.push(quote(&arg)?);
        }
        Ok(format!(
            "# Generated by PhotoFindry. Program files only; the library lives outside {}.\n[Unit]\nDescription=PhotoFindry companion\n\n[Service]\nType=simple\nExecStart={}\nRestart=on-failure\nRestartSec=5\nTimeoutStopSec=120\nKillMode=mixed\n\n[Install]\nWantedBy=default.target\n",
            root.display(),
            command.join(" ")
        ))
    }
    fn systemctl(args: &[&str]) -> Result<()> {
        require(SYSTEMCTL, &[&["--user"], args].concat(), "The systemd user manager refused the request. Check that you are logged in as this user.")
    }
    pub fn install(root: &Path, cfg: &Installation) -> Result<()> {
        let path = unit_path(root)?;
        fs::create_dir_all(path.parent().ok_or("Missing unit directory")?).map_err(|_| "Cannot create the systemd user unit directory")?;
        let staged = path.with_extension(format!("tmp-{}", nonce()?));
        fs::write(&staged, render(root, cfg)?).map_err(|_| "Cannot write the systemd user unit")?;
        fs::rename(&staged, &path).map_err(|_| "Cannot publish the systemd user unit")?;
        systemctl(&["daemon-reload"])?;
        let unit = unit(root)?;
        systemctl(&[if cfg.autostart { "enable" } else { "disable" }, unit.as_str()])?;
        desktop(root,cfg,false)
    }
    pub fn start(root: &Path) -> Result<()> {
        systemctl(&["restart", unit(root)?.as_str()])
    }
    pub async fn stop(root: &Path) -> Result<()> {
        let unit = unit(root)?;
        if run(SYSTEMCTL, &["--user", "cat", unit.as_str()])?.status.success() {
            systemctl(&["stop", unit.as_str()])?;
        }
        Ok(())
    }
    pub async fn remove(root: &Path) -> Result<()> {
        stop(root).await?;
        let unit = unit(root)?;
        let _ = run(SYSTEMCTL, &["--user", "disable", unit.as_str()]);
        match fs::remove_file(unit_path(root)?) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot remove the systemd user unit".into()),
        }
        systemctl(&["daemon-reload"])?;
        if let Ok(cfg)=read_state(root){desktop(root,&cfg,true)?;}
        Ok(())
    }
    pub fn schedule(root: &Path, action: &str) -> Result<()> {
        let unit = unit(root)?.replace(".service", "-maintenance");
        require("/usr/bin/systemd-run", &["--user", "--collect", "--unit", &unit,
            &self_executable()?.to_string_lossy(), "lifecycle", "maintenance", "--action", action,
            "--install-root", &root.to_string_lossy()], "The systemd user manager could not schedule maintenance.")
    }
    pub fn finish_maintenance(_root: &Path) {}
    pub fn registration(root: &Path) -> Result<Value> {
        let unit = unit(root)?;
        let enabled = run(SYSTEMCTL, &["--user", "is-enabled", unit.as_str()])?;
        let active = run(SYSTEMCTL, &["--user", "is-active", unit.as_str()])?;
        Ok(json!({"kind":"systemd-user","unit":unit,"enabled":String::from_utf8_lossy(&enabled.stdout).trim(),"active":String::from_utf8_lossy(&active.stdout).trim()}))
    }
    pub async fn ready(cfg: &Installation) -> Result<Value> {
        super::super::entry::status(&cfg.launch_state).await
    }
}

#[cfg(windows)]
mod service {
    use super::*;
    const SCHTASKS: &str = "C:\\Windows\\System32\\schtasks.exe";
    pub fn unit(root: &Path) -> Result<String> {
        Ok(match identifier(root)? {
            None => "PhotoFindry Companion".into(),
            Some(id) => format!("PhotoFindry Companion Test {id}"),
        })
    }
    fn xml(value: &str) -> Result<String> {
        if value.chars().any(char::is_control) {
            return Err("Installed paths cannot contain control characters".into());
        }
        Ok(value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;"))
    }
    /// Windows command-line quoting for CommandLineToArgvW / the Rust runtime.
    fn argument(value: &str) -> String {
        let mut out = String::from("\"");
        let mut slashes = 0;
        for c in value.chars() {
            match c {
                '\\' => slashes += 1,
                '"' => {
                    out.push_str(&"\\".repeat(slashes * 2 + 1));
                    out.push('"');
                    slashes = 0;
                }
                c => {
                    out.push_str(&"\\".repeat(slashes));
                    out.push(c);
                    slashes = 0;
                }
            }
        }
        out.push_str(&"\\".repeat(slashes * 2));
        out.push('"');
        out
    }
    /// The account's SID: unambiguous for local, Microsoft and domain accounts.
    fn user() -> Result<String> {
        super::super::native::current_sid()
    }
    pub fn render(root: &Path, cfg: &Installation) -> Result<String> {
        let user = xml(&user()?)?;
        // conhost --headless keeps the console companion from opening a window.
        let mut arguments = vec!["--headless".to_owned(), argument(&executable(root, cfg.current).to_string_lossy())];
        arguments.extend(service_arguments(root, cfg).iter().map(|a| argument(a)));
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>PhotoFindry companion. Program files only; the library is kept outside {root}.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>{enabled}</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>C:\Windows\System32\conhost.exe</Command><Arguments>{arguments}</Arguments></Exec></Actions>
</Task>
"#,
            root = xml(&root.to_string_lossy())?,
            enabled = cfg.autostart,
            arguments = xml(&arguments.join(" "))?,
        ))
    }
    pub fn install(root: &Path, cfg: &Installation) -> Result<()> {
        let path = root.join(format!("task-{}.xml", nonce()?));
        let mut bytes = vec![0xff, 0xfe];
        for unit in render(root, cfg)?.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        write_private(&path, &bytes)?;
        let name = unit(root)?;
        let result = require(SCHTASKS, &["/Create", "/TN", name.as_str(), "/XML", &path.to_string_lossy(), "/F"], "Task Scheduler refused the per-user logon task.");
        let _ = fs::remove_file(&path);
        result?;desktop(root,cfg,false)
    }
    pub fn start(root: &Path) -> Result<()> {
        require(SCHTASKS, &["/Run", "/TN", unit(root)?.as_str()], "Task Scheduler could not start the companion.")
    }
    fn present(root: &Path) -> Result<bool> {
        Ok(run(SCHTASKS, &["/Query", "/TN", unit(root)?.as_str()])?.status.success())
    }
    pub async fn stop(root: &Path) -> Result<()> {
        if !present(root)? {
            return Ok(());
        }
        // Ask the running companion to drain its worker before ending the task;
        // the entry waits for the process to exit. /End only covers a hung one.
        if let Ok(cfg) = read_state(root) {
            let _ = super::super::entry::request(&cfg.launch_state, "stop").await;
        }
        let _ = run(SCHTASKS, &["/End", "/TN", unit(root)?.as_str()]);
        Ok(())
    }
    pub async fn remove(root: &Path) -> Result<()> {
        stop(root).await?;
        if let Ok(cfg)=read_state(root){desktop(root,&cfg,true)?;}
        if present(root)? {
            require(SCHTASKS, &["/Delete", "/TN", unit(root)?.as_str(), "/F"], "Task Scheduler could not remove the logon task.")?;
        }
        Ok(())
    }
    pub fn schedule(root: &Path, action: &str) -> Result<()> {
        let cfg=read_state(root)?;
        let rendered=render(root,&cfg)?;
        let start=rendered.find("  <Triggers>").ok_or("Missing task triggers")?;
        let end=rendered.find("</Triggers>").ok_or("Missing task triggers")?+"</Triggers>".len();
        let mut text=rendered.clone();text.replace_range(start..end,"  <Triggers />");
        let begin=text.find("<Arguments>").ok_or("Missing task command")?+"<Arguments>".len();
        let end=text.find("</Arguments>").ok_or("Missing task command")?;
        let arguments=["--headless".to_owned(),argument(&self_executable()?.to_string_lossy()),
            "lifecycle".into(),"maintenance".into(),"--action".into(),action.into(),"--install-root".into(),argument(&root.to_string_lossy())].join(" ");
        text.replace_range(begin..end,&xml(&arguments)?);
        let path=root.join(format!("maintenance-{}.xml",nonce()?));
        let mut bytes=vec![0xff,0xfe];for unit in text.encode_utf16(){bytes.extend_from_slice(&unit.to_le_bytes());}
        write_private(&path,&bytes)?;
        let name=format!("{} Maintenance",unit(root)?);
        let result=require(SCHTASKS,&["/Create","/TN",&name,"/XML",&path.to_string_lossy(),"/F"],"Task Scheduler could not register maintenance.")
            .and_then(|_|require(SCHTASKS,&["/Run","/TN",&name],"Task Scheduler could not start maintenance."));
        let _=fs::remove_file(path);result
    }
    pub fn finish_maintenance(root: &Path) {if let Ok(unit)=unit(root){let _=run(SCHTASKS,&["/Delete","/TN",&format!("{unit} Maintenance"),"/F"]);}}
    pub fn registration(root: &Path) -> Result<Value> {
        let name = unit(root)?;
        let output = run(SCHTASKS, &["/Query", "/TN", name.as_str(), "/XML"])?;
        let text: String = String::from_utf8_lossy(&output.stdout).chars().filter(|c| !c.is_whitespace() && *c != '\0').collect();
        let trigger = text.split_once("<LogonTrigger>").and_then(|(_, rest)| rest.split_once("</LogonTrigger>")).map(|(inside, _)| inside.to_owned());
        Ok(json!({"kind":"scheduled-task","task":name,"present":output.status.success(),"logonTrigger":trigger.is_some(),
            "enabled":trigger.as_deref().is_some_and(|t| !t.contains("<Enabled>false</Enabled>")),"user":user()?}))
    }
    pub async fn ready(cfg: &Installation) -> Result<Value> {
        super::super::entry::request(&cfg.launch_state, "status").await
    }
}

async fn health(cfg: &Installation) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    loop {
        match service::ready(cfg).await {
            Ok(value) => return Ok(json!({"libraryId":value["libraryId"]})),
            Err(error) if tokio::time::Instant::now() >= deadline => {
                return Err(format!("The companion did not become ready: {error}"));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
}
async fn start(root: &Path, cfg: &Installation) -> Result<()> {
    service::install(root, cfg)?;
    service::start(root)?;
    health(cfg).await.map(drop)
}

// Only the account's default installation owns ordinary application launchers.
// Custom qualification roots never modify desktop entries, PATH or URL handlers.
#[cfg(target_os = "linux")]
fn desktop(root: &Path, cfg: &Installation, remove: bool) -> Result<()> {
    use std::os::unix::fs::symlink;
    if root!=default_root()?{return Ok(());}
    let bin=home_dir("HOME","")?.join(".local/bin");
    let apps=home_dir("XDG_DATA_HOME",".local/share")?.join("applications");
    let link=bin.join("photofindry");let entry=apps.join("photofindry.desktop");
    let raw=link.to_string_lossy();
    if raw.chars().any(char::is_control){return Err("Launcher paths cannot contain control characters.".into());}
    let quoted=raw.replace('\\',"\\\\\\\\").replace('"',"\\\\\\\"").replace('$',"\\\\$").replace('`',"\\\\`").replace('%',"%%");
    let text=format!("[Desktop Entry]\nType=Application\nName=PhotoFindry\nComment=Find the photograph you remember\nExec=\"{quoted}\" open\nTerminal=false\nCategories=Graphics;Photography;\n");
    let owned=fs::read_link(&link).ok().is_some_and(|target|target.starts_with(root));
    if remove {
        if owned{fs::remove_file(&link).map_err(|_|"Cannot remove the PhotoFindry command launcher")?;}
        if fs::read_to_string(&entry).ok().as_deref()==Some(text.as_str()){fs::remove_file(entry).map_err(|_|"Cannot remove the PhotoFindry desktop launcher")?;}
        return Ok(());
    }
    if fs::symlink_metadata(&link).is_ok()&&!owned{return Err("An unrelated ~/.local/bin/photofindry already exists. Keep it and choose another installation location.".into());}
    if entry.exists()&&fs::read_to_string(&entry).ok().as_deref()!=Some(text.as_str()){return Err("An unrelated PhotoFindry desktop launcher already exists. Keep it and review its location.".into());}
    fs::create_dir_all(&bin).and_then(|_|fs::create_dir_all(&apps)).map_err(|_|"Cannot create user application launchers")?;
    let staged=bin.join(format!(".photofindry-{}",nonce()?));
    symlink(executable(root,cfg.current),&staged).map_err(|_|"Cannot prepare the PhotoFindry command launcher")?;
    fs::rename(staged,&link).map_err(|_|"Cannot publish the PhotoFindry command launcher")?;
    fs::write(entry,text).map_err(|_|"Cannot publish the PhotoFindry desktop launcher".to_owned())
}
#[cfg(windows)]
fn desktop(root: &Path, cfg: &Installation, remove: bool) -> Result<()> {
    use windows_api::{core::{Interface,PCWSTR},Win32::{System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,CLSCTX_INPROC_SERVER,COINIT_APARTMENTTHREADED,IPersistFile},UI::Shell::{IShellLinkW,ShellLink}}};
    use std::os::windows::ffi::OsStrExt;
    if root!=default_root()?{return Ok(());}
    let entry=home_dir("APPDATA","")?.join("Microsoft/Windows/Start Menu/Programs/PhotoFindry.lnk");
    let record=root.join("desktop-launcher.json");
    let old:Option<String>=release::read(&record,8192).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).and_then(|v|v["sha256"].as_str().map(str::to_owned));
    let existing=fs::read(&entry).ok().map(|bytes|release::digest(&bytes));
    if remove {
        if existing.is_some()&&existing==old{fs::remove_file(entry).map_err(|_|"Cannot remove the PhotoFindry Start menu launcher")?;}
        return Ok(());
    }
    if existing.is_some()&&existing!=old{return Err("An unrelated PhotoFindry Start menu launcher already exists. Keep it and review its location.".into());}
    fs::create_dir_all(entry.parent().ok_or("Missing Start menu directory")?).map_err(|_|"Cannot create the current-user Start menu folder")?;
    let wide=|p:&std::ffi::OsStr|p.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let exe=wide(executable(root,cfg.current).as_os_str());let args=wide(std::ffi::OsStr::new("open"));let output=wide(entry.as_os_str());
    let initialized=unsafe{CoInitializeEx(None,COINIT_APARTMENTTHREADED)};
    if initialized.is_err()&&initialized.0!=0x80010106u32 as i32{return Err("Windows could not initialize the application launcher.".into());}
    let result=(||unsafe{
        let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER).map_err(|_|"Cannot create the Windows application launcher")?;
        link.SetPath(PCWSTR(exe.as_ptr())).map_err(|_|"Cannot set the application launcher path")?;
        link.SetArguments(PCWSTR(args.as_ptr())).map_err(|_|"Cannot set the application launcher action")?;
        let file:IPersistFile=link.cast().map_err(|_|"Cannot save the Windows application launcher")?;
        file.Save(PCWSTR(output.as_ptr()),true).map_err(|_|"Cannot publish the Windows application launcher")?;
        Ok::<(),String>(())
    })();
    if initialized.is_ok(){unsafe{CoUninitialize()};}
    result?;
    let bytes=fs::read(entry).map_err(|_|"Cannot verify the Windows application launcher")?;
    save(root,"desktop-launcher.json",&json!({"sha256":release::digest(&bytes)}))
}

// ---- Versions ------------------------------------------------------------------

fn stage(root: &Path) -> Result<PathBuf> {
    let versions = root.join("versions");
    private_dir(&versions, true)?;
    let path = versions.join(format!(".stage-{}", nonce()?));
    private_dir(&path, true)?;
    Ok(path)
}
/// Remove an unpublished stage. These are only bytes this command created.
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
/// Publish `stage/{package,release.json}` as `versions/<build>`. An existing
/// identical verified version is reused; a damaged one is set aside, never deleted.
fn commit(root: &Path, staged: &Stage, signed: &release::Release, envelope: &[u8], current: Option<u64>) -> Result<()> {
    release::verify_package(&staged.0.join("package"), signed)?;
    write_private(&staged.0.join("release.json"), envelope)?;
    let target = root.join("versions").join(signed.build.to_string());
    if target.exists() {
        let old = release::read(&target.join("release.json"), 131072).ok();
        if old.as_deref() == Some(envelope) && release::verify_package(&target.join("package"), signed).is_ok() {
            return Ok(());
        }
        if current == Some(signed.build) {
            return Err("The running version's files changed. Run lifecycle recover or reinstall; your library is kept.".into());
        }
        let aside = root.join("versions").join(format!(".damaged-{}-{}", signed.build, nonce()?));
        fs::rename(&target, aside).map_err(|_| "Cannot set aside a damaged program version")?;
    }
    fs::rename(&staged.0, &target).map_err(|_| "Cannot publish the verified program version; current software is kept")?;
    Ok(())
}
async fn switch(root: &Path, before: Installation, signed: &release::Release, rollback: bool) -> Result<()> {
    require_idle(&before)?;
    let helper = verifier(&self_executable()?)?;
    checked_release(root, signed.build, &helper)?;
    if !rollback && (signed.build <= before.current || signed.build < before.highest) {
        return Err("The update is older than this installation. Current software is kept.".into());
    }
    let mut after = before.clone();
    after.previous = Some(before.current);
    after.current = signed.build;
    after.highest = before.highest.max(signed.build);
    let mut tx = Transaction { schema: 1, before: Some(before), after: after.clone(), committed: false };
    save(root, JOURNAL, &tx)?;
    note(root, "updating", "Switching companion software. Your library stays in place.");
    let outcome = async {
        service::stop(root).await?;
        save(root, STATE, &after)?;
        start(root, &after).await?;
        tx.committed = true;
        save(root, JOURNAL, &tx)?;
        clear(root, JOURNAL)
    }
    .await;
    if let Err(reason) = outcome {
        if let Err(recovery) = restore(root, &tx).await {
            return Err(format!("{reason} Recovery is pending: {recovery} Run lifecycle recover; do not restore or delete your library."));
        }
        return Err(format!("{reason} The previous version was restored."));
    }
    note(root, "ready", "Companion software switched. Your library, originals and model selections are kept.");
    Ok(())
}
/// Undo an unfinished switch. Program state returns to `before`; the anti-replay
/// watermark keeps the highest build ever accepted. Library files are untouched.
async fn restore(root: &Path, tx: &Transaction) -> Result<()> {
    service::stop(root).await?;
    if let Some(before) = &tx.before {
        let mut before = before.clone();
        before.highest = before.highest.max(tx.after.highest);
        checked_release(root, before.current, &verifier(&self_executable()?)?)?;
        save(root, STATE, &before)?;
        start(root, &before).await?;
    } else {
        service::remove(root).await?;
        clear(root, STATE)?;
    }
    clear(root, JOURNAL)?;
    note(root, "recovered", "Prior companion software restored. Your library was not rolled back.");
    Ok(())
}
async fn recover(root: &Path) -> Result<()> {
    private_dir(root, false)?;
    let _lock = operation_lock(root)?;
    if root.join(JOURNAL).exists() {
        let tx: Transaction = serde_json::from_slice(&release::read(&root.join(JOURNAL), 65536)?)
            .map_err(|_| "Recovery metadata is invalid. Reinstall the verified companion; your library is kept.")?;
        validate_state(root, &tx.after)?;
        if let Some(before) = &tx.before {
            validate_state(root, before)?;
            if before.data != tx.after.data || before.launch_state != tx.after.launch_state {
                return Err("Recovery paths disagree; reinstall the verified companion.".into());
            }
        }
        if tx.schema != 1 {
            return Err("Unsupported recovery transaction".into());
        }
        if !tx.committed {
            return restore(root, &tx).await;
        }
        clear(root, JOURNAL)?;
    }
    let cfg = read_state(root)?;
    if health_once(&cfg).await {
        return Ok(());
    }
    checked_release(root, cfg.current, &verifier(&self_executable()?)?)?;
    start(root, &cfg).await
}
async fn health_once(cfg: &Installation) -> bool {
    service::ready(cfg).await.is_ok()
}

// ---- Download -----------------------------------------------------------------

fn download(url: &str, path: &Path, maximum: u64, feed: bool) -> Result<()> {
    if !(url == release::FEED || url.starts_with(release::RELEASES)) {
        return Err("Unsupported update destination".into());
    }
    let max = maximum.to_string();
    let output = run(
        CURL,
        &[
            "--disable", "--fail", "--silent", "--show-error", "--location", "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "5",
            "--connect-timeout", if feed { "8" } else { "20" }, "--max-time", if feed { "20" } else { "900" }, "--max-filesize", &max,
            "--output", &path.to_string_lossy(), url,
        ],
    )?;
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if !output.status.success() || size == 0 || size > maximum {
        let _ = fs::remove_file(path);
        return Err("Could not download the signed update. Your current version is kept; check your connection and retry.".into());
    }
    Ok(())
}
fn fetch(root: &Path, helper: &Path) -> Result<(release::Release, Vec<u8>)> {
    let path = root.join(format!("feed-{}.tmp", nonce()?));
    let result = download(release::FEED, &path, 131072, true).and_then(|_| release::read(&path, 131072));
    let _ = fs::remove_file(&path);
    let bytes = result?;
    Ok((release::verify(&bytes, helper)?, bytes))
}

// ---- Commands -----------------------------------------------------------------

async fn install(root: &Path, envelope: &Path, data: &Path, state: &Path, autostart: bool) -> Result<Value> {
    if root.starts_with(data) || data.starts_with(root) || state.starts_with(root) || root.starts_with(state) {
        return Err("The program installation must be separate from the library and launch state".into());
    }
    private_dir(root, true)?;
    let lock = operation_lock(root)?;
    let exe = self_executable()?;
    let helper = verifier(&exe)?;
    let bytes = release::read(envelope, 131072)?;
    let signed = release::verify(&bytes, &helper)?;
    let source = exe.parent().and_then(Path::parent).ok_or("Cannot locate the signed package")?.to_path_buf();
    release::verify_package(&source, &signed)?;
    if root.join(STATE).exists() {
        let cfg = read_state(root)?;
        if cfg.data != data || cfg.launch_state != state {
            return Err("Another library is registered for this installation. Keep it in place, or uninstall first.".into());
        }
        if signed.build > cfg.current && signed.build >= cfg.highest {
            let staged = Stage(stage(root)?);
            release::copy_package(&source, &staged.0.join("package"), &signed)?;
            commit(root, &staged, &signed, &bytes, Some(cfg.current))?;
            switch(root, cfg, &signed, false).await?;
        } else {
            checked_release(root, cfg.current, &helper)?;
            start(root, &cfg).await?;
        }
        drop(lock);
        return status(root).await;
    }
    let staged = Stage(stage(root)?);
    release::copy_package(&source, &staged.0.join("package"), &signed)?;
    commit(root, &staged, &signed, &bytes, None)?;
    if fs::symlink_metadata(data).is_err() {
        if let Some(parent) = data.parent() {
            fs::create_dir_all(parent).map_err(|_| "Cannot create the library's parent directory")?;
        }
        super::create_library(data)?;
    }
    private_dir(state, true)?;
    let cfg = Installation { schema: 1, current: signed.build, previous: None, highest: signed.build, data: data.to_path_buf(), launch_state: state.to_path_buf(), autostart };
    let mut tx = Transaction { schema: 1, before: None, after: cfg.clone(), committed: false };
    save(root, JOURNAL, &tx)?;
    let result = async {
        save(root, STATE, &cfg)?;
        start(root, &cfg).await?;
        tx.committed = true;
        save(root, JOURNAL, &tx)?;
        clear(root, JOURNAL)
    }
    .await;
    if let Err(error) = result {
        if let Err(recovery) = restore(root, &tx).await {
            return Err(format!("{error} Recovery is pending: {recovery} Your library is kept."));
        }
        return Err(error);
    }
    note(root, "ready", "Companion installed. Your library is kept; processing stays paused until you start it.");
    drop(lock);
    status(root).await
}
fn require_idle(cfg: &Installation) -> Result<()> {
    let path=cfg.data.join("jobs.sqlite");
    if !path.exists(){return Ok(());}
    let db=rusqlite::Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_|"Cannot check saved work before maintenance. Current software is kept.")?;
    db.busy_timeout(Duration::from_secs(2)).map_err(|_|"Cannot check saved work.")?;
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE state IN ('running','pausing'))",[],|row|row.get(0))
        .map_err(|_|"Cannot check saved work before maintenance. Current software is kept.")?;
    if busy{return Err("Finish or pause active jobs before switching companion versions. Your current library remains usable.".into());}
    Ok(())
}
async fn update(root: &Path, local: Option<&Path>) -> Result<()> {
    let lock = operation_lock(root)?;
    let cfg = read_state(root)?;
    let helper = verifier(&self_executable()?)?;
    let (next, bytes) = match local {
        Some(dir) => {
            let raw = release::read(&dir.join("release.json"), 131072)?;
            (release::verify(&raw, &helper)?, raw)
        }
        None => fetch(root, &helper)?,
    };
    if next.build <= cfg.current || next.build < cfg.highest {
        return Err("No newer signed release is available. Current software is unchanged.".into());
    }
    require_idle(&cfg)?;
    let staged = Stage(stage(root)?);
    let archive = match local {
        Some(dir) => dir.join(release::archive_name(&next.version)),
        None => {
            let path = staged.0.join(release::archive_name(&next.version));
            note(root, "downloading", "Downloading the signed companion update.");
            download(&next.archive_url, &path, next.archive_bytes, false)?;
            path
        }
    };
    release::extract(&archive, &staged.0.join("package"), &next)?;
    if local.is_none() {
        let _ = fs::remove_file(&archive);
    }
    commit(root, &staged, &next, &bytes, Some(cfg.current))?;
    switch(root, cfg, &next, false).await?;
    drop(lock);
    Ok(())
}
async fn rollback(root: &Path) -> Result<()> {
    let _lock = operation_lock(root)?;
    let cfg = read_state(root)?;
    require_idle(&cfg)?;
    let previous = cfg.previous.ok_or("No earlier signed version is retained for rollback.")?;
    let signed = checked_release(root, previous, &verifier(&self_executable()?)?)?;
    switch(root, cfg, &signed, true).await
}
async fn check(root: &Path) -> Result<Value> {
    let cfg = read_state(root)?;
    let (latest, _) = fetch(root, &verifier(&self_executable()?)?)?;
    if latest.build < cfg.highest {
        return Err("The server offered an older release. Current software is kept; check again later.".into());
    }
    save(root,"available.json",&latest)?;
    note(root,if latest.build>cfg.current{"available"}else{"ready"},if latest.build>cfg.current{"A newer publisher-signed update is available."}else{"The latest publisher-signed companion is installed."});
    Ok(json!({"current":cfg.current,"available":latest.build > cfg.current,"latest":latest}))
}
async fn status(root: &Path) -> Result<Value> {
    let cfg = read_state(root)?;
    let signed = release::read(&root.join("versions").join(cfg.current.to_string()).join("release.json"), 131072)
        .ok()
        .and_then(|b| release::verify(&b, &verifier(&executable(root, cfg.current)).ok()?).ok());
    let ready = service::ready(&cfg).await.ok();
    Ok(json!({
        "installed":true,"automaticUpdates":false,"previousBuild":cfg.previous,"recoveryPending":root.join(JOURNAL).exists(),
        "status":release::read(&root.join("status.json"),131072).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()),
        "available":release::read(&root.join("available.json"),131072).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()),
        "platform":release::PLATFORM,"build":cfg.current,"version":signed.map(|r|r.version),"previous":cfg.previous,"highest":cfg.highest,
        "data":cfg.data,"launchState":cfg.launch_state,"autostart":cfg.autostart,"service":service::registration(root)?,
        "ready":ready.is_some(),"libraryId":ready.map(|v|v["libraryId"].clone()),"pendingRecovery":root.join(JOURNAL).exists(),
    }))
}
async fn autostart(root: &Path, enabled: bool) -> Result<()> {
    let _lock = operation_lock(root)?;
    let mut cfg = read_state(root)?;
    cfg.autostart = enabled;
    save(root, STATE, &cfg)?;
    service::install(root, &cfg)
}

/// Move the program root to the account's Trash / Recycle Bin. Never the library.
#[cfg(unix)]
fn trash(path: &Path) -> Result<PathBuf> {
    let trash = home_dir("XDG_DATA_HOME", ".local/share")?.join("Trash");
    for dir in ["files", "info"] {
        fs::create_dir_all(trash.join(dir)).map_err(|_| "Cannot open the Trash")?;
    }
    let name = format!("{}.{}", path.file_name().ok_or("Invalid program path")?.to_string_lossy(), nonce()?);
    let info = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", percent(&path.to_string_lossy()), deletion_date());
    fs::write(trash.join("info").join(format!("{name}.trashinfo")), info).map_err(|_| "Cannot record the Trash entry")?;
    let target = trash.join("files").join(&name);
    fs::rename(path, &target).map_err(|_| "Cannot move the program to the Trash; it must be on the same disk as your home folder")?;
    Ok(target)
}
#[cfg(unix)]
fn percent(value: &str) -> String {
    value.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}
#[cfg(unix)]
fn deletion_date() -> String {
    let mut t: libc::tm = unsafe { std::mem::zeroed() };
    let now = now() as libc::time_t;
    unsafe { libc::localtime_r(&now, &mut t) };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", t.tm_year + 1900, t.tm_mon + 1, t.tm_mday, t.tm_hour, t.tm_min, t.tm_sec)
}
#[cfg(windows)]
fn recycle(path: &Path) -> Result<()> {
    use windows_api::Win32::UI::Shell::{FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW};
    use windows_api::core::PCWSTR;
    use std::os::windows::ffi::OsStrExt;
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 || operation.fAnyOperationsAborted.as_bool() || path.exists() {
        return Err(format!("Cannot move the program to the Recycle Bin (code {code})."));
    }
    Ok(())
}
/// Windows lets a folder holding a running executable be renamed but not
/// deleted. Rename the program root aside now; when this uninstaller runs from
/// inside it, a private copy of its bin/ waits for this process to exit and then
/// moves the renamed folder to the Recycle Bin.
#[cfg(windows)]
fn trash(path: &Path, build: u64) -> Result<PathBuf> {
    let aside = path.with_file_name(format!("{}.removed-{}", path.file_name().ok_or("Invalid program path")?.to_string_lossy(), nonce()?));
    fs::rename(path, &aside).map_err(|_| "Cannot remove the program folder. Close any window using it and retry; your library is kept.")?;
    if recycle(&aside).is_ok() {
        return Ok(PathBuf::from("Recycle Bin"));
    }
    let bin = package(&aside, build).join("bin");
    let temp = home_dir("TEMP", "")?.join(format!("PhotoFindry-uninstall-{}", nonce()?));
    private_dir(&temp, true)?;
    for entry in fs::read_dir(&bin).map_err(|_| "Cannot read the program directory")? {
        let file = entry.map_err(|_| "Cannot read the program directory")?.path();
        if file.is_file() {
            fs::copy(&file, temp.join(file.file_name().ok_or("Invalid program file")?)).map_err(|_| "Cannot prepare the uninstall finisher")?;
        }
    }
    use std::os::windows::process::CommandExt;
    Command::new(temp.join(EXE))
        .args(["lifecycle", "finish-uninstall", "--removed", &aside.to_string_lossy(), "--after-pid", &std::process::id().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000 | 0x0000_0008) // CREATE_NO_WINDOW | DETACHED_PROCESS
        .spawn()
        .map_err(|_| "Cannot start the uninstall finisher")?;
    Ok(aside)
}
#[cfg(windows)]
fn finish_uninstall(removed: &Path, pid: u32) -> Result<()> {
    let name = removed.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let suffix = name.rsplit_once(".removed-").map(|(_, s)| s).unwrap_or("");
    // Only a renamed program root qualifies: never an arbitrary folder.
    if suffix.len() != 24 || !suffix.bytes().all(|b| b.is_ascii_hexdigit())
        || !removed.join("versions").is_dir() || removed.join(STATE).exists() {
        return Err("Not a removed PhotoFindry program folder".into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    loop {
        if !photofindry_platform::process::process_alive(pid).unwrap_or(false) && recycle(removed).is_ok() {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            return Err("The removed program folder is still in use".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
async fn uninstall(root: &Path) -> Result<Value> {
    let lock = operation_lock(root)?;
    let cfg = read_state(root)?;
    if root.join(JOURNAL).exists() {
        return Err("An update is unfinished. Run lifecycle recover first; your library is kept.".into());
    }
    note(root, "uninstalling", "Removing companion software. Your library is kept.");
    service::remove(root).await?;
    clear(root, STATE)?;
    drop(lock);
    #[cfg(windows)]
    let trashed = trash(root, cfg.current)?;
    #[cfg(unix)]
    let trashed = trash(root)?;
    Ok(json!({"status":"uninstalled","data":cfg.data,"launchState":cfg.launch_state,"program":trashed,
        "message":"Companion removed and autostart disabled. Your library, original photos and model files are kept. Reinstall to reopen the same library."}))
}

fn same_library_directory(left: &Path, right: &Path) -> Result<bool> {
    #[cfg(windows)]
    {
        // Configured DOS paths and the serving host's extended paths can name
        // the same folder. Compare held native identities, never shortened IDs
        // or case-folded strings, and retain both private-directory boundaries.
        use photofindry_platform::fs as native;
        let left=native::private_dir(left,false).map_err(|_|"Cannot verify the installed library directory.")?;
        let right=native::private_dir(right,false).map_err(|_|"Cannot verify the serving library directory.")?;
        let a=native::identity(&left).map_err(|_|"Cannot identify the installed library directory.")?;
        let b=native::identity(&right).map_err(|_|"Cannot identify the serving library directory.")?;
        Ok(a.volume==b.volume && a.file_id==b.file_id)
    }
    #[cfg(not(windows))]
    {Ok(left==right)}
}
pub async fn web_status(data: &Path) -> Result<Value> {
    let Some(root)=installed_root(&self_executable()?)else{return Ok(json!({"installed":false,"message":"Install the signed companion to enable updates and login startup."}));};
    if !same_library_directory(&read_state(&root)?.data,data)?{return Err("This process does not own the installed library.".into());}
    status(&root).await
}
pub fn schedule(action: &str, data: &Path) -> Result<()> {
    if !matches!(action,"check"|"update"|"rollback"|"recover"|"enable-autostart"|"disable-autostart"){return Err("Unsupported maintenance action.".into());}
    let root=installed_root(&self_executable()?).ok_or("Open the installed signed companion to manage updates.")?;
    let lock=operation_lock(&root)?;
    let cfg=read_state(&root)?;
    if !same_library_directory(&cfg.data,data)?{return Err("This process does not own the installed library.".into());}
    if matches!(action,"update"|"rollback"|"recover"){require_idle(&cfg)?;}
    note(&root,"queued","Companion maintenance is scheduled. Your library stays in place.");
    dispatch_maintenance(lock, || service::schedule(&root,action))
}
fn dispatch_maintenance(lock: Lock, dispatch: impl FnOnce() -> Result<()>) -> Result<()> {
    // systemd-run can start the child before it returns. The child owns the
    // mutation lock and revalidates the installation/idle queue itself. Keeping
    // the parent's validation lock here races that child. The fixed per-root
    // transient unit still rejects overlapping maintenance dispatches.
    #[cfg(target_os = "linux")]
    drop(lock);
    #[cfg(windows)]
    let _lock = lock;
    dispatch()
}
// Explicit default Open checks the fixed signed feed; isolated entry requests
// keep their fixture paths and never contact or claim the default installation.
pub async fn default_open() -> Result<PathBuf> {
    let exe=self_executable()?;
    let root=installed_root(&exe).map(Ok).unwrap_or_else(default_root)?;
    let cfg=read_state(&root)?;
    let checked=check(&root).await;
    match checked {
        Ok(value) if value["available"]==true => {
            if let Err(reason)=update(&root,None).await{note(&root,"available",&reason);}
        }
        Err(reason)=>note(&root,"error",&reason),
        _=>{}
    }
    let cfg=read_state(&root).unwrap_or(cfg);
    if service::ready(&cfg).await.is_err(){start(&root,&cfg).await?;}
    Ok(cfg.launch_state)
}
pub fn handles(name: &str) -> bool {
    matches!(name, "lifecycle" | "uninstall")
}
pub async fn command(args: &[String]) -> Result<()> {
    // Program files and state stay private regardless of the caller's umask.
    #[cfg(unix)]
    unsafe {
        libc::umask(0o077);
    }
    let uninstalling = args[0] == "uninstall";
    let operation = if uninstalling { "uninstall" } else { args.get(1).map(String::as_str).ok_or(USAGE)? };
    let mut maintenance_action=None;
    let mut options = args[if uninstalling { 1 } else { 2 }..].iter();
    let (mut root, mut data, mut state, mut envelope, mut from, mut confirm, mut autostart_on) = (None, None, None, None, None, false, true);
    let (mut removed, mut after): (Option<PathBuf>, Option<u32>) = (None, None);
    #[cfg(unix)]
    let _ = (&removed, &after);
    let path = |v: Option<&String>| -> Result<PathBuf> {
        let p = PathBuf::from(v.ok_or("Missing path value")?);
        if !p.is_absolute() {
            return Err("Use absolute paths.".into());
        }
        Ok(p)
    };
    while let Some(option) = options.next() {
        match option.as_str() {
            "--action" if operation=="maintenance" => maintenance_action=Some(options.next().ok_or(USAGE)?.clone()),
            "--install-root" => root = Some(path(options.next())?),
            "--data-dir" => data = Some(path(options.next())?),
            "--launch-state" => state = Some(path(options.next())?),
            "--release" => envelope = Some(path(options.next())?),
            "--from-directory" => from = Some(path(options.next())?),
            "--no-autostart" => autostart_on = false,
            "--confirm" => confirm = true,
            "--removed" => removed = Some(path(options.next())?),
            "--after-pid" => after = Some(options.next().and_then(|v| v.parse().ok()).ok_or(USAGE)?),
            _ => return Err(USAGE.into()),
        }
    }
    let exe = self_executable()?;
    let root = match root {
        Some(root) => root,
        None => installed_root(&exe).map(Ok).unwrap_or_else(default_root)?,
    };
    let print = |value: Value| println!("{value}");
    if operation=="maintenance" {
        let action=maintenance_action.ok_or(USAGE)?;
        if !matches!(action.as_str(),"check"|"update"|"rollback"|"recover"|"enable-autostart"|"disable-autostart"){return Err(USAGE.into());}
        let result=Box::pin(command(&["lifecycle".into(),action,"--install-root".into(),root.to_string_lossy().into_owned()])).await;
        match &result{Ok(())=>{let phase=if root.join("status.json").exists(){release::read(&root.join("status.json"),131072).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok())}else{None};if phase.as_ref().is_none_or(|v|v["phase"]!="available"){note(&root,"ready","Companion maintenance finished. Your library is kept.");}},Err(reason)=>note(&root,"error",reason)}
        service::finish_maintenance(&root);return result;
    }
    match operation {
        "install" => print(install(&root, &envelope.ok_or("Choose the signed --release envelope.")?, &data.map(Ok).unwrap_or_else(super::default_data)?, &state.map(Ok).unwrap_or_else(default_launch_state)?, autostart_on).await?),
        "status" => print(status(&root).await?),
        "check" => print(check(&root).await?),
        "update" => {
            update(&root, from.as_deref()).await?;
            print(status(&root).await?)
        }
        "rollback" => {
            rollback(&root).await?;
            print(status(&root).await?)
        }
        "recover" => {
            recover(&root).await?;
            print(status(&root).await?)
        }
        "start" => {
            let cfg = read_state(&root)?;
            start(&root, &cfg).await?;
            print(status(&root).await?)
        }
        "stop" => {
            let _lock = operation_lock(&root)?;
            service::stop(&root).await?;
            print(json!({"stopped":true}))
        }
        "enable-autostart" | "disable-autostart" => {
            autostart(&root, operation == "enable-autostart").await?;
            print(status(&root).await?)
        }
        "uninstall" => {
            if !confirm {
                return Err("Uninstall removes the companion program and its autostart entry and keeps your library. Repeat with --confirm.".into());
            }
            print(uninstall(&root).await?)
        }
        #[cfg(windows)]
        "finish-uninstall" => finish_uninstall(&removed.ok_or(USAGE)?, after.ok_or(USAGE)?)?,
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
const USAGE: &str = "Use lifecycle install --release FILE [--data-dir DIR] [--launch-state DIR] [--no-autostart], lifecycle status|check|update [--from-directory DIR]|rollback|recover|start|stop|enable-autostart|disable-autostart, or uninstall --confirm. Every command accepts --install-root DIR.";

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn maintenance_child_can_lock_before_dispatch_returns_and_excludes_other_operations() {
        let temp=std::env::temp_dir().join(format!("pf-handoff-{}-{}",std::process::id(),nonce().unwrap()));
        private_dir(&temp,true).unwrap();
        let parent=operation_lock(&temp).unwrap();
        assert!(operation_lock(&temp).is_err(),"Validation must exclude another operation");
        dispatch_maintenance(parent,|| {
            // A separate thread/open file description models a child started
            // synchronously by systemd-run, before dispatch has returned.
            let (ready_tx,ready_rx)=std::sync::mpsc::channel();
            let (done_tx,done_rx)=std::sync::mpsc::channel();
            let child_root=temp.clone();
            let child=std::thread::spawn(move|| {
                let child_lock=operation_lock(&child_root);
                ready_tx.send(child_lock.is_ok()).unwrap();
                done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                drop(child_lock);
            });
            let acquired=ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let competing=operation_lock(&temp);
            done_tx.send(()).unwrap();
            child.join().unwrap();
            assert!(acquired,"The child must acquire its operation lock while dispatch is still running");
            assert!(competing.is_err(),"The child's mutation lock must exclude competing operations");
            Ok(())
        }).unwrap();
        drop(operation_lock(&temp).unwrap());
        let parent=operation_lock(&temp).unwrap();
        assert!(dispatch_maintenance(parent,||Err("scheduler rejected request".into())).is_err());
        drop(operation_lock(&temp).unwrap());
        fs::remove_dir_all(temp).unwrap();
    }
    fn sample(root: &Path) -> Installation {
        let base = root.parent().unwrap();
        Installation { schema: 1, current: 132, previous: Some(131), highest: 132, data: base.join("Library"), launch_state: base.join("Launch"), autostart: true }
    }
    fn root() -> PathBuf {
        if cfg!(windows) { PathBuf::from(r"C:\Users\Test User\AppData\Local\PhotoFindry Companion") } else { PathBuf::from("/home/test user/.local/share/PhotoFindry Companion") }
    }
    #[test]
    fn installation_state_rejects_replay_overlap_and_bad_versions() {
        let root = root();
        validate_state(&root, &sample(&root)).unwrap();
        let mut s = sample(&root); s.highest = 131; assert!(validate_state(&root, &s).is_err());
        let mut s = sample(&root); s.previous = Some(132); assert!(validate_state(&root, &s).is_err());
        let mut s = sample(&root); s.previous = Some(133); assert!(validate_state(&root, &s).is_err());
        let mut s = sample(&root); s.data = root.join("Library"); assert!(validate_state(&root, &s).is_err());
        let mut s = sample(&root); s.launch_state = root.clone(); assert!(validate_state(&root, &s).is_err());
        let mut s = sample(&root); s.current = 0; assert!(validate_state(&root, &s).is_err());
    }
    #[test]
    fn version_switch_requires_an_idle_readable_saved_queue() {
        let temp=std::env::temp_dir().join(format!("pf-idle-{}-{}",std::process::id(),nonce().unwrap()));
        fs::create_dir_all(&temp).unwrap();
        let mut cfg=sample(&root());cfg.data=temp.clone();
        let db=rusqlite::Connection::open(temp.join("jobs.sqlite")).unwrap();
        assert!(require_idle(&cfg).is_err(),"An unreadable/unknown queue must not be treated as idle");
        db.execute_batch("CREATE TABLE jobs(state TEXT);INSERT INTO jobs VALUES ('running')").unwrap();
        assert!(require_idle(&cfg).is_err());
        db.execute("UPDATE jobs SET state='pausing'",[]).unwrap();assert!(require_idle(&cfg).is_err());
        db.execute("UPDATE jobs SET state='paused'",[]).unwrap();assert!(require_idle(&cfg).is_ok());
        db.execute("UPDATE jobs SET state='completed'",[]).unwrap();assert!(require_idle(&cfg).is_ok());
        drop(db);fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn only_versioned_package_executables_name_an_installation() {
        let root = root();
        assert_eq!(installed_root(&executable(&root, 131)), Some(root.clone()));
        assert_eq!(installed_root(&root.join("bin").join(EXE)), None);
        assert_eq!(installed_root(&root.join("versions/x/package/bin").join(EXE)), None);
    }
    #[cfg(windows)]
    #[test]
    fn browser_ownership_accepts_extended_spelling_but_rejects_another_directory() {
        let temp=std::env::temp_dir().join(format!("pf-web-owner-{}",nonce().unwrap()));
        drop(photofindry_platform::fs::private_dir(&temp,true).unwrap());
        let other=temp.join("other");
        drop(photofindry_platform::fs::private_dir(&other,true).unwrap());
        let extended=fs::canonicalize(&temp).unwrap();
        assert!(same_library_directory(&plain(temp.clone()),&extended).unwrap());
        assert!(!same_library_directory(&temp,&other).unwrap());
        assert!(same_library_directory(&temp,&temp.join("missing")).is_err());
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn service_registration_runs_the_current_version_with_explicit_paths() {
        let root = root();
        let text = service::render(&root, &sample(&root)).unwrap();
        let runtime = package(&root, 132).join("runtime");
        assert!(text.contains("serve") && text.contains("--no-stdin") && text.contains("--launch-state"));
        assert!(!text.contains(&package(&root, 131).to_string_lossy().into_owned()));
        if cfg!(windows) {
            assert!(text.contains("<LogonTrigger><Enabled>true</Enabled>") && text.contains("<RunLevel>LeastPrivilege</RunLevel>") && text.contains("InteractiveToken"));
            assert!(text.contains(&format!("&quot;{}&quot;", runtime.display())));
        } else {
            assert!(text.contains(&format!("\"{}\"", runtime.display())) && text.contains("WantedBy=default.target"));
        }
        let mut odd = sample(&root);
        odd.data = root.parent().unwrap().join("Lib\nrary");
        assert!(service::render(&root, &odd).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_quoting_escapes_specifiers_and_variables() {
        let mut cfg = sample(&root());
        cfg.data = PathBuf::from("/home/x/100% \"$HOME\" \\lib");
        let text = service::render(&root(), &cfg).unwrap();
        assert!(text.contains(r#""/home/x/100%% \"$$HOME\" \\lib""#), "{text}");
    }
    #[cfg(windows)]
    #[test]
    fn task_xml_escapes_markup_in_paths() {
        let mut cfg = sample(&root());
        cfg.data = PathBuf::from(r"C:\Users\x\A&B <lib>");
        let text = service::render(&root(), &cfg).unwrap();
        assert!(text.contains("A&amp;B &lt;lib&gt;") && !text.contains("A&B <lib>"));
    }
}
