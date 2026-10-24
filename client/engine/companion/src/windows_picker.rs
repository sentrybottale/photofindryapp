//! Typed native Windows selections. No browser path or executable arguments.
use photofindry_platform::fs;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
const KINDS: &[&str] = &[
    "folder",
    "sourceFolder",
    "model",
    "projector",
    "reviewModel",
    "reviewProjector",
    "template",
    "reranker",
];
fn open_selection(path: &Path, kind: &str) -> std::io::Result<fs::File> {
    if kind == "folder" {
        fs::open_read_directory_no_reparse(path)
    } else {
        fs::open_read_no_reparse(path)
    }
}
struct Grant {
    path: PathBuf,
    kind: String,
    expires: Instant,
    identity: fs::Identity,
}
pub struct Picker {
    grants: Mutex<HashMap<String, Grant>>,
    active: Arc<AtomicBool>,
    generation: AtomicU64,
}
impl Picker {
    pub fn new(_runtime: &Path) -> Self {
        Self {
            grants: Mutex::new(HashMap::new()),
            active: Arc::new(AtomicBool::new(false)),
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
            || raw.len() > 32768
            || !Path::new(raw).is_absolute()
        {
            return Err("Choose an absolute Windows file or originals folder.".into());
        }
        let kind = match kind {
            "sourceFolder" => "folder",
            "reviewModel" => "model",
            "reviewProjector" => "projector",
            other => other,
        };
        let path =
            std::fs::canonicalize(raw).map_err(|_| "The selected location is unavailable.")?;
        let file = open_selection(&path, kind)
            .map_err(|_| "The selected location cannot be opened safely.")?;
        let identity =
            fs::identity(&file).map_err(|_| "The selected file identity is unavailable.")?;
        let folder = identity.attributes & 0x10 != 0;
        if folder != (kind == "folder") {
            return Err("The selected location has the wrong type.".into());
        }
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| "The Windows random source is unavailable.")?;
        let id: String = bytes.iter().map(|v| format!("{v:02x}")).collect();
        let label = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("Selected folder");
        let mut grants = self.grants.lock().unwrap();
        grants.retain(|_, g| g.expires > Instant::now());
        if grants.len() >= 256 {
            return Err("Too many pending selections. Reconnect to clear them.".into());
        }
        if generation != self.generation.load(Ordering::Acquire) {
            return Err("The local session changed; choose the location again.".into());
        }
        let result = json!({"grantId":id,"label":label,"kind":kind,"cancelled":false});
        grants.insert(
            id,
            Grant {
                path,
                kind: kind.into(),
                expires: Instant::now() + Duration::from_secs(1800),
                identity,
            },
        );
        Ok(result)
    }
    pub fn resolve(&self, id: &str, kind: &str) -> Result<String, String> {
        let grants = self.grants.lock().unwrap();
        let old = grants
            .get(id)
            .filter(|g| g.kind == kind && g.expires > Instant::now())
            .ok_or("Choose this location again; its selection expired.")?;
        let file =
            open_selection(&old.path, kind).map_err(|_| "The selected location is unavailable.")?;
        let now =
            fs::identity(&file).map_err(|_| "The selected location identity is unavailable.")?;
        if now.volume != old.identity.volume
            || now.file_id != old.identity.file_id
            || (kind != "folder"
                && (now.size != old.identity.size
                    || now.modified_100ns != old.identity.modified_100ns
                    || now.changed_100ns != old.identity.changed_100ns))
        {
            return Err("The selected location changed. Choose it again.".into());
        }
        old.path
            .to_str()
            .map(str::to_owned)
            .ok_or("The selected path is not valid Unicode.".into())
    }
    pub fn resolve_source_access(&self, id: &str) -> Result<Option<Value>, String> {
        self.resolve(id, "folder")?;
        Ok(None)
    }
    pub async fn pick(&self, kind: &str) -> Result<Value, String> {
        if !KINDS.contains(&kind) {
            return Err("This selection is unavailable in the Windows preview.".into());
        }
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Finish the open Windows chooser first.".into());
        }
        let active = self.active.clone();
        let generation = self.generation.load(Ordering::Acquire);
        let kind_owned = kind.to_owned();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("photofindry-windows-chooser".into())
            .spawn(move || {
                struct Reset(Arc<AtomicBool>);
                impl Drop for Reset {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _reset = Reset(active);
                let _ = sender.send(super::native::choose(&kind_owned));
            })
            .map_err(|_| {
                self.active.store(false, Ordering::Release);
                "The Windows file chooser could not start."
            })?;
        let result = tokio::time::timeout(Duration::from_secs(300), receiver)
            .await
            .map_err(|_| "The Windows chooser took too long. Close it and choose again.")?
            .map_err(|_| "The Windows chooser stopped unexpectedly.")??;
        if generation != self.generation.load(Ordering::Acquire) {
            return Err("The local session changed; choose the location again.".into());
        }
        match result {
            Some(path) => self.grant_at(
                kind,
                path.to_str()
                    .ok_or("The selected path is not valid Unicode.")?,
                generation,
            ),
            None => Ok(json!({"cancelled":true})),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selections_are_opaque_typed_and_rechecked_against_real_file_identity() {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("pf-win-picker-{id}"));
        fs::private_dir(&path, true).unwrap();
        let model = path.join("model.gguf");
        std::fs::write(&model, b"original-model").unwrap();
        assert!(
            fs::open_read_no_reparse(&path).is_err(),
            "The native regular-file API must reject directories."
        );
        assert!(fs::open_read_directory_no_reparse(&path).is_ok());
        assert!(fs::open_read_directory_no_reparse(&model).is_err());
        let picker = Picker::new(&path);
        assert!(picker.grant_path("model", path.to_str().unwrap()).is_err());
        assert!(
            picker
                .grant_path("folder", model.to_str().unwrap())
                .is_err()
        );
        let selected = picker.grant_path("model", model.to_str().unwrap()).unwrap();
        let grant = selected["grantId"].as_str().unwrap();
        assert!(!selected.to_string().contains(path.to_str().unwrap()));
        assert_eq!(
            picker.resolve(grant, "model").unwrap(),
            std::fs::canonicalize(&model).unwrap().to_str().unwrap()
        );
        assert!(picker.resolve(grant, "folder").is_err());
        assert!(picker.grant_path("model", "relative.gguf").is_err());
        std::fs::write(&model, b"changed-model-length").unwrap();
        assert!(picker.resolve(grant, "model").is_err());
        let selected = picker.grant_path("folder", path.to_str().unwrap()).unwrap();
        let folder = selected["grantId"].as_str().unwrap();
        std::fs::write(path.join("new-photo.jpg"), b"new").unwrap();
        assert!(picker.resolve(folder, "folder").is_ok());
        picker.revoke();
        assert!(picker.resolve(folder, "folder").is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
}
