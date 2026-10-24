//! Cached previews only, opened through native no-reparse handles and real IDs.
use photofindry_platform::fs::{self as native_fs, File, Identity};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MAX_PREVIEW: u64 = 8 * 1024 * 1024;
pub struct Previews {
    root: PathBuf,
    directory: Option<File>,
    identity: Identity,
}

impl Previews {
    pub fn open(data: &Path) -> Result<Self, String> {
        // Native opens retain every non-reparse ancestor handle. Both the saved
        // root and the resolved asset remain pinned while their bytes are read.
        let directory = native_fs::open_read_directory_no_reparse(data)
            .map_err(|_| "Cannot safely open the preview root.")?;
        let identity =
            native_fs::identity(&directory).map_err(|_| "Cannot inspect the preview root.")?;
        if !identity.is_directory() {
            return Err("Preview storage is not a directory.".into());
        }
        Ok(Self {
            root: data.into(),
            directory: Some(directory),
            identity,
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
    pub fn reference(
        &self,
        photo: &serde_json::Value,
        face: Option<&str>,
        crop: Option<&str>,
    ) -> Option<String> {
        let id = photo["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)?;
        if face.is_some() && crop.is_some() {
            return None;
        }
        let path = if let Some(face_id) = face {
            photo["faces"]
                .as_array()?
                .iter()
                .find(|face| face["id"] == face_id && face["photoId"] == id)?["cropPath"]
                .as_str()?
        } else if let Some(crop_id) = crop {
            let mut rows = photo["detailCrops"]
                .as_array()?
                .iter()
                .filter(|crop| crop["id"] == crop_id);
            let found = rows.next()?;
            if rows.next().is_some() {
                return None;
            }
            found["path"].as_str()?
        } else {
            photo["offlinePreview"]["path"]
                .as_str()
                .or_else(|| photo["thumbnailPath"].as_str())?
        };
        self.eligible(path).then(|| path.to_owned())
    }
    pub fn read(&self, path: &str) -> Result<(&'static str, Vec<u8>), ()> {
        self.relative(path)?;
        let root = native_fs::identity(self.directory.as_ref().ok_or(())?).map_err(|_| ())?;
        let current_root = native_fs::open_read_directory_no_reparse(&self.root).map_err(|_| ())?;
        let current_root_id = native_fs::identity(&current_root).map_err(|_| ())?;
        if root.volume != self.identity.volume
            || root.file_id != self.identity.file_id
            || root.volume != current_root_id.volume
            || root.file_id != current_root_id.file_id
        {
            return Err(());
        }
        let mut held = native_fs::open_read_no_reparse(Path::new(path)).map_err(|_| ())?;
        let before = native_fs::identity(&held).map_err(|_| ())?;
        if before.is_directory() || before.links != 1 || before.size > MAX_PREVIEW {
            return Err(());
        }
        let mut bytes = Vec::with_capacity(before.size as usize);
        (&mut held)
            .take(MAX_PREVIEW + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() as u64 > MAX_PREVIEW || native_fs::identity(&held).map_err(|_| ())? != before
        {
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
    fn current_cached_image_read_is_bounded_and_original_paths_cannot_be_served() {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let root = std::env::temp_dir().join(format!("pf-win-previews-{id}"));
        native_fs::private_dir(&root, true).unwrap();
        native_fs::private_dir(&root.join("previews"), true).unwrap();
        std::fs::write(
            root.join("previews/saved.png"),
            b"\x89PNG\r\n\x1a\nsaved-fixture",
        )
        .unwrap();
        std::fs::write(root.join("original.jpg"), b"\xff\xd8\xfforiginal").unwrap();
        std::fs::write(
            root.join("previews/not-image"),
            b"<script>untrusted</script>",
        )
        .unwrap();
        let previews = Previews::open(&root).unwrap();
        let cached = root.join("previews/saved.png").to_str().unwrap().to_owned();
        assert_eq!(previews.read(&cached).unwrap().0, "image/png");
        assert!(
            previews
                .read(root.join("original.jpg").to_str().unwrap())
                .is_err()
        );
        assert!(
            previews
                .read(root.join("previews/not-image").to_str().unwrap())
                .is_err()
        );
        assert!(
            previews
                .read(root.join("previews/../original.jpg").to_str().unwrap())
                .is_err()
        );
        let photo = serde_json::json!({"id":"photo","thumbnailPath":cached});
        assert!(previews.reference(&photo, None, None).is_some());
        assert!(
            previews
                .reference(&photo, Some("unrelated-face"), None)
                .is_none()
        );
        drop(previews);
        std::fs::remove_dir_all(root).unwrap();
    }
}
