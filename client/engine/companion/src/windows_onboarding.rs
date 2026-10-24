//! Host-only guide progress. Portable library and source exports never include this file.
use photofindry_platform::fs::{self as native_fs, File};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
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
        // The private directory handle already pins the parent and proves ACLs;
        // reading a regular-file handle for this directory would always fail.
        native_fs::private_dir(&self.directory, false)
            .map_err(|_| "Local guide storage is not private to this account.".into())
    }
    fn read(&self) -> Result<Option<Record>, String> {
        let _root = self.open_directory()?;
        let file = match native_fs::private_file(&self.path(), false) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot read guide preferences safely.".into()),
        };
        let id = native_fs::identity(&file).map_err(|_| "Cannot inspect guide preferences.")?;
        if id.is_directory() || id.links != 1 || id.size > MAX_BYTES {
            return Err("Unsafe or oversized guide preferences.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read guide preferences.")?;
        let record: Record =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid guide preferences.")?;
        record.validate()?;
        Ok(Some(record))
    }
    fn write(&self, record: &Record) -> Result<(), String> {
        record.validate()?;
        let _root = self.open_directory()?;
        let mut bytes =
            serde_json::to_vec(record).map_err(|_| "Cannot encode guide preferences.")?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Guide preferences exceed their size limit.".into());
        }
        native_fs::atomic_write_private(&self.path(), &bytes)
            .map_err(|_| "Cannot save guide preferences safely.".into())
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
    #[test]
    fn private_directory_guide_defaults_update_and_reopen_do_not_use_a_regular_file_open() {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let root = std::env::temp_dir().join(format!("pf-win-guide-{id}"));
        drop(native_fs::private_dir(&root, true).unwrap());
        assert!(native_fs::open_read_no_reparse(&root).is_err());
        let mut store = Store::new(&root);
        assert_eq!(store.snapshot("library-a").unwrap()["step"], 0);
        assert!(!root.join(FILE_NAME).exists());
        store
            .update("library-a", json!({"step":3,"hasStarted":true}))
            .unwrap();
        let mut reopened = Store::new(&root);
        assert_eq!(reopened.snapshot("library-a").unwrap()["step"], 3);
        assert_eq!(reopened.snapshot("library-b").unwrap()["step"], 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
