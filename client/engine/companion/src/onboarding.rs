//! Host-only guide progress. Portable library and source exports never include this file.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const VERSION: u64 = 1;
const MAX_BYTES: u64 = 16 * 1024;
const MAX_ID_BYTES: usize = 128;
const FILE_NAME: &str = "companion-ui.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Record {
    version: u64,
    library_id: String,
    step: u8,
    automatic_enabled: bool,
    completed: bool,
    has_started: bool,
    starter_source_id: String,
    starter_root_id: String,
    #[serde(rename = "taggingJobID")]
    tagging_job_id: String,
}

impl Record {
    fn fresh(library_id: &str) -> Self {
        Self {
            version: VERSION,
            library_id: library_id.into(),
            step: 0,
            automatic_enabled: true,
            completed: false,
            has_started: false,
            starter_source_id: String::new(),
            starter_root_id: String::new(),
            tagging_job_id: String::new(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("Unsupported local guide preferences version.".into());
        }
        if !valid_id(&self.library_id, false)
            || self.step > 9
            || !valid_id(&self.starter_source_id, true)
            || !valid_id(&self.starter_root_id, true)
            || !valid_id(&self.tagging_job_id, true)
        {
            return Err("Invalid local guide preferences.".into());
        }
        Ok(())
    }

    fn value(&self) -> Value {
        json!({
            "version": self.version,
            "libraryId": self.library_id,
            "step": self.step,
            "automaticEnabled": self.automatic_enabled,
            "completed": self.completed,
            "hasStarted": self.has_started,
            "starterSourceId": self.starter_source_id,
            "starterRootId": self.starter_root_id,
            "taggingJobID": self.tagging_job_id,
        })
    }
}

fn valid_id(value: &str, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// The owning host serializes access through its mutex. No file is read at startup.
pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(data: &Path) -> Self {
        Self {
            directory: data.into(),
        }
    }

    pub fn snapshot(&mut self, library_id: &str) -> Result<Value, String> {
        if !valid_id(library_id, false) {
            return Err("Invalid local library identity.".into());
        }
        let record = match self.read()? {
            Some(record) if record.library_id == library_id => record,
            _ => Record::fresh(library_id),
        };
        Ok(record.value())
    }

    pub fn update(&mut self, library_id: &str, patch: Value) -> Result<Value, String> {
        if !valid_id(library_id, false) {
            return Err("Invalid local library identity.".into());
        }
        let changes = patch
            .as_object()
            .ok_or("Local guide changes must be an object.")?;
        if changes.keys().any(|key| {
            ![
                "step",
                "automaticEnabled",
                "completed",
                "hasStarted",
                "starterSourceId",
                "starterRootId",
                "taggingJobID",
            ]
            .contains(&key.as_str())
        }) {
            return Err("Unknown local guide preference.".into());
        }
        let mut record = match self.read()? {
            Some(record) if record.library_id == library_id => record,
            _ => Record::fresh(library_id),
        };
        for (key, value) in changes {
            match key.as_str() {
                "step" => {
                    record.step = value
                        .as_u64()
                        .filter(|step| *step <= 9)
                        .ok_or("Invalid guide step.")? as u8;
                }
                "automaticEnabled" => {
                    record.automatic_enabled = value
                        .as_bool()
                        .ok_or("Invalid automatic guide preference.")?;
                }
                "completed" => {
                    record.completed = value.as_bool().ok_or("Invalid guide completion.")?;
                }
                "hasStarted" => {
                    record.has_started = value.as_bool().ok_or("Invalid guide start state.")?;
                }
                "starterSourceId" => {
                    record.starter_source_id = checked_id(value)?.into();
                }
                "starterRootId" => {
                    record.starter_root_id = checked_id(value)?.into();
                }
                "taggingJobID" => {
                    record.tagging_job_id = checked_id(value)?.into();
                }
                _ => unreachable!(),
            }
        }
        self.write(&record)?;
        Ok(record.value())
    }

    /// Explicit guide-only recovery. A malformed regular record may be replaced,
    /// while symlinks and special files remain untouched.
    pub fn reset(&mut self, library_id: &str) -> Result<Value, String> {
        if !valid_id(library_id, false) {
            return Err("Invalid local library identity.".into());
        }
        match fs::symlink_metadata(self.path()) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("Local guide preferences must be a regular file.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot inspect local guide preferences.".into()),
        }
        let record = Record::fresh(library_id);
        self.write(&record)?;
        Ok(record.value())
    }

