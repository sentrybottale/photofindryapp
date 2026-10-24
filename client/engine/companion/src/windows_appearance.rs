//! Closed-enum browser preference, kept in private current-user host storage.
use photofindry_platform::fs;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
const MAX: u64 = 1024;
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
        json!({"mode":self})
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub mode: Mode,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    mode: Mode,
}
pub struct Store {
    directory: PathBuf,
}
impl Store {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn path(&self) -> PathBuf {
        self.directory.join("companion-appearance.json")
    }
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        fs::private_dir(&self.directory, false)
            .map_err(|_| "Appearance storage is not private to this account.")?;
        let file = match fs::private_file(&self.path(), false) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Appearance preferences could not be opened safely.".into()),
        };
        let id = fs::identity(&file).map_err(|_| "Appearance preferences could not be checked.")?;
        if id.is_directory() || id.links != 1 || id.size > MAX {
            return Err("Appearance preferences must be a small private ordinary file.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Appearance preferences could not be read.")?;
        if bytes.len() as u64 > MAX {
            return Err("Appearance preferences exceed their bound.".into());
        }
        Ok(Some(bytes))
    }
    pub fn mode(&self) -> Result<Mode, String> {
        let Some(bytes) = self.read()? else {
            return Ok(Mode::System);
        };
        let r:Record=serde_json::from_slice(&bytes).map_err(|_|"Appearance could not be read. Choose Light, Dark or Follow System to save it again.")?;
        if r.version != 1 {
            return Err("This appearance preference uses a newer format. Update PhotoFindry before changing it.".into());
        }
        Ok(r.mode)
    }
    pub fn snapshot(&self) -> Result<Value, String> {
        Ok(self.mode()?.value())
    }
    pub fn update(&self, mode: Mode) -> Result<Value, String> {
        if let Some(bytes) = self.read()? {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if value.get("version").is_some_and(|v| v != 1) {
                    return Err("This appearance preference uses a newer format. Update PhotoFindry before changing it.".into());
                }
            }
        }
        let mut bytes = serde_json::to_vec(&Record { version: 1, mode })
            .map_err(|_| "Cannot encode appearance.")?;
        bytes.push(b'\n');
        fs::atomic_write_private(&self.path(), &bytes)
            .map_err(|_| "Appearance could not be saved safely.")?;
        Ok(mode.value())
    }
}
pub fn html(template: &str, mode: Mode) -> String {
    template.replacen(
        "<html lang=\"en\">",
        &format!("<html lang=\"en\" data-appearance=\"{}\">", mode.name()),
        1,
    )
}
