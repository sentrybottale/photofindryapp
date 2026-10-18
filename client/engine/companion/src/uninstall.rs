//! Local, recoverable package removal. This module never removes library data.
use super::launch::{self, Registration};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs::{self, File, OpenOptions}, io::Read,
    os::{fd::AsRawFd, unix::fs::{MetadataExt, OpenOptionsExt}}, path::{Component, Path, PathBuf},
    process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Result<T> = std::result::Result<T, String>;
const MANIFEST: &str = "SHA256SUMS.txt";
const MAX_MANIFEST: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 8192;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TOTAL: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Plan, Confirm, Interactive }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub package_path: PathBuf,
    pub library_path: PathBuf,
    pub launch_state_path: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub preserves_library: bool,
    pub preserves_models: bool,
    pub preserves_originals: bool,
    pub preserves_settings: bool,
    pub removes_only_verified_package: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub status: &'static str,
    pub plan: Plan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trashed_path: Option<PathBuf>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Receipt {
    version: u32,
    status: String,
    registration: Registration,
    manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trashed_path: Option<PathBuf>,
}
#[derive(Debug)]
struct Verified { plan: Plan, digest: String }

fn uid() -> u32 { unsafe {libc::geteuid()} }
fn safe_ancestry(path: &Path) -> Result<()> {
    let text = path.to_str().ok_or("Uninstall paths must be valid text.")?;
    if !path.is_absolute() || text.len()>4096 || text.chars().any(char::is_control)
        || text.split('/').any(|v|matches!(v,"."|"..")) || text.contains("//") {
        return Err("Uninstall requires a normalized absolute installed package path.".into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {Component::RootDir | Component::Normal(_) => current.push(component.as_os_str()),
            _ => return Err("Uninstall paths cannot contain dot components.".into())}
        let metadata = fs::symlink_metadata(&current).map_err(|_|"An uninstall path is unavailable.")?;
        if metadata.file_type().is_symlink() || launch::trusted_owner(&current, &metadata).is_err() {
            return Err("Uninstall paths cannot traverse links or another account's writable directories.".into());
        }
    }
    Ok(())
}
fn owned_metadata(path: &Path, directory: bool) -> Result<fs::Metadata> {
    let value = fs::symlink_metadata(path).map_err(|_|"The installed package changed or is unavailable.")?;
    if value.uid() != uid() || value.mode() & 0o022 != 0 || value.file_type().is_symlink()
        || if directory {!value.is_dir()} else {!value.is_file() || value.nlink() != 1} {
        return Err("The package contains linked, special or unsafe-owner files. No files were removed.".into());
    }
    Ok(value)
}
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino() && a.len() == b.len() && a.mode() == b.mode()
        && a.mtime() == b.mtime() && a.mtime_nsec() == b.mtime_nsec()
}
fn open_file(path: &Path) -> Result<(File, fs::Metadata)> {
    let before = owned_metadata(path, false)?;
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK)
        .open(path).map_err(|_|"Cannot read an installed package file.")?;
    if !same_file(&before, &file.metadata().map_err(|_|"Cannot inspect an installed package file.")?) {
        return Err("An installed package file changed while opening it.".into());
    }
    Ok((file, before))
}
fn relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) || value.contains('\\')
        || value.split('/').any(|v|matches!(v,""|"."|"..")) || path.components().count()>16
        || path.components().any(|c|!matches!(c, Component::Normal(_)))
        || path.components().any(|c|c.as_os_str().to_str().is_none_or(|s|s.starts_with('.')))
        || path.to_string_lossy() != value || value.contains("//") || value.ends_with('/') {
        return Err("The installed package inventory contains an unsafe path.".into());
    }
    Ok(path.to_path_buf())
}
fn overlaps(a: &Path,b: &Path) -> Result<bool> {
    let a_info=fs::metadata(a).map_err(|_|"Cannot inspect uninstall path identity.")?;
    let b_info=fs::metadata(b).map_err(|_|"Cannot inspect uninstall path identity.")?;
    for (path,identity) in [(a,&b_info),(b,&a_info)] {
        for parent in path.ancestors() {
            let info=fs::metadata(parent).map_err(|_|"Cannot inspect uninstall path ancestry.")?;
            if info.dev()==identity.dev() && info.ino()==identity.ino() {return Ok(true);}
        }
    }
    Ok(false)
}
fn package_root(record: &Registration, executable: &Path) -> Result<PathBuf> {
    safe_ancestry(executable)?;
    if fs::canonicalize(executable).map_err(|_|"Cannot locate this installed companion.")? != record.executable
        || executable != record.executable || executable.file_name().is_none_or(|name|name != "photofindry") {
        return Err("Only the currently registered installed companion can uninstall itself.".into());
    }
    let bin = executable.parent().ok_or("Cannot locate the installed package.")?;
    let root = bin.parent().ok_or("Cannot locate the installed package.")?;
    if bin.file_name().is_none_or(|name|name != "bin") || root.parent().is_none()
        || root.components().count() < 3 || record.runtime != root.join("runtime") {
        return Err("Uninstall is available only for a complete installed PhotoFindry package, not a development executable.".into());
    }
    safe_ancestry(root)?;
    owned_metadata(root, true)?;
    safe_ancestry(&record.data_dir)?;
    if overlaps(&record.data_dir,root)? {
        return Err("The package and library overlap. Move the package outside the library before uninstalling; no files were removed.".into());
    }
    Ok(root.to_path_buf())
}
fn verify(record: &Registration, executable: &Path, state: &Path, checksums: bool) -> Result<Verified> {
    let root = package_root(record, executable)?;
    if overlaps(state,&root)? {
        return Err("The package and local launch state must be separate directories.".into());
    }
    let (manifest, metadata) = open_file(&root.join(MANIFEST))?;
    if metadata.len() > MAX_MANIFEST {return Err("The package inventory exceeds its size bound.".into());}
    let mut raw = Vec::new();
    manifest.take(MAX_MANIFEST+1).read_to_end(&mut raw).map_err(|_|"Cannot read the package inventory.")?;
    if raw.len() as u64 > MAX_MANIFEST || raw.is_empty() {return Err("The package inventory is empty or oversized.".into());}
    let text = std::str::from_utf8(&raw).map_err(|_|"The package inventory is not valid text.")?;
    let mut expected = BTreeMap::new();
    let mut directories = BTreeSet::new();
    for line in text.lines() {
        let (digest, name) = line.split_once("  ").ok_or("The package inventory has an invalid row.")?;
        if digest.len()!=64 || !digest.bytes().all(|b|b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            return Err("The package inventory has an invalid checksum.".into());
        }
        let path = relative(name)?;
        if path == Path::new(MANIFEST) || expected.insert(path.clone(), digest.to_owned()).is_some() {
            return Err("The package inventory contains duplicate or recursive entries.".into());
        }
        for parent in path.ancestors().skip(1).filter(|p|!p.as_os_str().is_empty()) {directories.insert(parent.to_path_buf());}
        if expected.len()>MAX_FILES {return Err("The package inventory contains too many entries.".into());}
    }
    for required in ["bin/photofindry", "bin/CompanionLauncher", "bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher",
        "bin/CompanionLauncher.app/Contents/Info.plist", "runtime/PhotoWorker", "runtime/PhotoEngine", "runtime/engine-manifest.json", "LICENSE", "README.txt"] {
        if !expected.contains_key(Path::new(required)) {return Err("This is not a complete installed PhotoFindry package.".into());}
    }
    let mut found = BTreeSet::new();
    let mut pending = vec![root.clone()];
    let mut total = 0u64;
    let mut count = 0;
    while let Some(directory) = pending.pop() {
        owned_metadata(&directory, true)?;
        for entry in fs::read_dir(&directory).map_err(|_|"Cannot inspect the complete installed package.")? {
            let path = entry.map_err(|_|"Cannot inspect an installed package entry.")?.path();
            let name = path.strip_prefix(&root).map_err(|_|"An installed package entry escaped its root.")?.to_path_buf();
            let info = fs::symlink_metadata(&path).map_err(|_|"An installed package entry disappeared.")?;
            if info.is_dir() && !info.file_type().is_symlink() {
                if !directories.contains(&name) {return Err("The package contains an unknown or empty directory. Move your extra files elsewhere before uninstalling.".into());}
                owned_metadata(&path, true)?; pending.push(path); continue;
            }
            count += 1;
            if count>MAX_FILES+1 {return Err("The installed package contains too many entries.".into());}
            if name == Path::new(MANIFEST) {owned_metadata(&path, false)?; continue;}
            let expected_digest = expected.get(&name).ok_or("The package contains extra files. Move your extra files elsewhere before uninstalling.")?;
            let (mut file, before) = open_file(&path)?;
            total = total.checked_add(before.len()).ok_or("The package exceeds its size limit.")?;
            if before.len()>MAX_FILE || total>MAX_TOTAL {return Err("The package exceeds its size limit.".into());}
            if !checksums {found.insert(name);continue;}
            let mut hash = Sha256::new(); let mut buffer = [0u8; 64*1024]; let mut read = 0u64;
            loop {
                let n = file.read(&mut buffer).map_err(|_|"Cannot checksum the installed package.")?;
                if n==0 {break;}
                read+=n as u64;
                if read>before.len() {return Err("An installed file grew during verification.".into());}
                hash.update(&buffer[..n]);
            }
            if !same_file(&before, &owned_metadata(&path, false)?) || read != before.len()
                || format!("{:x}",hash.finalize()) != *expected_digest {
                return Err("An installed package file changed or failed its checksum. No package files were removed.".into());
            }
            found.insert(name);
        }
    }
    if found.len()!=expected.len() {return Err("The installed package is incomplete. No files were removed.".into());}
    let mut package_path=root;
    let mut files=found.len()+1;
    let mut bytes=total+raw.len() as u64;
    let mut digest=format!("{:x}",Sha256::digest(&raw));
    if let Some(app)=super::browser_package::enclosing(executable)? {
        safe_ancestry(&app)?;owned_metadata(&app,true)?;
        if overlaps(&record.data_dir,&app)?||overlaps(state,&app)?{return Err("The browser app overlaps saved data. Move the app outside the library before uninstalling.".into());}
        let (outer_bytes,outer_files,outer_digest)=super::browser_package::verify_layout(&app)?;
        bytes+=outer_bytes;files+=outer_files;
        digest=format!("{:x}",Sha256::digest(format!("{digest}:{outer_digest}").as_bytes()));
        package_path=app;
    }
    Ok(Verified {plan: Plan {package_path, library_path:record.data_dir.clone(), launch_state_path:state.to_path_buf(),
        files, bytes, preserves_library:true, preserves_models:true,
        preserves_originals:true, preserves_settings:true, removes_only_verified_package:true}, digest})
}

