//! Host-owned browser appearance. This is not library evidence or model configuration.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

const FILE_NAME: &CStr = c"companion-appearance.json";
const MAX_BYTES: u64 = 1024;

fn recovery(reason: &str) -> String {
    format!(
        "{reason} Your selected appearance still works for this session. Open Help & feedback, choose Bug report and paste this message."
    )
}

#[derive(Clone, Copy, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    fn value(self) -> Value {
        json!({"mode": self})
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub mode: Mode,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    mode: Mode,
}

/// Serialized by the companion state mutex. Every read is bounded; atomic
/// publication also lets another open browser see the current host preference.
pub struct Store {
    directory: PathBuf,
}
impl Store {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        Ok(self.mode()?.value())
    }

    pub fn mode(&self) -> Result<Mode, String> {
        let directory = self.open_directory()?;
        let Some(bytes) = Self::read(&directory)? else {
            return Ok(Mode::System);
        };
        let record: Record = serde_json::from_slice(&bytes)
            .map_err(|_| "Appearance could not be read. Choose Light, Dark or Follow System to save it again.")?;
        if record.version != 1 {
            return Err("This appearance preference uses a newer format. Update PhotoFindry before changing it.".into());
        }
        Ok(record.mode)
    }

    pub fn update(&self, mode: Mode) -> Result<Value, String> {
        let directory = self.open_directory()?;
        // An explicit choice can repair malformed JSON, but cannot replace
        // linked, shared, oversized or unsupported-version records.
        if let Some(bytes) = Self::read(&directory)? {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                && value.get("version").is_some_and(|version| version != 1)
            {
                return Err("This appearance preference uses a newer format. Update PhotoFindry before changing it.".into());
            }
        }
        let mut bytes = serde_json::to_vec(&Record { version: 1, mode })
            .map_err(|_| "Appearance could not be saved. Choose the mode again to retry.")?;
        bytes.push(b'\n');
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| "Appearance could not be saved. Choose the mode again to retry.")?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let temporary = CString::new(format!(".companion-appearance-{suffix}.tmp")).unwrap();
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                temporary.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if descriptor < 0 {
            return Err("Appearance could not be saved. Choose the mode again to retry.".into());
        }
        let mut file = unsafe { File::from_raw_fd(descriptor) };
        let result = (|| -> Result<(), String> {
            let failed =
                || "Appearance could not be saved. Choose the mode again to retry.".to_owned();
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| failed())?;
            file.write_all(&bytes).map_err(|_| failed())?;
            file.sync_all().map_err(|_| failed())?;
            drop(file);
            if unsafe {
                libc::renameat(
                    directory.as_raw_fd(),
                    temporary.as_ptr(),
                    directory.as_raw_fd(),
                    FILE_NAME.as_ptr(),
                )
            } != 0
            {
                return Err(failed());
            }
            directory.sync_all().map_err(|_| failed())?;
            Ok(())
        })();
        if result.is_err() {
            unsafe {
                libc::unlinkat(directory.as_raw_fd(), temporary.as_ptr(), 0);
            }
        }
        result?;
        Ok(mode.value())
    }

    fn open_directory(&self) -> Result<File, String> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.directory)
            .map_err(|_| "Appearance is unavailable. Reopen PhotoFindry and try again.".into())
    }

    fn read(directory: &File) -> Result<Option<Vec<u8>>, String> {
        // Relative opens/rename keep all file operations on the directory we
        // checked, even if its pathname is replaced concurrently.
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                FILE_NAME.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if descriptor < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(recovery(
                "Appearance preferences could not be opened safely.",
            ));
        }
        let file = unsafe { File::from_raw_fd(descriptor) };
        let metadata = file
            .metadata()
            .map_err(|_| recovery("Appearance preferences could not be checked."))?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(recovery(
                "Appearance preferences are not a private, ordinary file.",
            ));
        }
        if metadata.len() > MAX_BYTES {
            return Err(recovery("Appearance preferences exceed their size limit."));
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(
            |_| "Appearance preferences could not be read. Reopen PhotoFindry and try again.",
        )?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(recovery("Appearance preferences exceed their size limit."));
        }
        Ok(Some(bytes))
    }
}

