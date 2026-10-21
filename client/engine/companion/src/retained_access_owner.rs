//! Authenticated, fixed-protocol macOS access owner. Its sealed bytes and path
//! are separate from replaceable CLI versions. No browser supplies these paths.
use super::{cli_release as release, launch};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
pub const IDENTIFIER: &str = "com.photofindry.companion.source-access.v1";
const DIRECTORY: &str = "source-access";
const APP: &str = "PhotoFindryAccess.app";
const PREFIX: &str = "bin/CompanionSourceAccess.app";
const LIMIT: u64 = 64 * 1024 * 1024;
const FAILURE:&str="The retained PhotoFindry folder-access owner is missing or changed. Keep your library and source-access state. Rerun verified setup with the same authenticated access-owner artifact; a new permission identity was not generated.";
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Owner {
    schema: u32,
    protocol: u32,
    origin_build: u64,
    removed: bool,
}
pub fn directory(root: &Path) -> PathBuf {
    root.join(DIRECTORY)
}
pub fn application(root: &Path) -> PathBuf {
    directory(root).join(APP)
}
pub fn executable(root: &Path) -> PathBuf {
    application(root).join("Contents/MacOS/CompanionSourceAccess")
}
fn files(raw: &[u8], signed: &release::Release) -> Result<BTreeMap<PathBuf, String>> {
    if release::digest(raw) != signed.inventory_sha256 {
        return Err(FAILURE.into());
    }
    let text = std::str::from_utf8(raw).map_err(|_| FAILURE)?;
    let mut names = BTreeSet::new();
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let (hash, name) = line.split_once("  ").ok_or(FAILURE)?;
        let path = Path::new(name);
        if !release::sha_valid(hash)
            || name.len() > 1024
            || name.contains('\\')
            || name.chars().any(char::is_control)
            || path.components().count() > 16
            || path
                .components()
                .any(|p| !matches!(p, Component::Normal(_)))
            || name
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || !names.insert(name)
            || names.len() > 8192
        {
            return Err(FAILURE.into());
        }
        if let Ok(relative) = path.strip_prefix(PREFIX) {
            if relative.as_os_str().is_empty() {
                return Err(FAILURE.into());
            }
            result.insert(relative.to_path_buf(), hash.into());
        }
    }
    for required in [
        "Contents/Info.plist",
        "Contents/MacOS/CompanionSourceAccess",
        "Contents/_CodeSignature/CodeResources",
        "Contents/Resources/access-owner-protocol.json",
    ] {
        if !result.contains_key(Path::new(required)) {
            return Err(
                "This signed CLI release has no complete protocol1 folder-access owner.".into(),
            );
        }
    }
    if result.len() > 32 {
        return Err(FAILURE.into());
    }
    Ok(result)
}
fn owner(at: &Path) -> Result<Owner> {
    let value: Owner = serde_json::from_slice(&release::read(&at.join("owner.json"), 4096)?)
        .map_err(|_| FAILURE)?;
    if value.schema != 1 || value.protocol != 1 || value.origin_build == 0 {
        return Err(FAILURE.into());
    }
    Ok(value)
}
fn origin(
    at: &Path,
    verifier: &Path,
) -> Result<(Owner, Vec<u8>, Vec<u8>, BTreeMap<PathBuf, String>)> {
    launch::private_directory(at, false)?;
    let state = owner(at)?;
    let envelope = release::read(&at.join("origin-release.json"), 131072)?;
    let signed = release::verify(&envelope, verifier)?;
    if signed.build != state.origin_build {
        return Err(FAILURE.into());
    }
    let inventory = release::read(&at.join("origin-SHA256SUMS.txt"), 2 * 1024 * 1024)?;
    let expected = files(&inventory, &signed)?;
    Ok((state, envelope, inventory, expected))
}
fn verify_bundle(app: &Path, expected: &BTreeMap<PathBuf, String>) -> Result<()> {
    launch::private_directory(app, false)?;
    let mut dirs = BTreeSet::new();
    for p in expected.keys() {
        for parent in p.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
            dirs.insert(parent.to_path_buf());
        }
    }
    let mut pending = vec![app.to_path_buf()];
    let mut seen = BTreeSet::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).map_err(|_| FAILURE)? {
            let path = entry.map_err(|_| FAILURE)?.path();
            let relative = path.strip_prefix(app).map_err(|_| FAILURE)?.to_path_buf();
            let m = fs::symlink_metadata(&path).map_err(|_| FAILURE)?;
            if m.file_type().is_symlink()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o022 != 0
            {
                return Err(FAILURE.into());
            }
            if m.is_dir() {
                if !dirs.contains(&relative) {
                    return Err(FAILURE.into());
                }
                pending.push(path);
            } else if !m.is_file()
                || m.nlink() != 1
                || expected.get(&relative) != Some(&release::file_hash(&path, LIMIT)?)
                || !seen.insert(relative)
                || seen.len() > 32
            {
                return Err(FAILURE.into());
            }
        }
    }
    if seen.len() != expected.len() {
        return Err(FAILURE.into());
    }
    let protocol: serde_json::Value = serde_json::from_slice(&release::read(
        &app.join("Contents/Resources/access-owner-protocol.json"),
        4096,
    )?)
    .map_err(|_| FAILURE)?;
    if protocol != serde_json::json!({"schema":1,"protocol":1,"bundleIdentifier":IDENTIFIER}) {
        return Err(FAILURE.into());
    }
    Ok(())
}
pub fn verified(root: &Path, verifier: &Path) -> Result<PathBuf> {
    let at = directory(root);
    let (state, _, _, expected) = origin(&at, verifier)?;
    if state.removed {
        return Err(FAILURE.into());
    }
    verify_bundle(&at.join(APP), &expected)?;
    Ok(executable(root))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "Cannot stage the authenticated folder-access owner")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot flush the folder-access owner".into())
}
fn copy_bundle(source: &Path, target: &Path, expected: &BTreeMap<PathBuf, String>) -> Result<()> {
    launch::private_directory(target, true)?;
    for (relative, hash) in expected {
        let original = source.join(relative);
        if release::file_hash(&original, LIMIT)? != *hash {
            return Err(FAILURE.into());
        }
        let copied = target.join(relative);
        launch::private_directory(copied.parent().ok_or(FAILURE)?, true)?;
        let bytes = release::read(&original, LIMIT)?;
        write_new(&copied, &bytes)?;
        let mode = fs::metadata(&original)
            .map_err(|_| FAILURE)?
            .permissions()
            .mode()
            & 0o777;
        fs::set_permissions(&copied, fs::Permissions::from_mode(mode)).map_err(|_| FAILURE)?;
    }
    verify_bundle(target, expected)
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn stage(root: &Path) -> Result<Stage> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| "Cannot stage the folder-access owner")?;
    let path = root.join(format!(".source-access-stage-{}", release::digest(&nonce)));
    launch::private_directory(&path, true)?;
    Ok(Stage(path))
}
fn replace_owner(at: &Path, state: &Owner) -> Result<()> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| FAILURE)?;
    let temporary = at.join(format!(".owner-{}", release::digest(&nonce)));
    write_new(&temporary, &serde_json::to_vec(state).map_err(|_| FAILURE)?)?;
    fs::rename(&temporary, at.join("owner.json")).map_err(|_| FAILURE)?;
    launch::private_directory(at, false)?
        .sync_all()
        .map_err(|_| FAILURE.into())
}
/// An existing good owner always wins, including a newer CLI whose packaged
/// helper bytes differ. No ordinary update re-signs or replaces its identity.
pub fn provision(
    root: &Path,
    source_package: &Path,
    envelope: &[u8],
    verifier: &Path,
) -> Result<PathBuf> {
    let at = directory(root);
    if fs::symlink_metadata(&at).is_ok() {
        let (state, _, _, expected) = origin(&at, verifier)?;
        if !state.removed {
            return verified(root, verifier);
        }
        let _lease = launch::try_lock(&at, "owner.lock")?.ok_or(
            "A source-access operation is still closing. Retry verified setup after it finishes.",
        )?;
        // Explicit verified setup after completed uninstall may restore only
        // the identical authenticated owner, never manufacture a new identity.
        if fs::symlink_metadata(at.join(APP)).is_ok() {
            verify_bundle(&at.join(APP), &expected)?;
        } else {
            let retained = root
                .join("versions")
                .join(state.origin_build.to_string())
                .join("package")
                .join(PREFIX);
            let incoming = source_package.join(PREFIX);
            let source = [retained, incoming]
                .into_iter()
                .find(|candidate| {
                    expected.iter().all(|(p, h)| {
                        release::file_hash(&candidate.join(p), LIMIT).ok().as_ref() == Some(h)
                    })
                })
                .ok_or(FAILURE)?;
            let staged = stage(root)?;
            copy_bundle(&source, &staged.0.join(APP), &expected)?;
            fs::rename(staged.0.join(APP), at.join(APP)).map_err(|_| FAILURE)?;
        }
        replace_owner(
            &at,
            &Owner {
                removed: false,
                ..state
            },
        )?;
        return verified(root, verifier);
    }
    // State is retained separately. Its presence without an owner receipt is
    // a lost authority record, not permission to create a new app identity.
    if fs::symlink_metadata(root.join("source-access-state")).is_ok() {
        return Err(FAILURE.into());
    }
    let signed = release::verify(envelope, verifier)?;
    let inventory = release::read(&source_package.join("SHA256SUMS.txt"), 2 * 1024 * 1024)?;
    let expected = files(&inventory, &signed)?;
    let staged = stage(root)?;
    copy_bundle(&source_package.join(PREFIX), &staged.0.join(APP), &expected)?;
    write_new(&staged.0.join("origin-release.json"), envelope)?;
    write_new(&staged.0.join("origin-SHA256SUMS.txt"), &inventory)?;
    write_new(&staged.0.join("owner.lock"), b"")?;
    write_new(
        &staged.0.join("owner.json"),
        &serde_json::to_vec(&Owner {
            schema: 1,
            protocol: 1,
            origin_build: signed.build,
            removed: false,
        })
        .map_err(|_| FAILURE)?,
    )?;
    fs::rename(&staged.0, &at).map_err(|_| "Cannot publish the retained folder-access owner")?;
    launch::private_directory(root, false)?
        .sync_all()
        .map_err(|_| "Cannot flush the retained folder-access owner")?;
    verified(root, verifier)
}
/// Caller has already drained the companion and all owned readers. The shared
/// helper lease still prevents deleting a chooser/restore process in progress.
pub fn uninstall(root: &Path, verifier: &Path) -> Result<()> {
    if fs::symlink_metadata(directory(root)).is_err() {
        return Ok(());
    }
    let at = directory(root);
    let (state, _, _, expected) = origin(&at, verifier)?;
    if state.removed && fs::symlink_metadata(at.join(APP)).is_err() {
        return Ok(());
    }
    verify_bundle(&at.join(APP), &expected)?;
    let lock = launch::private_file(&at.join("owner.lock"), true)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("A native folder-access operation is still open. Finish or cancel it, then retry uninstall; your library is kept.".into());
    }
    replace_owner(
        &at,
        &Owner {
            removed: true,
            ..state
        },
    )?;
    fs::remove_dir_all(at.join(APP))
        .map_err(|_| "Cannot remove the verified folder-access owner; retry uninstall")?;
    launch::private_directory(&at, false)?
        .sync_all()
        .map_err(|_| FAILURE)?;
    // Native permission records/bookmarks are host state, not original photos.
    // Retain them and the authenticated owner receipt for exact-byte reinstall.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde_json::json;
    struct Fixture {
        base: PathBuf,
        root: PathBuf,
        verifier: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let mut nonce = [0u8; 16];
            getrandom::fill(&mut nonce).unwrap();
            let base = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!("retained-access-{}", release::digest(&nonce)));
            launch::private_directory(&base, true).unwrap();
            let root = base.join("Install");
            launch::private_directory(&root, true).unwrap();
            let verifier = base.join("fixture-verifier");
            fs::write(
                &verifier,
                b"#!/bin/sh\n/bin/cat >/dev/null\nprintf '%s' '{\"valid\":true}'\n",
            )
            .unwrap();
            fs::set_permissions(&verifier, fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                base,
                root,
                verifier,
            }
        }
        fn source(&self, build: u64, code: &str) -> (PathBuf, Vec<u8>) {
            let source = self.base.join(format!("Source{build}"));
            launch::private_directory(&source, true).unwrap();
            let mut inventory = String::new();
            for (name, bytes) in [
                (
                    "Contents/MacOS/CompanionSourceAccess",
                    code.as_bytes().to_vec(),
                ),
                ("Contents/Info.plist", b"fixed protocol1 identity".to_vec()),
                (
                    "Contents/_CodeSignature/CodeResources",
                    b"sealed fixture metadata".to_vec(),
                ),
                (
                    "Contents/Resources/access-owner-protocol.json",
                    serde_json::to_vec(
                        &json!({"schema":1,"protocol":1,"bundleIdentifier":IDENTIFIER}),
                    )
                    .unwrap(),
                ),
            ] {
                let relative = format!("{PREFIX}/{name}");
                let path = source.join(&relative);
                launch::private_directory(path.parent().unwrap(), true).unwrap();
                fs::write(&path, &bytes).unwrap();
                inventory.push_str(&format!("{}  {relative}\n", release::digest(&bytes)));
            }
            fs::write(source.join("SHA256SUMS.txt"), inventory.as_bytes()).unwrap();
            let version = "0.2.0-beta.99";
            let body=serde_json::to_vec(&json!({"schema":1,"kind":"photofindry-cli-release","version":version,"build":build,"platform":"macos-arm64","dataContract":"photofindry-library-compatible-v1","inventorySHA256":release::digest(inventory.as_bytes()),"archiveSHA256":"a".repeat(64),"archiveBytes":1,"archiveURL":format!("https://github.com/sentrybottale/photofindryapp/releases/download/v{version}/PhotoFindry-{version}-arm64-companion.zip")})).unwrap();
            (
                source,
                serde_json::to_vec(
                    &json!({"payload":STANDARD.encode(body),"signature":STANDARD.encode([0u8;64])}),
                )
                .unwrap(),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    #[test]
    fn ordinary_updates_retain_the_original_authenticated_owner_bytes_and_inode() {
        let f = Fixture::new();
        let (first, envelope) = f.source(10, "owner1");
        let exe = provision(&f.root, &first, &envelope, &f.verifier).unwrap();
        let before = fs::metadata(&exe).unwrap();
        let bytes = fs::read(&exe).unwrap();
        let (second, next) = f.source(11, "different incoming helper");
        assert_eq!(
            provision(&f.root, &second, &next, &f.verifier).unwrap(),
            exe
        );
        assert_eq!(fs::read(&exe).unwrap(), bytes);
        assert_eq!(fs::metadata(&exe).unwrap().ino(), before.ino());
        assert_eq!(owner(&directory(&f.root)).unwrap().origin_build, 10);
    }
    #[test]
    fn missing_or_modified_owner_is_not_silently_regenerated_from_a_new_release() {
        for missing in [true, false] {
            let f = Fixture::new();
            let (first, envelope) = f.source(10, "owner1");
            let exe = provision(&f.root, &first, &envelope, &f.verifier).unwrap();
            if missing {
                fs::remove_file(&exe).unwrap();
            } else {
                fs::write(&exe, "changed").unwrap();
            }
            let (second, next) = f.source(11, "owner2");
            assert!(provision(&f.root, &second, &next, &f.verifier).is_err());
            assert_eq!(owner(&directory(&f.root)).unwrap().origin_build, 10);
            if missing {
                assert!(!exe.exists());
            } else {
                assert_eq!(fs::read(&exe).unwrap(), b"changed");
            }
        }
    }
    #[test]
    fn uninstall_waits_for_only_its_owned_lease_and_preserves_grants_for_exact_reinstall() {
        let f = Fixture::new();
        let (first, envelope) = f.source(10, "owner1");
        provision(&f.root, &first, &envelope, &f.verifier).unwrap();
        let grants = f.root.join("source-access-state");
        launch::private_directory(&grants, true).unwrap();
        fs::write(grants.join("kept.json"), b"private fixture grant").unwrap();
        let held = launch::private_file(&directory(&f.root).join("owner.lock"), true).unwrap();
        assert_eq!(
            unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) },
            0
        );
        assert!(uninstall(&f.root, &f.verifier).is_err());
        assert!(application(&f.root).is_dir());
        drop(held);
        uninstall(&f.root, &f.verifier).unwrap();
        uninstall(&f.root, &f.verifier).unwrap();
        assert!(!application(&f.root).exists());
        assert!(owner(&directory(&f.root)).unwrap().removed);
        assert_eq!(
            fs::read(grants.join("kept.json")).unwrap(),
            b"private fixture grant"
        );
        let (second, next) = f.source(11, "owner2");
        assert!(provision(&f.root, &second, &next, &f.verifier).is_err());
        assert!(!application(&f.root).exists());
        provision(&f.root, &first, &envelope, &f.verifier).unwrap();
        assert_eq!(fs::read(executable(&f.root)).unwrap(), b"owner1");
        assert!(!owner(&directory(&f.root)).unwrap().removed);
    }
    #[test]
    fn linked_unknown_or_lost_authority_is_rejected_without_deleting_anything() {
        let f = Fixture::new();
        let (source, envelope) = f.source(10, "owner1");
        provision(&f.root, &source, &envelope, &f.verifier).unwrap();
        let unexpected = application(&f.root).join("unrelated.txt");
        fs::write(&unexpected, b"keep").unwrap();
        assert!(uninstall(&f.root, &f.verifier).is_err());
        assert_eq!(fs::read(&unexpected).unwrap(), b"keep");
        fs::remove_file(unexpected).unwrap();
        let exe = executable(&f.root);
        fs::remove_file(&exe).unwrap();
        std::os::unix::fs::symlink(
            source
                .join(PREFIX)
                .join("Contents/MacOS/CompanionSourceAccess"),
            &exe,
        )
        .unwrap();
        assert!(verified(&f.root, &f.verifier).is_err());
        let lost = Fixture::new();
        launch::private_directory(&lost.root.join("source-access-state"), true).unwrap();
        let (source, envelope) = lost.source(10, "owner1");
        assert!(provision(&lost.root, &source, &envelope, &lost.verifier).is_err());
        assert!(!directory(&lost.root).exists());
    }
}