async fn helper(executable: &Path, operation: &str, input: serde_json::Value, timeout: Duration) -> Result<serde_json::Value> {
    let result_alert = matches!(operation,"--trash-package"|"--uninstall-error");
    let mut child = launch::desktop_command(&launch::browser_opener(executable)?).arg(operation)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(!result_alert)
        .spawn().map_err(|_|"The installed native uninstall helper could not start.")?;
    let result = async {
        let mut stdin = child.stdin.take().ok_or("Cannot open the native uninstall request.")?;
        let bytes = serde_json::to_vec(&input).map_err(|_|"Cannot encode the native uninstall request.")?;
        stdin.write_all(&bytes).await.map_err(|_|"Cannot deliver the native uninstall request.")?;
        stdin.shutdown().await.map_err(|_|"Cannot finish the native uninstall request.")?;
        drop(stdin);
        let stdout = child.stdout.take().ok_or("Cannot read the native uninstall response.")?;
        let mut bytes = Vec::new(); stdout.take(8193).read_to_end(&mut bytes).await.map_err(|_|"Cannot read the native uninstall response.")?;
        if bytes.len()>8192 {return Err("The native uninstall response exceeded its limit.".into());}
        let value:serde_json::Value = serde_json::from_slice(&bytes).map_err(|_|"The native uninstall helper returned an invalid response.")?;
        // The fixed helper closes stdout before its result-only native alert.
        // Persist completion now; the user can dismiss that alert at any time.
        if result_alert && (value.get("trashed").and_then(serde_json::Value::as_bool).is_some() || value==serde_json::json!({"shown":true})) {return Ok(value);}
        let status = child.wait().await.map_err(|_|"Cannot finish the native uninstall helper.")?;
        if !status.success() {return Err("The native uninstall helper did not complete. Your library is preserved; retry this package's uninstall command.".into());}
        Ok(value)
    };
    let completed = tokio::time::timeout(timeout,result).await.map_err(|_|"The native uninstall helper timed out; your library is preserved.".to_owned()).and_then(|v|v);
    if completed.is_err() {let _ = child.kill().await;let _ = child.wait().await;}
    completed
}
fn read_receipt(state: &Path) -> Result<Option<Receipt>> {
    launch::read_uninstall_receipt(state)?.map(|value|serde_json::from_value(value).map_err(|_|"Uninstall recovery state is incompatible.".into())).transpose()
}
fn registered_or_pending(state: &Path, executable: &Path) -> Result<(Registration, Option<Receipt>)> {
    let receipt = read_receipt(state)?.filter(|r|r.status != "complete");
    if let Some(receipt) = receipt {
        if receipt.version != 1 || receipt.registration.executable != executable || !matches!(receipt.status.as_str(),"confirmed"|"revoked"|"stopped"|"unregistered") {
            return Err("A different or incompatible uninstall is pending for this account.".into());
        }
        if fs::symlink_metadata(state.join(launch::REGISTRATION)).is_ok()
            && launch::read_registration(state)? != receipt.registration {
            return Err("The registration changed after uninstall began. No package files were removed.".into());
        }
        Ok((receipt.registration.clone(),Some(receipt)))
    } else {Ok((launch::read_registration(state)?,None))}
}
fn try_library_guard(data: &Path) -> Result<Option<Vec<File>>> {
    let directory = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC)
        .open(data).map_err(|_|"Cannot establish that the library has stopped.")?;
    let mut files = vec![directory];
    let host = data.join(".companion.lock");
    match fs::symlink_metadata(&host) {
        Ok(_) => files.push(launch::private_file(&host,false)?),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound => {},
        Err(_) => return Err("Cannot establish that the companion has stopped.".into()),
    }
    for file in &files {
        if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0 {
            if std::io::Error::last_os_error().kind()==std::io::ErrorKind::WouldBlock {return Ok(None);}
            return Err("Cannot check local process ownership before uninstall.".into());
        }
    }
    Ok(Some(files))
}
async fn unregister(root: &Path, state: &Path) -> Result<()> {
    if state != launch::default_directory()? {return Ok(());}
    let nested=root.join("Contents/Resources/companion");
    let program=if nested.is_dir(){nested}else{root.to_path_buf()};
    let mut bundles=["CompanionPicker","CompanionDownloads","CompanionLabels","CompanionLauncher"].map(|name|program.join("bin").join(format!("{name}.app"))).to_vec();
    if program!=root{bundles.push(root.to_path_buf());}
    for bundle in bundles {
        // Fixed exact package paths only; never unregister a bundle ID or old app.
        if !bundle.is_dir() {continue;}
        let status = tokio::time::timeout(Duration::from_secs(15),tokio::process::Command::new(
            "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .arg("-u").arg(bundle).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .kill_on_drop(true).status()).await.map_err(|_|"macOS launcher cleanup timed out. Retry uninstall.")?
            .map_err(|_|"Cannot unregister this package's native launchers. Retry uninstall.")?;
        if !status.success() {return Err("macOS could not unregister this package's native launchers. Retry uninstall.".into());}
    }
    Ok(())
}

/// Read-only UI preview. Full file checksums are verified by the explicit
/// native/CLI uninstall command, never on each browser settings refresh.
pub fn plan(state: &Path, executable: &Path) -> Result<Plan> {
    let owner = launch::private_directory(state,false)?;
    let (record,pending) = registered_or_pending(state,executable)?;
    let verified = verify(&record,executable,state,false)?;
    if pending.as_ref().is_some_and(|receipt|receipt.manifest_sha256!=verified.digest) {
        return Err("The installed package changed since uninstall began.".into());
    }
    launch::same_directory(state,&owner)?;
    Ok(verified.plan)
}

/// Every packaged host holds this shared directory lease until all owned worker
/// processes drain. Uninstall holds its exclusive counterpart before moving files.
/// Developer executables do not acquire a package lease.
pub fn runtime_lease(executable: &Path, runtime: &Path) -> Result<Option<Vec<File>>> {
    let Some(bin) = executable.parent() else {return Ok(None)};
    let Some(root) = bin.parent() else {return Ok(None)};
    if executable.file_name().is_none_or(|v|v!="photofindry") || bin.file_name().is_none_or(|v|v!="bin")
        || runtime != root.join("runtime") {return Ok(None);}
    safe_ancestry(executable)?;let identity=owned_metadata(root,true)?;
    open_file(&root.join(MANIFEST))?;
    let file = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC)
        .open(root).map_err(|_|"Cannot acquire the installed runtime lease.")?;
    if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_SH|libc::LOCK_NB)}!=0 {
        return Err("This PhotoFindry package is being uninstalled. Finish uninstall or register a different package.".into());
    }
    if !same_file(&identity,&file.metadata().map_err(|_|"Cannot recheck the installed runtime lease.")?)
        || !same_file(&identity,&owned_metadata(root,true)?) {return Err("This PhotoFindry package moved during startup.".into());}
    let mut leases=vec![file];
    if let Some(app)=super::browser_package::enclosing(executable)? {
        let outer=OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(app).map_err(|_|"Cannot acquire the browser app runtime lease.")?;
        if unsafe{libc::flock(outer.as_raw_fd(),libc::LOCK_SH|libc::LOCK_NB)}!=0{return Err("The browser app is being updated or removed. Retry after it finishes.".into());}
        leases.push(outer);
    }
    Ok(Some(leases))
}