    fn path(&self) -> PathBuf {
        self.directory.join(FILE_NAME)
    }

    fn open_directory(&self) -> Result<File, String> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.directory)
            .map_err(|_| "Cannot open local guide preferences directory.".into())
    }

    fn read(&self) -> Result<Option<Record>, String> {
        let _directory = self.open_directory()?;
        let path = self.path();
        let before = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot inspect local guide preferences.".into()),
        };
        if !before.is_file() || before.file_type().is_symlink() {
            return Err("Local guide preferences must be a regular file.".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| "Cannot read local guide preferences.")?;
        let metadata = file
            .metadata()
            .map_err(|_| "Cannot inspect local guide preferences.")?;
        if !metadata.is_file()
            || metadata.dev() != before.dev()
            || metadata.ino() != before.ino()
            || metadata.mode() & 0o077 != 0
            || metadata.len() > MAX_BYTES
        {
            return Err("Unsafe or oversized local guide preferences.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read local guide preferences.")?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Local guide preferences exceed their size limit.".into());
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid local guide preferences JSON.")?;
        let version = value["version"]
            .as_u64()
            .ok_or("Invalid local guide preferences version.")?;
        if version != VERSION {
            return Err("Unsupported local guide preferences version.".into());
        }
        let record: Record = serde_json::from_value(value)
            .map_err(|_| "Invalid local guide preferences fields.")?;
        record.validate()?;
        Ok(Some(record))
    }

    fn write(&self, record: &Record) -> Result<(), String> {
        record.validate()?;
        let directory = self.open_directory()?;
        let mut bytes = serde_json::to_vec(record)
            .map_err(|_| "Cannot encode local guide preferences.")?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Local guide preferences exceed their size limit.".into());
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| "Cannot allocate local guide preferences staging file.")?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let temporary = self.directory.join(format!(".companion-ui-{suffix}.tmp"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)
            .map_err(|_| "Cannot stage local guide preferences.")?;
        let result = (|| {
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Cannot protect local guide preferences.")?;
            file.write_all(&bytes)
                .map_err(|_| "Cannot save local guide preferences.")?;
            file.sync_all()
                .map_err(|_| "Cannot flush local guide preferences.")?;
            drop(file);
            fs::rename(&temporary, self.path())
                .map_err(|_| "Cannot publish local guide preferences.")?;
            directory
                .sync_all()
                .map_err(|_| "Cannot flush local guide preferences directory.")?;
            Ok::<(), String>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn checked_id(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|id| valid_id(id, true))
        .ok_or("Invalid local guide identity.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut random = [0u8; 12];
            getrandom::fill(&mut random).unwrap();
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let path = std::env::temp_dir().join(format!("photofindry-onboarding-{suffix}"));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join(FILE_NAME)
        }
        fn write_record(&self, value: Value) {
            fs::write(self.path(), value.to_string()).unwrap();
            fs::set_permissions(self.path(), fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_uses_defaults_without_creating_state() {
        let fixture = Fixture::new();
        let mut store = Store::new(&fixture.0);
        assert_eq!(store.snapshot("library-a").unwrap(), Record::fresh("library-a").value());
        assert!(!fixture.path().exists());
    }

    #[test]
    fn atomic_update_survives_restart_with_private_mode() {
        let fixture = Fixture::new();
        let mut store = Store::new(&fixture.0);
        let value = store
            .update(
                "library-a",
                json!({"step":3,"hasStarted":true,"automaticEnabled":false,
                    "starterSourceId":"source-a","starterRootId":"root-a","taggingJobID":"job-a"}),
            )
            .unwrap();
        assert_eq!(value["step"], 3);
        assert_eq!(fs::metadata(fixture.path()).unwrap().mode() & 0o777, 0o600);
        assert_eq!(Store::new(&fixture.0).snapshot("library-a").unwrap(), value);
        let entries: Vec<_> = fs::read_dir(&fixture.0).unwrap().collect();
        assert_eq!(entries.len(), 1, "staging file must be removed after publication");
        let cleared = store
            .update("library-a", json!({"taggingJobID":"","starterRootId":""}))
            .unwrap();
        assert_eq!(cleared["taggingJobID"], "");
        assert_eq!(cleared["starterRootId"], "");
    }

    #[test]
    fn other_library_defaults_without_rewriting_until_explicit_update() {
        let fixture = Fixture::new();
        let mut store = Store::new(&fixture.0);
        store.update("library-a", json!({"step":6,"completed":true})).unwrap();
        let original = fs::read(fixture.path()).unwrap();
        assert_eq!(store.snapshot("library-b").unwrap(), Record::fresh("library-b").value());
        assert_eq!(fs::read(fixture.path()).unwrap(), original);
        store.update("library-b", json!({"hasStarted":true})).unwrap();
        assert_eq!(store.snapshot("library-b").unwrap()["hasStarted"], true);
        assert_eq!(store.snapshot("library-a").unwrap(), Record::fresh("library-a").value());
    }

    #[test]
    fn rejects_unknown_paths_tokens_bad_types_and_oversized_updates_without_changes() {
        let fixture = Fixture::new();
        let mut store = Store::new(&fixture.0);
        store.update("library-a", json!({"step":2})).unwrap();
        let original = fs::read(fixture.path()).unwrap();
        for patch in [
            json!({"path":"/private/photos"}),
            json!({"scopeId":"opaque"}),
            json!({"token":"secret"}),
            json!({"photos":[]}),
            json!({"ready":true}),
            json!({"step":10}),
            json!({"step":-1}),
            json!({"step":"3"}),
            json!({"automaticEnabled":1}),
            json!({"completed":null}),
            json!({"starterSourceId":"/private/photos"}),
            json!({"taggingJobID":"x".repeat(129)}),
        ] {
            assert!(store.update("library-a", patch).is_err());
            assert_eq!(fs::read(fixture.path()).unwrap(), original);
        }
    }

    #[test]
    fn malformed_unsupported_oversized_and_symlink_records_fail_explicitly() {
        let fixture = Fixture::new();
        let mut store = Store::new(&fixture.0);
        fixture.write_record(json!({"version":2}));
        assert!(store.snapshot("library-a").unwrap_err().contains("Unsupported"));
        assert_eq!(store.reset("library-a").unwrap(), Record::fresh("library-a").value());
        fixture.write_record(json!({"version":1,"libraryId":"library-a","step":"3"}));
        assert!(store.snapshot("library-a").is_err());
        fixture.write_record(json!({"version":1,"libraryId":"library-a","step":0,
            "automaticEnabled":true,"completed":false,"hasStarted":false,
            "starterSourceId":"","starterRootId":"","taggingJobID":"",
            "path":"/private/photos"}));
        assert!(store.snapshot("library-a").is_err());
        fs::write(fixture.path(), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(store.snapshot("library-a").is_err());
        fs::remove_file(fixture.path()).unwrap();
        let target = fixture.0.join("kept-target");
        fs::write(&target, b"unchanged").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path()).unwrap();
        assert!(store.snapshot("library-a").is_err());
        assert!(store.update("library-a", json!({"step":1})).is_err());
        assert!(store.reset("library-a").is_err());
        assert!(fs::symlink_metadata(fixture.path()).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(target).unwrap(), b"unchanged");
    }
}
