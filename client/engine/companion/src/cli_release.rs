//! Signed CLI payload admission. The hosting server supplies bytes, never trust.
use super::launch;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub type Result<T> = std::result::Result<T, String>;
pub const FEED: &str = "https://photofindry.com/updates/cli/release.json";
pub const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TOTAL: u64 = 8 * 1024 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Release {
    pub schema: u32,
    pub kind: String,
    pub version: String,
    pub build: u64,
    pub platform: String,
    pub data_contract: String,
    #[serde(rename = "inventorySHA256")]
    pub inventory_sha256: String,
    #[serde(rename = "archiveSHA256")]
    pub archive_sha256: String,
    pub archive_bytes: u64,
    #[serde(rename = "archiveURL")]
    pub archive_url: String,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn sha_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn version(s: &str) -> bool {
    let Some((v, beta)) = s.split_once("-beta.") else {
        return false;
    };
    let parts: Vec<_> = v.split('.').collect();
    s.len() <= 64
        && parts.len() == 3
        && parts
            .iter()
            .chain(std::iter::once(&beta))
            .all(|p| !p.is_empty() && p.len() <= 8 && p.bytes().all(|b| b.is_ascii_digit()))
}
impl Release {
    pub fn validate(&self) -> Result<()> {
        if self.schema!=1||self.kind!="photofindry-cli-release"||self.platform!="macos-arm64"||self.data_contract!="photofindry-library-compatible-v1"
   ||self.build==0||self.build>1_000_000_000||!version(&self.version)||!sha_valid(&self.inventory_sha256)||!sha_valid(&self.archive_sha256)
   ||self.archive_bytes==0||self.archive_bytes>MAX_ARCHIVE
   ||self.archive_url!=format!("https://github.com/sentrybottale/photofindryapp/releases/download/v{0}/PhotoFindry-{0}-arm64-companion.zip",self.version) {
    return Err("The signed update describes an unsupported package. Keep the current version and check again later.".into());
  }
        Ok(())
    }
}
pub fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let path = launch::canonical(path, false).or_else(|_| -> Result<PathBuf> {
        // Metadata is not executable; apply the same ancestry check to its parent.
        let parent = path.parent().ok_or("Missing metadata parent")?;
        launch::canonical(parent, true)?;
        if fs::symlink_metadata(path)
            .map_err(|_| "Release metadata is unavailable")?
            .file_type()
            .is_symlink()
        {
            return Err("Linked release metadata is not accepted".into());
        }
        Ok(path.to_path_buf())
    })?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|_| "Cannot open release metadata")?;
    let before = file
        .metadata()
        .map_err(|_| "Cannot inspect release metadata")?;
    if !before.is_file()
        || before.nlink() != 1
        || before.len() > limit
        || before.mode() & 0o022 != 0
        || ![0, unsafe { libc::geteuid() }].contains(&before.uid())
    {
        return Err("Unsafe or oversized release file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read release file")?;
    let after = fs::symlink_metadata(&path).map_err(|_| "Release file disappeared")?;
    if bytes.len() as u64 > limit
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.mtime() != after.mtime()
    {
        return Err("Release file changed during verification".into());
    }
    Ok(bytes)
}
pub fn verify(envelope: &[u8], helper: &Path) -> Result<Release> {
    if envelope.len() > 131072 {
        return Err("Release envelope exceeds its bound".into());
    }
    let value: Envelope =
        serde_json::from_slice(envelope).map_err(|_| "The update envelope is invalid")?;
    let body = STANDARD
        .decode(&value.payload)
        .map_err(|_| "The update payload encoding is invalid")?;
    if body.len() > 65536
        || STANDARD
            .decode(&value.signature)
            .map_err(|_| "Invalid update signature")?
            .len()
            != 64
    {
        return Err("Invalid update signature or payload size".into());
    }
    let helper = launch::canonical(helper, false)?;
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            "The installed signature verifier is unavailable. Reinstall the verified companion."
        })?;
    let input = child
        .stdin
        .take()
        .ok_or("Cannot contact signature verifier")?;
    // Write on a separate thread so a failed helper cannot deadlock the caller.
    let bytes = serde_json::to_vec(&value).map_err(|_| "Cannot encode signature request")?;
    let writer = std::thread::spawn(move || {
        let mut input = input;
        input.write_all(&bytes)
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "Cannot check signature verifier")?
        {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.join();
            return Err("The signature verifier timed out. Current software is unchanged.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let _ = writer.join();
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .ok_or("Missing signature response")?
        .take(129)
        .read_to_end(&mut output)
        .map_err(|_| "Cannot read signature result")?;
    if !status.success()
        || output.len() > 128
        || serde_json::from_slice::<serde_json::Value>(&output).ok()
            != Some(serde_json::json!({"valid":true}))
    {
        return Err(
            "The update signature is invalid. Current software is unchanged; try again later."
                .into(),
        );
    }
    let result: Release =
        serde_json::from_slice(&body).map_err(|_| "The signed release metadata is invalid")?;
    result.validate()?;
    Ok(result)
}
pub fn relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(char::is_control)
        || value.contains('\\')
        || value
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.starts_with('.'))
        || path.components().count() > 16
        || path
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err("Unsafe package entry".into());
    }
    Ok(path.to_path_buf())
}
pub fn inventory(root: &Path, release: &Release) -> Result<BTreeMap<PathBuf, String>> {
    let raw = read(&root.join("SHA256SUMS.txt"), 2 * 1024 * 1024)?;
    if digest(&raw) != release.inventory_sha256 {
        return Err("The package inventory does not match its publisher signature".into());
    }
    let text = std::str::from_utf8(&raw).map_err(|_| "Invalid package inventory text")?;
    let mut expected = BTreeMap::new();
    for line in text.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or("Invalid package inventory row")?;
        let p = relative(name)?;
        if !sha_valid(hash)
            || name == "SHA256SUMS.txt"
            || expected.insert(p, hash.to_owned()).is_some()
            || expected.len() > 8192
        {
            return Err("Invalid or duplicate package inventory entry".into());
        }
    }
    for required in [
        "bin/photofindry",
        "bin/CompanionReleaseVerify",
        "bin/CompanionLauncher",
        "bin/CompanionLauncher.app/Contents/Info.plist",
        "runtime/PhotoWorker",
        "runtime/PhotoEngine",
        "LICENSE",
        "README.txt",
    ] {
        if !expected.contains_key(Path::new(required)) {
            return Err("The signed companion package is incomplete".into());
        }
    }
    Ok(expected)
}
pub fn verify_package(root: &Path, release: &Release) -> Result<()> {
    launch::canonical(root, true)?;
    let expected = inventory(root, release)?;
    let mut directories = BTreeSet::new();
    for p in expected.keys() {
        for parent in p.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
            directories.insert(parent.to_path_buf());
        }
    }
    let mut pending = vec![root.to_path_buf()];
    let mut found = BTreeSet::new();
    let mut total = 0u64;
    while let Some(dir) = pending.pop() {
        for e in fs::read_dir(dir).map_err(|_| "Cannot inspect staged package")? {
            let p = e.map_err(|_| "Cannot inspect package entry")?.path();
            let name = p
                .strip_prefix(root)
                .map_err(|_| "Package escaped its root")?
                .to_path_buf();
            let m = fs::symlink_metadata(&p).map_err(|_| "Package entry disappeared")?;
            if m.file_type().is_symlink()
                || m.mode() & 0o022 != 0
                || ![0, unsafe { libc::geteuid() }].contains(&m.uid())
            {
                return Err("Package contains links or unsafe permissions".into());
            }
            if m.is_dir() {
                if !directories.contains(&name) {
                    return Err("Package has an unexpected directory".into());
                }
                pending.push(p);
                continue;
            }
            if !m.is_file() || m.nlink() != 1 || m.len() > MAX_FILE {
                return Err("Package has a special, linked or oversized file".into());
            }
            total = total.checked_add(m.len()).ok_or("Package too large")?;
            if total > MAX_TOTAL {
                return Err("Package too large".into());
            }
            if name == Path::new("SHA256SUMS.txt") {
                continue;
            }
            let hash = expected
                .get(&name)
                .ok_or("Package has an unexpected file")?;
            if file_hash(&p, MAX_FILE)? != *hash {
                return Err("Package checksum mismatch. Current software is unchanged.".into());
            }
            found.insert(name);
        }
    }
    if found.len() != expected.len() {
        return Err("Package files are missing".into());
    }
    Ok(())
}
pub fn file_hash(path: &Path, limit: u64) -> Result<String> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cannot read package file")?;
    let m = f.metadata().map_err(|_| "Cannot inspect package file")?;
    if !m.is_file() || m.nlink() != 1 || m.len() > limit {
        return Err("Invalid package file".into());
    }
    let mut h = Sha256::new();
    let mut size = 0;
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|_| "Cannot hash package file")?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > limit {
            return Err("Package grew during verification".into());
        }
        h.update(&buf[..n]);
    }
    let after = fs::symlink_metadata(path).map_err(|_| "Package changed during verification")?;
    if size != m.len()
        || m.dev() != after.dev()
        || m.ino() != after.ino()
        || m.len() != after.len()
        || m.mtime() != after.mtime()
        || m.mtime_nsec() != after.mtime_nsec()
    {
        return Err("Package changed during verification".into());
    }
    Ok(format!("{:x}", h.finalize()))
}
pub fn copy_package(source: &Path, target: &Path, release: &Release) -> Result<()> {
    verify_package(source, release)?;
    let expected = inventory(source, release)?;
    if target.exists() {
        return verify_package(target, release);
    }
    fs::create_dir(target).map_err(|_| "Cannot create private installation stage")?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot protect installation stage")?;
    for name in expected
        .keys()
        .chain(std::iter::once(&PathBuf::from("SHA256SUMS.txt")))
    {
        let from = source.join(name);
        let to = target.join(name);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|_| "Cannot create package directory")?;
        }
        let input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&from)
            .map_err(|_| "Cannot read source package")?;
        let metadata = input
            .metadata()
            .map_err(|_| "Cannot inspect source package")?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_FILE {
            return Err("Source package changed".into());
        }
        let mode = if metadata.mode() & 0o111 != 0 {
            0o700
        } else {
            0o600
        };
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(to)
            .map_err(|_| "Cannot stage package file")?;
        let n = std::io::copy(&mut input.take(metadata.len() + 1), &mut output)
            .map_err(|_| "Package copy failed; old version is kept")?;
        if n != metadata.len() {
            return Err("Source package changed while copying".into());
        }
        output
            .sync_all()
            .map_err(|_| "Cannot flush staged package")?;
    }
    verify_package(target, release)
}
// libarchive is supplied by macOS. Only already authenticated archives reach it.
#[link(name = "archive")]
unsafe extern "C" {
    fn archive_read_new() -> *mut libc::c_void;
    fn archive_read_support_format_zip(a: *mut libc::c_void) -> i32;
    fn archive_read_support_filter_all(a: *mut libc::c_void) -> i32;
    fn archive_read_open_filename(a: *mut libc::c_void, p: *const libc::c_char, b: usize) -> i32;
    fn archive_read_next_header(a: *mut libc::c_void, e: *mut *mut libc::c_void) -> i32;
    fn archive_entry_pathname(e: *mut libc::c_void) -> *const libc::c_char;
    fn archive_entry_filetype(e: *mut libc::c_void) -> libc::mode_t;
    fn archive_entry_size(e: *mut libc::c_void) -> i64;
    fn archive_entry_perm(e: *mut libc::c_void) -> libc::mode_t;
    fn archive_entry_symlink(e: *mut libc::c_void) -> *const libc::c_char;
    fn archive_entry_hardlink(e: *mut libc::c_void) -> *const libc::c_char;
    fn archive_read_data(a: *mut libc::c_void, p: *mut libc::c_void, n: usize) -> isize;
    fn archive_read_free(a: *mut libc::c_void) -> i32;
}
pub fn extract(archive: &Path, target: &Path, release: &Release) -> Result<()> {
    if fs::metadata(archive)
        .map_err(|_| "Missing downloaded update")?
        .len()
        != release.archive_bytes
        || file_hash(archive, MAX_ARCHIVE)? != release.archive_sha256
    {
        return Err(
            "The downloaded update failed its signed checksum. Current software is kept.".into(),
        );
    }
    if target.exists() {
        return Err("Choose a fresh private extraction directory".into());
    }
    fs::create_dir(target).map_err(|_| "Cannot stage downloaded update")?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot protect update stage")?;
    struct Reader(*mut libc::c_void);
    impl Drop for Reader {
        fn drop(&mut self) {
            unsafe {
                archive_read_free(self.0);
            }
        }
    }
    let a = Reader(unsafe { archive_read_new() });
    if a.0.is_null() {
        return Err("Cannot initialize archive reader".into());
    }
    let path = std::ffi::CString::new(archive.as_os_str().as_encoded_bytes())
        .map_err(|_| "Invalid archive path")?;
    unsafe {
        archive_read_support_format_zip(a.0);
        archive_read_support_filter_all(a.0);
    }
    if unsafe { archive_read_open_filename(a.0, path.as_ptr(), 65536) } != 0 {
        return Err("Cannot open signed update archive".into());
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let mut entry = std::ptr::null_mut();
        let status = unsafe { archive_read_next_header(a.0, &mut entry) };
        if status == 1 {
            break;
        }
        if status != 0 || entry.is_null() {
            return Err("Invalid archive header".into());
        }
        let raw = unsafe { archive_entry_pathname(entry) };
        if raw.is_null() {
            return Err("Missing archive path".into());
        }
        let name = unsafe { std::ffi::CStr::from_ptr(raw) }
            .to_str()
            .map_err(|_| "Archive paths must be UTF-8")?;
        let dir = unsafe { archive_entry_filetype(entry) } == libc::S_IFDIR;
        let relative = relative(if dir {
            name.strip_suffix('/').unwrap_or(name)
        } else {
            name
        })?;
        if !names.insert(relative.clone())
            || names.len() > 8192
            || !unsafe { archive_entry_symlink(entry) }.is_null()
            || !unsafe { archive_entry_hardlink(entry) }.is_null()
        {
            return Err("Duplicate or linked archive entry".into());
        }
        let output = target.join(relative);
        if dir {
            fs::create_dir_all(&output).map_err(|_| "Cannot create archive directory")?;
            continue;
        }
        if unsafe { archive_entry_filetype(entry) } != libc::S_IFREG {
            return Err("Special archive entries are not allowed".into());
        }
        let size = unsafe { archive_entry_size(entry) };
        if size < 0 || size as u64 > MAX_FILE {
            return Err("Archive file exceeds its bound".into());
        }
        total = total.checked_add(size as u64).ok_or("Archive too large")?;
        if total > MAX_TOTAL {
            return Err("Archive exceeds its bound".into());
        }
        fs::create_dir_all(output.parent().ok_or("Invalid archive parent")?)
            .map_err(|_| "Cannot stage archive directories")?;
        let mode = if unsafe { archive_entry_perm(entry) } & 0o111 != 0 {
            0o700
        } else {
            0o600
        };
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(output)
            .map_err(|_| "Unsafe archive destination")?;
        let mut n = 0;
        loop {
            let got = unsafe { archive_read_data(a.0, buffer.as_mut_ptr().cast(), buffer.len()) };
            if got < 0 {
                return Err("Archive decompression failed".into());
            }
            if got == 0 {
                break;
            }
            n += got as u64;
            if n > size as u64 {
                return Err("Archive file grew beyond its declared size".into());
            }
            file.write_all(&buffer[..got as usize])
                .map_err(|_| "Cannot write update stage")?;
        }
        if n != size as u64 {
            return Err("Truncated archive file".into());
        }
        file.sync_all().map_err(|_| "Cannot flush update stage")?;
    }
    verify_package(target, release)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_links_traversal_and_duplicates_never_escape_staging() {
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("pf-cli-malformed-{}", std::process::id()));
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let archives=["UEsDBBQAAAAAAAAAIQD7OSuCAwAAAAMAAAAKAAAALi4vb3V0c2lkZWJhZFBLAQIUAxQAAAAAAAAAIQD7OSuCAwAAAAMAAAAKAAAAAAAAAAAAAACkgQAAAAAuLi9vdXRzaWRlUEsFBgAAAAABAAEAOAAAACsAAAAAAA==","UEsDBBQAAAAAAAAAIQD7OSuCAwAAAAMAAAAIAAAAYmluL2xpbmtiYWRQSwECFAMUAAAAAAAAACEA+zkrggMAAAADAAAACAAAAAAAAAAAAAAA/6EAAAAAYmluL2xpbmtQSwUGAAAAAAEAAQA2AAAAKQAAAAAA","UEsDBBQAAAAAAAAAIQD7OSuCAwAAAAMAAAAKAAAAYmluL3JlcGVhdGJhZFBLAwQUAAAAAAAAACEAG4FHEgkAAAAJAAAACgAAAGJpbi9yZXBlYXRkdXBsaWNhdGVQSwECFAMUAAAAAAAAACEAG4FHEgkAAAAJAAAACgAAAAAAAAAAAAAApIErAAAAYmluL3JlcGVhdFBLAQIUAxQAAAAAAAAAIQAbgUcSCQAAAAkAAAAKAAAAAAAAAAAAAACkgSsAAABiaW4vcmVwZWF0UEsFBgAAAAACAAIAcAAAAFwAAAAAAA=="];
        for (index, encoded) in archives.iter().enumerate() {
            let bytes = STANDARD.decode(encoded).unwrap();
            let archive = base.join(format!("{index}.zip"));
            fs::write(&archive, &bytes).unwrap();
            let release = Release {
                schema: 1,
                kind: "photofindry-cli-release".into(),
                version: "0.2.0-beta.15".into(),
                build: 113,
                platform: "macos-arm64".into(),
                data_contract: "photofindry-library-compatible-v1".into(),
                inventory_sha256: "0".repeat(64),
                archive_sha256: digest(&bytes),
                archive_bytes: bytes.len() as u64,
                archive_url: "unused in the extraction unit".into(),
            };
            assert!(extract(&archive, &base.join(format!("stage-{index}")), &release).is_err());
            assert!(!base.join("outside").exists());
        }
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn publisher_field_names_and_origin_are_exact() {
        let body = serde_json::json!({"schema":1,"kind":"photofindry-cli-release","version":"0.2.0-beta.15","build":113,
            "platform":"macos-arm64","dataContract":"photofindry-library-compatible-v1","inventorySHA256":"a".repeat(64),
            "archiveSHA256":"b".repeat(64),"archiveBytes":100,"archiveURL":"https://github.com/sentrybottale/photofindryapp/releases/download/v0.2.0-beta.15/PhotoFindry-0.2.0-beta.15-arm64-companion.zip"});
        let mut release: Release = serde_json::from_value(body.clone()).unwrap();
        release.validate().unwrap();
        assert_eq!(serde_json::to_value(&release).unwrap(), body);
        release.archive_url = "https://photofindry.com/untrusted.zip".into();
        assert!(release.validate().is_err());
    }
    #[test]
    #[ignore = "Requires an explicitly signed private candidate; extracts only to a disposable directory"]
    fn signed_candidate_archive_and_tampering() {
        let fixture =
            PathBuf::from(std::env::var("PHOTOFINDRY_CLI_SIGNED_FIXTURE").expect("signed fixture"));
        let helper =
            PathBuf::from(std::env::var("PHOTOFINDRY_CLI_VERIFIER").expect("installed verifier"));
        let envelope = read(&fixture.join("release.json"), 131072).unwrap();
        let signed = verify(&envelope, &helper).unwrap();
        let mut wrong: Envelope = serde_json::from_slice(&envelope).unwrap();
        wrong.signature = STANDARD.encode([0u8; 64]);
        assert!(verify(&serde_json::to_vec(&wrong).unwrap(), &helper)
            .unwrap_err()
            .contains("signature"));
        let mut changed: Envelope = serde_json::from_slice(&envelope).unwrap();
        let mut body = STANDARD.decode(&changed.payload).unwrap();
        body[0] = b'[';
        changed.payload = STANDARD.encode(body);
        assert!(verify(&serde_json::to_vec(&changed).unwrap(), &helper).is_err());
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("pf-cli-extract-{}", std::process::id()));
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let zip = fixture.join(format!(
            "PhotoFindry-{}-arm64-companion.zip",
            signed.version
        ));
        let mut corrupt = signed.clone();
        corrupt.archive_sha256 = "0".repeat(64);
        assert!(extract(&zip, &base.join("rejected"), &corrupt).is_err());
        assert!(!base.join("rejected").exists());
        extract(&zip, &base.join("package"), &signed).unwrap();
        fs::write(base.join("package/README.txt"), b"tampered").unwrap();
        assert!(verify_package(&base.join("package"), &signed).is_err());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn package_paths_are_never_extraction_authority() {
        for p in [
            "../x",
            "/x",
            "x//y",
            "x/./y",
            "x\\y",
            ".git/config",
            "x/.hidden",
        ] {
            assert!(relative(p).is_err(), "{p}")
        }
        assert_eq!(
            relative("bin/photofindry").unwrap(),
            PathBuf::from("bin/photofindry")
        );
    }
    #[test]
    fn versions_and_hashes_are_bounded() {
        assert!(version("0.2.0-beta.15"));
        for v in ["../../x", "0.2.0-beta.1?q=x", "0.2.0", "0.2.0-beta."] {
            assert!(!version(v))
        }
        assert!(!sha_valid(&"A".repeat(64)));
        assert!(sha_valid(&"a".repeat(64)));
    }
}