/// The only dynamic HTML is a closed enum. System mode is resolved by the local
/// stylesheet's media query, so no inline script or browser storage is needed.
pub fn html(template: &str, mode: Mode) -> String {
    template.replacen(
        "<html lang=\"en\">",
        &format!("<html lang=\"en\" data-appearance=\"{}\">", mode.name()),
        1,
    )
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
            let path = std::env::temp_dir().join(format!("photofindry-appearance-{suffix}"));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join(FILE_NAME.to_str().unwrap())
        }
        fn store(&self) -> Store {
            Store::new(&self.0)
        }
        fn write(&self, bytes: &[u8]) {
            fs::write(self.path(), bytes).unwrap();
            fs::set_permissions(self.path(), fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_are_read_only_and_each_mode_survives_reopen() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.store().snapshot().unwrap(),
            json!({"mode":"system"})
        );
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
        for mode in [Mode::Dark, Mode::Light, Mode::System] {
            assert_eq!(fixture.store().update(mode).unwrap(), mode.value());
            assert_eq!(fixture.store().mode().unwrap(), mode);
            assert_eq!(fs::metadata(fixture.path()).unwrap().mode() & 0o777, 0o600);
            assert_eq!(
                fs::read_dir(&fixture.0).unwrap().count(),
                1,
                "No leftover staging files"
            );
        }
    }

    #[test]
    fn only_closed_enum_and_exact_update_fields_are_accepted() {
        for bytes in [
            r#"{}"#,
            r#"{"mode":"auto"}"#,
            r#"{"mode":true}"#,
            r#"{"mode":null}"#,
            r#"{"mode":"dark","path":"/private"}"#,
            r#"{"mode":"dark","mode":"light"}"#,
            r#"{"mode":"\" onload=\"alert(1)"}"#,
        ] {
            assert!(serde_json::from_str::<Update>(bytes).is_err(), "{bytes}");
        }
        for mode in [Mode::System, Mode::Light, Mode::Dark] {
            let value: Update = serde_json::from_value(mode.value()).unwrap();
            assert_eq!(value.mode, mode);
            let html = html(
                "<!doctype html>\n<html lang=\"en\"><head></head></html>",
                mode,
            );
            assert!(html.contains(&format!("data-appearance=\"{}\"", mode.name())));
            assert!(!html.contains("<script"));
        }
    }

    #[test]
    fn malformed_preferences_recover_only_after_explicit_choice() {
        let fixture = Fixture::new();
        fixture.write(b"invalid JSON");
        assert!(fixture.store().snapshot().is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), b"invalid JSON");
        fixture.store().update(Mode::Dark).unwrap();
        assert_eq!(fixture.store().mode().unwrap(), Mode::Dark);
        fixture.write(br#"{"version":2,"mode":"light"}"#);
        assert!(fixture.store().snapshot().is_err());
        assert!(fixture.store().update(Mode::Dark).is_err());
        assert_eq!(
            fs::read(fixture.path()).unwrap(),
            br#"{"version":2,"mode":"light"}"#
        );
    }

    #[test]
    fn linked_oversized_shared_and_nonregular_records_are_never_read_or_replaced() {
        let fixture = Fixture::new();
        fixture.write(&vec![b' '; MAX_BYTES as usize + 1]);
        for mode in [Mode::System, Mode::Dark] {
            assert!(fixture.store().update(mode).is_err());
        }
        assert!(fixture.store().snapshot().is_err());
        assert_eq!(fs::metadata(fixture.path()).unwrap().len(), MAX_BYTES + 1);
        fixture.write(br#"{"version":1,"mode":"dark"}"#);
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(fixture.store().snapshot().is_err());
        assert!(fixture.store().update(Mode::Light).is_err());
        fs::remove_file(fixture.path()).unwrap();
        let target = fixture.0.join("kept.json");
        fs::write(&target, b"unchanged").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, fixture.path()).unwrap();
        assert!(fixture.store().snapshot().is_err());
        assert!(fixture.store().update(Mode::Light).is_err());
        fs::remove_file(fixture.path()).unwrap();
        fs::hard_link(&target, fixture.path()).unwrap();
        assert!(fixture.store().snapshot().is_err());
        assert!(fixture.store().update(Mode::Light).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"unchanged");
        fs::remove_file(fixture.path()).unwrap();
        fs::create_dir(fixture.path()).unwrap();
        assert!(fixture.store().snapshot().is_err());
        assert!(fixture.store().update(Mode::Light).is_err());
    }

    #[test]
    fn unsafe_or_oversized_preferences_have_a_real_manual_recovery_action() {
        let fixture = Fixture::new();
        fixture.write(&vec![b' '; MAX_BYTES as usize + 1]);
        let oversized = fixture.store().snapshot().unwrap_err();
        fs::remove_file(fixture.path()).unwrap();
        std::os::unix::fs::symlink(fixture.0.join("missing-target"), fixture.path()).unwrap();
        let linked = fixture.store().update(Mode::Light).unwrap_err();
        for message in [oversized, linked] {
            assert!(message.contains("selected appearance still works for this session"));
            assert!(
                message.contains("Open Help & feedback, choose Bug report and paste this message")
            );
            assert!(
                !message.contains("Reopen"),
                "Reopening cannot repair an unsafe preference file"
            );
        }
    }
}
