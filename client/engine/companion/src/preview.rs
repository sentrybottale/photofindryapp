//! Cached previews only. Each path component is opened relative to a held directory
//! descriptor with O_NOFOLLOW; canonicalization alone would permit symlink races.
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
    path::{Component, Path, PathBuf},
};

pub const MAX_PREVIEW: u64 = 8 * 1024 * 1024;
pub struct Previews {
    root: PathBuf,
    directory: Option<File>,
}

impl Previews {
    pub fn open(data: &Path) -> Result<Self, String> {
        let root = data.to_owned();
        let directory = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&root)
        {
            Ok(file) => Some(file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("The isolated preview directory is invalid.".into()),
        };
        Ok(Self {
            root,
            directory,
        })
    }
    pub fn eligible(&self, path: &str) -> bool {
        self.relative(path).is_ok() && self.directory.is_some()
    }
    fn relative<'a>(&self, path: &'a str) -> Result<Vec<&'a std::ffi::OsStr>, ()> {
        let relative = Path::new(path).strip_prefix(&self.root).map_err(|_| ())?;
        let parts: Vec<_> = relative
            .components()
            .map(|part| match part {
                Component::Normal(name) => Ok(name),
                _ => Err(()),
            })
            .collect::<Result<_, _>>()?;
        if parts.len() < 2
            || parts.len() > 12
            || ![
                "previews",
                "offline-previews",
                "restored-assets",
                "faces",
                "source-capsules",
                "detail-crops",
            ]
            .contains(&parts[0].to_str().unwrap_or(""))
        {
            return Err(());
        }
        Ok(parts)
    }
    /// Resolve only from this request's fresh worker-authorized record. A path
    /// seen in an older gallery/People/Removed response is never a capability.
    pub fn reference(&self, photo: &serde_json::Value, face: Option<&str>, crop: Option<&str>) -> Option<String> {
        let id = photo["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 128)?;
        if face.is_some() && crop.is_some() { return None; }
        let path = if let Some(face_id) = face {
            photo["faces"].as_array()?.iter()
                .find(|face| face["id"] == face_id && face["photoId"] == id)?["cropPath"].as_str()?
        } else if let Some(crop_id) = crop {
            let mut rows = photo["detailCrops"].as_array()?.iter().filter(|crop| crop["id"] == crop_id);
            let found = rows.next()?;
            if rows.next().is_some() { return None; }
            found["path"].as_str()?
        } else {
            photo["offlinePreview"]["path"].as_str().or_else(|| photo["thumbnailPath"].as_str())?
        };
        self.eligible(path).then(|| path.to_owned())
    }
    pub fn read(&self, path: &str) -> Result<(&'static str, Vec<u8>), ()> {
        let parts = self.relative(path)?;
        let mut held = self
            .directory
            .as_ref()
            .ok_or(())?
            .try_clone()
            .map_err(|_| ())?;
        for (index, part) in parts.iter().enumerate() {
            let name = CString::new(part.as_encoded_bytes()).map_err(|_| ())?;
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if index + 1 < parts.len() {
                    libc::O_DIRECTORY
                } else {
                    libc::O_NONBLOCK
                };
            // SAFETY: held remains open; CString is terminated; successful fd is owned exactly once.
            let fd = unsafe { libc::openat(held.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(());
            }
            held = unsafe { File::from_raw_fd(fd) };
        }
        let metadata = held.metadata().map_err(|_| ())?;
        if !metadata.is_file() || metadata.len() > MAX_PREVIEW {
            return Err(());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        held.take(MAX_PREVIEW + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() as u64 > MAX_PREVIEW {
            return Err(());
        }
        let mime = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else {
            return Err(());
        };
        Ok((mime, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_require_this_records_current_photo_and_face_membership() {
        let base = std::env::temp_dir();
        let previews = Previews::open(&base).unwrap();
        let path = base.join("previews/current.png").to_string_lossy().into_owned();
        let current = serde_json::json!({"id":"photo-a","thumbnailPath":path,
            "faces":[{"id":"face-a","photoId":"photo-a","cropPath":path}]});
        assert_eq!(previews.reference(&current, None, None), Some(path.clone()));
        assert_eq!(previews.reference(&current, Some("face-a"), None), Some(path.clone()));
        assert!(previews.reference(&current, Some("other-face"), None).is_none());
        let missing = serde_json::json!({"id":"photo-a"});
        assert!(previews.reference(&missing, None, None).is_none());
        assert!(previews.reference(&missing, Some("face-a"), None).is_none());
        let wrong_owner = serde_json::json!({"id":"photo-b","faces":[{"id":"face-a","photoId":"photo-a","cropPath":path}]});
        assert!(previews.reference(&wrong_owner, Some("face-a"), None).is_none());
    }
    #[test]
    fn rejects_escape_symlinks_and_nonimages() {
        let base =
            std::env::temp_dir().join(format!("photofindry-preview-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("previews/nested")).unwrap();
        std::fs::write(base.join("secret.jpg"), b"\xff\xd8\xffprivate").unwrap();
        std::fs::write(base.join("previews/good.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
        std::fs::write(base.join("previews/not-image"), b"<script>bad</script>").unwrap();
        std::os::unix::fs::symlink(base.join("secret.jpg"), base.join("previews/link.jpg"))
            .unwrap();
        std::os::unix::fs::symlink(&base, base.join("previews/directory-link")).unwrap();
        let previews = Previews::open(&base).unwrap();
        let path = |part: &str| base.join(part).to_string_lossy().into_owned();
        assert!(previews.read(&path("previews/good.png")).is_ok());
        for rejected in [
            "secret.jpg",
            "previews/../secret.jpg",
            "previews/link.jpg",
            "previews/directory-link/secret.jpg",
            "previews/not-image",
        ] {
            assert!(previews.read(&path(rejected)).is_err(), "{rejected}");
        }
        std::fs::remove_dir_all(&base).unwrap();
    }
}
