//! Linux desktop selections become typed, short-lived capabilities. Browser
//! callers never submit filesystem paths or chooser arguments.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, AsyncReadExt};
const KINDS: &[&str] = &[
    "folder",
    "sourceFolder",
    "model",
    "projector",
    "reviewModel",
    "reviewProjector",
    "template",
    "reranker",
    "backupOpen",
    "backupSave",
];
const BACKUP: &str = ".photofindrybackup";
const LEGACY_BACKUP: &str = ".photorecallbackup";
struct Grant {
    path: PathBuf,
    kind: String,
    expires: Instant,
    identity: (u64, u64, u64, i64, i64),
}
pub struct Picker {
    grants: Mutex<HashMap<String, Grant>>,
    active: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}
/// A portable backup is a folder package; a save destination is a new name
/// inside an existing folder, so its grant binds that parent folder instead.
fn backup_package(path: &Path, legacy: bool) -> bool {
    path.file_name().and_then(|v| v.to_str()).is_some_and(|name| {
        (name.len() > BACKUP.len() && name.ends_with(BACKUP))
            || (legacy && name.len() > LEGACY_BACKUP.len() && name.ends_with(LEGACY_BACKUP))
    })
}
fn identity(path: &Path, kind: &str) -> Result<(u64, u64, u64, i64, i64), String> {
    if kind == "backupSave" {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err("The backup destination already exists. Choose a new name.".into());
        }
        return identity(path.parent().ok_or("Invalid backup destination.")?, "folder");
    }
    let folder = ["folder", "backupOpen"].contains(&kind);
    let m = std::fs::metadata(path).map_err(|_| "The selected location is unavailable.")?;
    if folder != m.is_dir() || (!folder && !m.is_file()) {
        return Err("The selected location has the wrong type.".into());
    }
    if kind == "backupOpen" && !std::fs::symlink_metadata(path.join("manifest.json")).is_ok_and(|v| v.is_file()) {
        return Err("Choose the whole .photofindrybackup folder exported by PhotoFindry.".into());
    }
    Ok((
        m.dev(),
        m.ino(),
        if folder { 0 } else { m.len() },
        if folder { 0 } else { m.mtime() },
        if folder { 0 } else { m.mtime_nsec() },
    ))
}
async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut bytes = Vec::new();
    let mut overflow = false;
    let mut chunk = [0; 4096];
    loop {
        let n = reader
            .read(&mut chunk)
            .await
            .map_err(|_| "The desktop file chooser disconnected.")?;
        if n == 0 {
            break;
        }
        let keep = n.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..keep]);
        overflow |= keep < n;
    }
    Ok((bytes, overflow))
}
impl Picker {
    pub fn new(_runtime: &Path) -> Self {
        Self {
            grants: Mutex::new(HashMap::new()),
            active: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }
    pub fn revoke(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.grants.lock().unwrap().clear();
    }
    pub fn grant_path(&self, kind: &str, raw: &str) -> Result<Value, String> {
        self.grant_at(kind, raw, self.generation.load(Ordering::Acquire))
    }
    fn grant_at(&self, kind: &str, raw: &str, generation: u64) -> Result<Value, String> {
        if !KINDS.contains(&kind)
            || raw.contains('\0')
            || raw.len() > 16384
            || !Path::new(raw).is_absolute()
        {
            return Err("Choose an absolute local model file or originals folder.".into());
        }
        let kind = match kind {
            "sourceFolder" => "folder",
            "reviewModel" => "model",
            "reviewProjector" => "projector",
            other => other,
        };
        let path = if kind == "backupSave" {
            let raw = Path::new(raw);
            let name = raw.file_name().and_then(|v| v.to_str()).ok_or("Choose a backup name.")?;
            let name = if name.ends_with(BACKUP) { name.to_owned() } else { format!("{name}{BACKUP}") };
            std::fs::canonicalize(raw.parent().ok_or("Invalid backup destination.")?)
                .map_err(|_| "The destination folder is unavailable.")?
                .join(name)
        } else {
            std::fs::canonicalize(raw).map_err(|_| "The selected location is unavailable.")?
        };
        if kind == "backupOpen" && !backup_package(&path, true) {
            return Err("Choose a .photofindrybackup or legacy .photorecallbackup folder.".into());
        }
        let recorded = identity(&path, kind)?;
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|_| "The system random source is unavailable.")?;
        let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let mut grants = self.grants.lock().unwrap();
        grants.retain(|_, g| g.expires > Instant::now());
        if grants.len() >= 256 {
            return Err("Too many pending selections. Reconnect to clear them.".into());
        }
        if generation != self.generation.load(Ordering::Acquire) {
            return Err("The local session changed. Choose the location again.".into());
        }
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Selected folder".into());
        let result = json!({"grantId":id,"label":label,"kind":kind,"cancelled":false});
        grants.insert(
            id,
            Grant {
                path,
                kind: kind.into(),
                expires: Instant::now() + Duration::from_secs(1800),
                identity: recorded,
            },
        );
        Ok(result)
    }
    pub fn resolve(&self, id: &str, kind: &str) -> Result<String, String> {
        let grants = self.grants.lock().unwrap();
        let grant = grants
            .get(id)
            .filter(|g| g.kind == kind && g.expires > Instant::now())
            .ok_or("Choose this file or folder again; its selection expired.")?;
        if identity(&grant.path, kind)? != grant.identity {
            return Err("The selected file changed. Choose it again.".into());
        }
        Ok(grant.path.to_string_lossy().into_owned())
    }
    pub fn resolve_source_access(&self, id: &str) -> Result<Option<Value>, String> {
        self.resolve(id, "folder")?;
        Ok(None)
    }
    pub async fn pick(&self, kind: &str) -> Result<Value, String> {
        if !KINDS.contains(&kind) {
            return Err(
                "This selection is not yet supported by the Linux development build.".into(),
            );
        }
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err("A Linux desktop session is required for the chooser. In the companion terminal use grant KIND /absolute/path, then use its returned selection ID.".into());
        }
        let _active = self
            .active
            .try_lock()
            .map_err(|_| "Finish the open file chooser first.")?;
        let generation = self.generation.load(Ordering::Acquire);
        let mut command = tokio::process::Command::new("/usr/bin/zenity");
        command.args(["--file-selection", "--title=Choose for PhotoFindry"]);
        if ["folder", "sourceFolder", "backupOpen"].contains(&kind) {
            command.arg("--directory");
        }
        if kind == "backupSave" {
            command.args(["--save", "--filename=PhotoFindry Library.photofindrybackup"]);
        }
        let mut child=command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().map_err(|_|"Install zenity for the Linux desktop chooser, or use the companion terminal's grant command.")?;
        let output = child.stdout.take().ok_or("The chooser could not start.")?;
        let errors = child.stderr.take().ok_or("The chooser could not start.")?;
        let task = async {
            let (out, err) = tokio::join!(read_bounded(output, 16385), read_bounded(errors, 4096));
            let (out, large) = out?;
            let _ = err?;
            let status = child
                .wait()
                .await
                .map_err(|_| "The chooser stopped unexpectedly.")?;
            if status.code() == Some(1) {
                return Ok(json!({"cancelled":true}));
            }
            if !status.success() || large {
                return Err(
                    "The Linux desktop chooser did not return a bounded selection.".to_owned(),
                );
            }
            let raw = std::str::from_utf8(&out)
                .map_err(|_| "The selected path is not valid UTF-8.")?
                .strip_suffix('\n')
                .unwrap_or(std::str::from_utf8(&out).unwrap());
            self.grant_at(kind, raw, generation)
        };
        match tokio::time::timeout(Duration::from_secs(300), task).await {
            Ok(result) => result,
            Err(_) => {
                let _ = child.kill().await;
                Err("The desktop file chooser timed out. Choose again.".into())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_grants_reject_substitution_and_revoke() {
        let path = std::env::temp_dir().join(format!("pf-linux-picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        let file = path.join("model.gguf");
        std::fs::write(&file, b"first").unwrap();
        let p = Picker::new(&path);
        let grant = p.grant_path("model", file.to_str().unwrap()).unwrap();
        let id = grant["grantId"].as_str().unwrap();
        assert!(!grant.to_string().contains(path.to_str().unwrap()));
        assert!(p.resolve(id, "projector").is_err());
        assert!(p.resolve(id, "model").is_ok());
        std::fs::write(&file, b"different bytes").unwrap();
        assert!(p.resolve(id, "model").is_err());
        let folder = p.grant_path("folder", path.to_str().unwrap()).unwrap();
        let id = folder["grantId"].as_str().unwrap();
        std::fs::write(path.join("new"), b"new").unwrap();
        assert!(p.resolve(id, "folder").is_ok());
        p.revoke();
        assert!(p.resolve(id, "folder").is_err());
        assert!(p.grant_path("folder", "relative").is_err());
        assert!(p.grant_path("unknown", path.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn backup_grants_bind_packages_and_new_destinations() {
        let path = std::env::temp_dir().join(format!("pf-linux-backup-picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let package = path.join("Library.photofindrybackup");
        std::fs::create_dir(&package).unwrap();
        let p = Picker::new(&path);
        assert!(p.grant_path("backupOpen", package.to_str().unwrap()).is_err(), "a package needs its manifest");
        std::fs::write(package.join("manifest.json"), b"{}").unwrap();
        let open = p.grant_path("backupOpen", package.to_str().unwrap()).unwrap();
        let id = open["grantId"].as_str().unwrap();
        assert_eq!(p.resolve(id, "backupOpen").unwrap(), package.to_str().unwrap());
        assert!(p.resolve(id, "folder").is_err() && p.resolve(id, "backupSave").is_err());
        let legacy = path.join("Old.photorecallbackup");
        std::fs::create_dir(&legacy).unwrap();
        std::fs::write(legacy.join("manifest.json"), b"{}").unwrap();
        assert!(p.grant_path("backupOpen", legacy.to_str().unwrap()).is_ok());
        std::fs::create_dir(path.join("plain")).unwrap();
        std::fs::write(path.join("plain/manifest.json"), b"{}").unwrap();
        assert!(p.grant_path("backupOpen", path.join("plain").to_str().unwrap()).is_err());
        assert!(p.grant_path("backupOpen", package.join("manifest.json").to_str().unwrap()).is_err());
        let save = p.grant_path("backupSave", path.join("Export").to_str().unwrap()).unwrap();
        let id = save["grantId"].as_str().unwrap();
        let destination = p.resolve(id, "backupSave").unwrap();
        assert_eq!(destination, path.join("Export.photofindrybackup").to_str().unwrap());
        std::fs::create_dir(&destination).unwrap();
        assert!(p.resolve(id, "backupSave").is_err(), "a destination that appeared is refused");
        assert!(p.grant_path("backupSave", package.to_str().unwrap()).is_err());
        assert!(p.grant_path("backupSave", path.join("missing/Export").to_str().unwrap()).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}