/// Update preparation acquires these only after the broker and worker drain.
/// No PID, executable, or directory can come from a website request.
pub fn try_update_runtime(executable:&Path)->Result<Option<Vec<File>>> {
    let Some(bin)=executable.parent()else{return Ok(Some(Vec::new()))};
    let Some(root)=bin.parent()else{return Ok(Some(Vec::new()))};
    if bin.file_name().is_none_or(|name|name!="bin"){return Ok(Some(Vec::new()));}
    let mut roots=vec![root.to_path_buf()];
    if let Some(app)=super::browser_package::enclosing(executable)? {roots.push(app);}
    let mut guards=Vec::new();
    for root in roots {
        safe_ancestry(&root)?;
        let file=OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(root).map_err(|_|"Cannot verify that this app's runtime has stopped.")?;
        if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0 {
            if std::io::Error::last_os_error().kind()==std::io::ErrorKind::WouldBlock{return Ok(None);}
            return Err("Cannot establish exclusive app runtime ownership before updating.".into());
        }
        guards.push(file);
    }
    Ok(Some(guards))
}

pub async fn run(state: &Path, executable: &Path, mode: Mode) -> Result<Outcome> {
    let preview = if mode==Mode::Interactive {plan(state,executable).ok()} else {None};
    let result = execute(state,executable,mode).await;
    if let (Err(error),Some(plan))=(&result,preview) {
        // The browser host may already have stopped. Keep recovery feedback in
        // the installed native helper; an error alert never changes any files.
        if plan.package_path.is_dir() && !error.starts_with("macOS could not move the package to Trash.") {
            let _=helper(executable,"--uninstall-error",serde_json::json!({"packagePath":plan.package_path,
                "libraryPath":plan.library_path,"message":error}),Duration::from_secs(30)).await;
        }
    }
    result
}
async fn execute(state: &Path, executable: &Path, mode: Mode) -> Result<Outcome> {
    // Dry-run is strictly read-only: it creates no lock, receipt or directory.
    let owner = launch::private_directory(state,false)?;
    let (record, pending) = registered_or_pending(state,executable)?;
    let verified = verify(&record,executable,state,true)?;
    if let Some(receipt) = &pending {if receipt.manifest_sha256 != verified.digest {return Err("The package changed since uninstall began. No files were removed.".into());}}
    launch::same_directory(state,&owner)?;
    if mode==Mode::Plan {return Ok(Outcome {status:"planned",plan:verified.plan,trashed_path:None});}
    let guard = launch::LifecycleGuard::acquire(state)?;
    let (current, current_pending) = registered_or_pending(state,executable)?;
    if current != record || current_pending.as_ref().map(|r|&r.manifest_sha256) != pending.as_ref().map(|r|&r.manifest_sha256) {
        return Err("Local registration changed while planning uninstall. Retry from the installed package.".into());
    }
    if mode==Mode::Interactive {
        let result = helper(executable,"--confirm-uninstall",serde_json::json!({"packagePath":verified.plan.package_path,"libraryPath":record.data_dir}),Duration::from_secs(600)).await?;
        if result == serde_json::json!({"confirmed":false}) {return Ok(Outcome {status:"cancelled",plan:verified.plan,trashed_path:None});}
        if result != serde_json::json!({"confirmed":true}) {return Err("Native uninstall confirmation was invalid; no changes were made.".into());}
    }
    // A native transfer may refuse shutdown. Do this before durable revocation
    // so declining a busy uninstall does not strand the working registration.
    launch::request_stop(state).await?;
    let mut receipt = pending.unwrap_or(Receipt {version:1,status:"confirmed".into(),registration:record.clone(),manifest_sha256:verified.digest.clone(),trashed_path:None});
    launch::write_uninstall_receipt(state,&receipt)?;
    if fs::symlink_metadata(state.join(launch::REGISTRATION)).is_ok() {
        if launch::read_registration(state)? != record {return Err("The local registration changed before revocation.".into());}
        launch::same_directory(state,&guard.owner)?;
        fs::remove_file(state.join(launch::REGISTRATION)).map_err(|_|"Cannot revoke local launch registration. Retry uninstall.")?;
        guard.owner.sync_all().map_err(|_|"Cannot flush local launch revocation. Retry uninstall.")?;
    }
    receipt.status="revoked".into();launch::write_uninstall_receipt(state,&receipt)?;
    let deadline = tokio::time::Instant::now()+Duration::from_secs(60);
    let package = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC)
        .open(&verified.plan.package_path).map_err(|_|"Cannot establish package runtime ownership.")?;
    let (_broker, _library) = loop {
        if let Some(broker) = launch::try_lock(state,"broker.lock")? {
            if let Some(library) = try_library_guard(&record.data_dir)? {
                if unsafe{libc::flock(package.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0 {break (broker,library);}
                if std::io::Error::last_os_error().kind()!=std::io::ErrorKind::WouldBlock {return Err("Cannot lock package runtime ownership.".into());}
            }
        }
        if tokio::time::Instant::now()>=deadline {return Err("The package is still used by a companion or library. Quit PhotoFindry and retry uninstall; nothing in the package or library was deleted.".into());}
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    receipt.status="stopped".into();launch::write_uninstall_receipt(state,&receipt)?;
    super::lifecycle::disable_for_uninstall(executable).await?;
    // Recheck all contents after shutdown and immediately before the Trash move.
    let final_verified = verify(&record,executable,state,true)?;
    if final_verified.digest != verified.digest {return Err("The package changed during uninstall. No package files were removed.".into());}
    unregister(&verified.plan.package_path,state).await?;
    receipt.status="unregistered".into();launch::write_uninstall_receipt(state,&receipt)?;
    let result = helper(executable,"--trash-package",serde_json::json!({"packagePath":verified.plan.package_path,"libraryPath":record.data_dir,"interactive":mode==Mode::Interactive}),Duration::from_secs(600)).await?;
    if result == serde_json::json!({"trashed":false}) {return Err("macOS could not move the package to Trash. Your library is preserved; retry this package's uninstall command.".into());}
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Trash {trashed:bool,path:PathBuf}
    let trash:Trash = serde_json::from_value(result).map_err(|_|"The native helper did not return a valid Trash receipt. Your library is preserved; inspect Trash before retrying.")?;
    if !trash.trashed || !trash.path.is_absolute() || verified.plan.package_path.exists() {
        return Err("The package Trash move was not confirmed. Your library is preserved; inspect Trash before retrying.".into());
    }
    receipt.status="complete".into(); receipt.trashed_path=Some(trash.path.clone());
    launch::write_uninstall_receipt(state,&receipt)?;
    super::lifecycle::finish_uninstall(executable)?;
    Ok(Outcome {status:"uninstalled",plan:verified.plan,trashed_path:Some(trash.path)})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    struct Fixture {root:PathBuf, state:PathBuf, record:Registration}
    impl Fixture {
        fn new(confirm:bool) -> Self {
            let mut random=[0u8;8];getrandom::fill(&mut random).unwrap();
            let suffix:String=random.iter().map(|v|format!("{v:02x}")).collect();
            let root=std::env::temp_dir().canonicalize().unwrap().join(format!("pfu-{suffix}"));
            fs::create_dir(&root).unwrap();fs::set_permissions(&root,fs::Permissions::from_mode(0o700)).unwrap();
            let state=root.join("s");fs::create_dir(&state).unwrap();fs::set_permissions(&state,fs::Permissions::from_mode(0o700)).unwrap();
            let package=root.join("PhotoFindry");
            let data=root.join("library");fs::create_dir(&data).unwrap();
            fs::write(data.join("catalog.sqlite"),b"preserved library bytes").unwrap();
            fs::write(data.join("settings.json"),b"preserved model settings").unwrap();
            let record=Registration{version:1,data_dir:data,runtime:package.join("runtime"),executable:package.join("bin/photofindry")};
            for path in ["bin/photofindry","bin/CompanionLauncher","bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher",
                "bin/CompanionLauncher.app/Contents/Info.plist","runtime/PhotoWorker","runtime/PhotoEngine","runtime/engine-manifest.json","LICENSE","README.txt"] {
                let path=package.join(path);fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path,b"packaged fixture bytes").unwrap();fs::set_permissions(path,fs::Permissions::from_mode(0o700)).unwrap();
            }
            let script=format!("#!/bin/sh\ncase \"$1\" in\n--confirm-uninstall) cat >/dev/null; printf '{{\"confirmed\":{confirm}}}' ;;\n--trash-package) cat >/dev/null; /bin/mv '{}' '{}'; printf '{{\"trashed\":true,\"path\":\"{}\"}}' ;;\n*) exit 2 ;;\nesac\n",package.display(),root.join("Trash").display(),root.join("Trash").display());
            fs::write(package.join("bin/CompanionLauncher"),script).unwrap();
            let fixture=Self{root,state,record};fixture.register();fixture.seal();fixture
        }
        fn package(&self)->PathBuf {self.record.runtime.parent().unwrap().to_path_buf()}
        fn register(&self) {
            let path=self.state.join(launch::REGISTRATION);fs::write(&path,serde_json::to_vec(&self.record).unwrap()).unwrap();
            fs::set_permissions(path,fs::Permissions::from_mode(0o600)).unwrap();
        }
        fn seal(&self) {
            let mut pending=vec![self.package()];let mut rows=Vec::new();
            while let Some(dir)=pending.pop(){for entry in fs::read_dir(dir).unwrap(){let path=entry.unwrap().path();if path.is_dir(){pending.push(path);}else if path.file_name().unwrap()!=MANIFEST {rows.push(format!("{:x}  {}\n",Sha256::digest(fs::read(&path).unwrap()),path.strip_prefix(self.package()).unwrap().display()));}}}
            rows.sort();fs::write(self.package().join(MANIFEST),rows.concat()).unwrap();
        }
        fn plan(&self)->Result<Plan>{plan(&self.state,&self.record.executable)}
        fn assert_data(&self){assert_eq!(fs::read(self.record.data_dir.join("catalog.sqlite")).unwrap(),b"preserved library bytes");assert_eq!(fs::read(self.record.data_dir.join("settings.json")).unwrap(),b"preserved model settings");}
    }
    impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}

    #[tokio::test]
    async fn plan_and_cancel_preserve_registration_package_and_library() {
        let f=Fixture::new(false);let registry=fs::read(f.state.join(launch::REGISTRATION)).unwrap();
        assert_eq!(fs::read_dir(&f.state).unwrap().count(),1);
        assert!(f.plan().unwrap().preserves_library);
        assert_eq!(run(&f.state,&f.record.executable,Mode::Plan).await.unwrap().status,"planned");
        assert_eq!(fs::read_dir(&f.state).unwrap().count(),1);
        assert_eq!(run(&f.state,&f.record.executable,Mode::Interactive).await.unwrap().status,"cancelled");
        assert_eq!(fs::read(f.state.join(launch::REGISTRATION)).unwrap(),registry);
        assert!(!f.state.join("uninstall.json").exists());assert!(f.package().is_dir());f.assert_data();
    }
    #[test]
    fn rejects_unknown_files_empty_directories_links_and_modified_payload() {
        let f=Fixture::new(false);
        fs::write(f.package().join("my-photos.jpg"),b"keep me").unwrap();assert!(f.plan().is_err());fs::remove_file(f.package().join("my-photos.jpg")).unwrap();
        fs::create_dir(f.package().join("My Library")).unwrap();assert!(f.plan().is_err());fs::remove_dir(f.package().join("My Library")).unwrap();
        let file=f.package().join("README.txt");let bytes=fs::read(&file).unwrap();fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(f.record.data_dir.join("catalog.sqlite"),&file).unwrap();assert!(f.plan().is_err());fs::remove_file(&file).unwrap();
        fs::hard_link(f.record.data_dir.join("catalog.sqlite"),&file).unwrap();assert!(f.plan().is_err());fs::remove_file(&file).unwrap();
        fs::write(&file,&bytes).unwrap();assert!(f.plan().is_ok());fs::write(&file,b"tampered").unwrap();
        assert!(verify(&f.record,&f.record.executable,&f.state,true).is_err());f.assert_data();
    }
    #[test]
    fn refuses_data_overlap_developer_executables_and_foreign_registration() {
        let mut f=Fixture::new(false);let original=f.record.clone();
        f.record.data_dir=f.package().join("runtime");f.register();assert!(f.plan().unwrap_err().contains("overlap"));
        f.record=original.clone();f.record.runtime=f.record.data_dir.clone();f.register();assert!(f.plan().is_err());
        f.record=original;f.register();assert!(plan(&f.state,&f.root.join("other-executable")).is_err());
        let alias=f.root.join("alias");std::os::unix::fs::symlink(f.package(),&alias).unwrap();
        assert!(plan(&f.state,&alias.join("bin/photofindry")).is_err());
    }
    #[test]
    fn rejects_case_alias_overlap_on_case_insensitive_volumes() {
        let mut f=Fixture::new(false);
        let alias=f.root.join("photofindry/runtime");
        if alias.exists() {
            f.record.data_dir=alias;f.register();
            assert!(f.plan().is_err());
            assert!(verify(&f.record,&f.record.executable,&f.state,false).unwrap_err().contains("overlap"));
        }
    }
    #[test]
    fn manifest_paths_and_recovery_ownership_are_strict() {
        let f=Fixture::new(false);
        for value in ["/absolute", "../README.txt", "bin/../README.txt", "bin/./photofindry", "bin//photofindry", ".git/config", "bin/", "bin\\photofindry", "bin/\nphotofindry"] {
            assert!(relative(value).is_err(),"{value}");
        }
        let mut other=f.record.clone();other.executable=f.root.join("other/bin/photofindry");
        launch::write_uninstall_receipt(&f.state,&Receipt{version:1,status:"revoked".into(),registration:other,manifest_sha256:"a".repeat(64),trashed_path:None}).unwrap();
        assert!(f.plan().is_err());assert_eq!(launch::read_registration(&f.state).unwrap(),f.record);f.assert_data();
    }
    #[tokio::test]
    async fn confirmed_uninstall_revokes_only_matching_registration_and_trashes_package() {
        let f=Fixture::new(true);fs::write(f.state.join("support.json"),b"preserved host preferences").unwrap();
        let result=run(&f.state,&f.record.executable,Mode::Confirm).await.unwrap();
        assert_eq!(result.status,"uninstalled");assert!(!f.package().exists());assert!(f.root.join("Trash/bin/photofindry").exists());
        assert!(!f.state.join(launch::REGISTRATION).exists());
        assert_eq!(fs::read(f.state.join("support.json")).unwrap(),b"preserved host preferences");
        assert_eq!(read_receipt(&f.state).unwrap().unwrap().status,"complete");f.assert_data();
    }
    #[tokio::test]
    async fn busy_native_transfer_refuses_before_revocation_and_then_can_retry() {
        let f=Fixture::new(true);
        let broker=launch::Broker::start_with_shutdown(&f.state,&f.record,23456,||Ok("a".repeat(64)),||Err("A backup is still running.".into())).unwrap();
        assert!(run(&f.state,&f.record.executable,Mode::Confirm).await.unwrap_err().contains("backup"));
        assert_eq!(launch::read_registration(&f.state).unwrap(),f.record);
        assert!(!f.state.join("uninstall.json").exists());assert!(f.package().exists());
        broker.stop().await;
        assert_eq!(run(&f.state,&f.record.executable,Mode::Confirm).await.unwrap().status,"uninstalled");f.assert_data();
    }
    #[test]
    fn package_leases_preserve_other_live_library_hosts() {
        let f=Fixture::new(true);
        let a=runtime_lease(&f.record.executable,&f.record.runtime).unwrap().unwrap();
        let b=runtime_lease(&f.record.executable,&f.record.runtime).unwrap().unwrap();
        let exclusive=File::open(f.package()).unwrap();
        assert_ne!(unsafe{libc::flock(exclusive.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
        drop(a);assert_ne!(unsafe{libc::flock(exclusive.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
        drop(b);assert_eq!(unsafe{libc::flock(exclusive.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);
        assert!(runtime_lease(&f.record.executable,&f.record.runtime).is_err());
    }
    #[tokio::test]
    async fn pending_uninstall_blocks_relaunch_and_resumes_with_same_package() {
        let f=Fixture::new(true);let v=verify(&f.record,&f.record.executable,&f.state,true).unwrap();
        let receipt=Receipt{version:1,status:"revoked".into(),registration:f.record.clone(),manifest_sha256:v.digest,trashed_path:None};
        launch::write_uninstall_receipt(&f.state,&receipt).unwrap();fs::remove_file(f.state.join(launch::REGISTRATION)).unwrap();
        assert!(launch::ensure_available(&f.state).is_err());assert!(launch::open_registered(&f.state).await.is_err());
        assert!(f.plan().is_ok());assert_eq!(run(&f.state,&f.record.executable,Mode::Confirm).await.unwrap().status,"uninstalled");f.assert_data();
    }
    #[tokio::test]
    async fn lifecycle_guard_serializes_registration_launch_and_uninstall() {
        let f=Fixture::new(true);let guard=launch::LifecycleGuard::acquire(&f.state).unwrap();
        assert!(run(&f.state,&f.record.executable,Mode::Confirm).await.unwrap_err().contains("progress"));
        assert!(launch::register(&f.state,&f.record.data_dir,&f.record.runtime,&f.record.executable).is_err());
        assert_eq!(launch::read_registration(&f.state).unwrap(),f.record);assert!(!f.state.join("uninstall.json").exists());
        drop(guard);f.assert_data();
    }
}
