//! Explicit, closed-library host handover. This module never opens a customer's
//! originals, migrates a schema, starts a job, or reads native UserDefaults.
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, OsStr},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::{DirBuilderExt, MetadataExt, OpenOptionsExt}},
    },
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

type Result<T> = std::result::Result<T, String>;
const MAX_DEPTH: usize = 48;
const MAX_ENTRIES: u64 = 1_000_000;
const MAX_DIR_ENTRIES: usize = 100_000;
const MAX_MARKER: u64 = 4096;
const MAX_HEADER: u64 = 2 * 1024 * 1024;
const RECEIPT: &str = ".photofindry-handover.json";
const LOCK: &str = "worker.lock";
const ADMISSION: &str = "companion-library.json";
const RESTORE_INTENT: &str = "companion-restore-intent.json";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    entries: u64,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Receipt {
    format: String,
    version: u64,
    pub library_id: String,
    pub entries: u64,
    pub bytes: u64,
    pub sha256: String,
    #[serde(skip)]
    pub rollback: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Admission {
    format: String,
    version: u64,
    pub library_id: String,
    pub origin: String,
    rollback_sha256: Option<String>,
    #[serde(skip)]
    pub recovery: Option<Recovery>,
}
#[derive(Debug, Clone)]
pub enum Recovery {
    Restore { target_id: String },
    Source,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RestoreIntent {
    format: String,
    version: u64,
    old_library_id: String,
    target_library_id: String,
    backup_catalog_sha256: String,
    #[serde(default)]
    stage_confirmed: bool,
}

struct Summary {
    hash: Sha256,
    entries: u64,
    bytes: u64,
}
impl Summary {
    fn new() -> Self { Self { hash: Sha256::new(), entries: 0, bytes: 0 } }
    fn add(&mut self, path: &Path, kind: u8, size: u64, digest: Option<&[u8]>) -> Result<()> {
        let name = path.to_str().filter(|s| !s.is_empty() && s.len() <= 16_384)
            .ok_or("A local library entry has an unsupported name.")?;
        self.entries = self.entries.checked_add(1).filter(|n| *n <= MAX_ENTRIES)
            .ok_or("The local library contains too many entries for this handover.")?;
        self.bytes = self.bytes.checked_add(size)
            .ok_or("The local library size exceeds the handover bound.")?;
        self.hash.update([kind]);
        self.hash.update((name.len() as u32).to_le_bytes());
        self.hash.update(name.as_bytes());
        self.hash.update(size.to_le_bytes());
        if let Some(digest) = digest { self.hash.update(digest); }
        Ok(())
    }
    fn finish(self) -> Fingerprint {
        Fingerprint { entries: self.entries, bytes: self.bytes,
            sha256: format!("{:x}", self.hash.finalize()) }
    }
}

fn metadata_same(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino() && a.len() == b.len()
        && a.mtime() == b.mtime() && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime() && a.ctime_nsec() == b.ctime_nsec()
}
fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new().read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path).map_err(|_| "The local library directory is unavailable or is a symlink.")?;
    let metadata = file.metadata().map_err(|_| "Cannot inspect the local library directory.")?;
    if !metadata.is_dir() { return Err("The local library root must be a directory.".into()); }
    Ok(file)
}
fn same_directory(path: &Path, held: &File) -> Result<()> {
    let current = fs::symlink_metadata(path)
        .map_err(|_| "A handover directory changed during the copy.")?;
    let metadata = held.metadata().map_err(|_| "Cannot inspect a held handover directory.")?;
    if current.file_type().is_symlink() || !current.is_dir()
        || current.dev() != metadata.dev() || current.ino() != metadata.ino() {
        return Err("A handover directory changed during the copy.".into());
    }
    Ok(())
}
fn safe_ancestors(path: &Path, require_owner: bool) -> Result<()> {
    if !path.is_absolute() { return Err("Choose an absolute handover directory.".into()); }
    let uid = unsafe { libc::geteuid() };
    for ancestor in path.ancestors().filter(|part| part.is_absolute()) {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|_| "Cannot inspect a handover directory ancestor.")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir()
            || metadata.mode() & 0o022 != 0
            || (metadata.uid() != uid && metadata.uid() != 0) {
            return Err("Choose a library and rollback parent without shared writable or symlink ancestors.".into());
        }
        if require_owner && ancestor == path && metadata.uid() != uid {
            return Err("The rollback parent must be owned by the current user.".into());
        }
    }
    Ok(())
}
fn open_child(parent: &File, name: &OsStr, directory: bool) -> Result<File> {
    let name = CString::new(name.as_bytes()).map_err(|_| "Invalid local library entry name.")?;
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC
        | if directory { libc::O_DIRECTORY } else { libc::O_NONBLOCK };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 { return Err("A local library entry changed or is a symlink.".into()); }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn sorted_names(path: &Path, held: &File) -> Result<Vec<std::ffi::OsString>> {
    same_directory(path, held)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(path).map_err(|_| "Cannot list the local library.")? {
        let name = entry.map_err(|_| "Cannot list a local library entry.")?.file_name();
        if names.len() >= MAX_DIR_ENTRIES {
            return Err("A local library directory has too many entries for bounded handover.".into());
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}
fn skip_root(name: &OsStr, root: bool, skip_locks: bool, skip_receipt: bool) -> bool {
    root && ((skip_locks && (name == OsStr::new(LOCK) || name == OsStr::new(".companion.lock")))
        || (skip_receipt && name == OsStr::new(RECEIPT)))
}
fn ensure_absent(path: &Path, message: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(message.into()),
    }
}
fn sqlite_rollback_journal(name: &OsStr) -> bool {
    let bytes = name.as_bytes();
    bytes.ends_with(b".sqlite-journal")
}
fn sqlite_shared_memory(name: &OsStr) -> bool {
    name.as_bytes().ends_with(b".sqlite-shm")
}
fn check_cancel(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err("Library handover cancelled; any incomplete private copy has no valid receipt.".into())
    } else { Ok(()) }
}

fn walk(root: &Path, directory: &File, relative: &Path, destination: Option<&Path>,
        summary: &mut Summary, depth: usize, skip_locks: bool, skip_receipt: bool,
        cancelled: &AtomicBool) -> Result<()> {
    check_cancel(cancelled)?;
    if depth > MAX_DEPTH { return Err("The local library directory tree is too deep.".into()); }
    let path = root.join(relative);
    for name in sorted_names(&path, directory)? {
        check_cancel(cancelled)?;
        if skip_root(&name, depth == 0, skip_locks, skip_receipt) { continue; }
        if sqlite_shared_memory(&name) {
            let metadata = fs::symlink_metadata(root.join(relative).join(&name))
                .map_err(|_| "Cannot inspect disposable SQLite shared memory.")?;
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
                return Err("An unsafe SQLite shared-memory sidecar needs review before handover.".into());
            }
            // SQLite rewrites -shm on a read-only WAL view. It is disposable
            // coordination state, so neither copy nor receipt includes it.
            continue;
        }
        if sqlite_rollback_journal(&name) {
            return Err("A SQLite rollback journal needs native recovery before handover.".into());
        }
        let child_relative = relative.join(&name);
        let child_path = root.join(&child_relative);
        let before = fs::symlink_metadata(&child_path)
            .map_err(|_| "A local library entry changed during handover.")?;
        if before.file_type().is_symlink() { return Err("A symlink in the local library needs explicit review before handover.".into()); }
        if before.is_dir() {
            let child = open_child(directory, &name, true)?;
            let current = child.metadata().map_err(|_| "Cannot inspect a local library folder.")?;
            if current.dev() != before.dev() || current.ino() != before.ino() {
                return Err("A local library folder changed during handover.".into());
            }
            summary.add(&child_relative, b'd', 0, None)?;
            if let Some(target) = destination {
                fs::DirBuilder::new().mode(0o700).create(target.join(&child_relative))
                    .map_err(|_| "Cannot create a private rollback folder.")?;
            }
            walk(root, &child, &child_relative, destination, summary, depth + 1,
                skip_locks, skip_receipt, cancelled)?;
        } else if before.is_file() {
            if before.nlink() != 1 { return Err("A hard-linked local library file needs explicit review before handover.".into()); }
            let mut child = open_child(directory, &name, false)?;
            let opened = child.metadata().map_err(|_| "Cannot inspect a local library file.")?;
            if !metadata_same(&before, &opened) || !opened.is_file() || opened.nlink() != 1 {
                return Err("A local library file changed during handover.".into());
            }
            let mut output = match destination {
                Some(target) => Some(OpenOptions::new().write(true).create_new(true).mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(target.join(&child_relative))
                    .map_err(|_| "Cannot create a private rollback file.")?),
                None => None,
            };
            let mut hash = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            let mut read = 0u64;
            loop {
                check_cancel(cancelled)?;
                let amount = child.read(&mut buffer)
                    .map_err(|_| "Cannot read a local library file for handover.")?;
                if amount == 0 { break; }
                read = read.checked_add(amount as u64)
                    .ok_or("A local library file exceeds the handover bound.")?;
                if read > before.len() { return Err("A local library file grew during handover.".into()); }
                hash.update(&buffer[..amount]);
                if let Some(file) = output.as_mut() {
                    file.write_all(&buffer[..amount])
                        .map_err(|_| "Cannot write a private rollback file.")?;
                }
            }
            let after = child.metadata().map_err(|_| "Cannot recheck a local library file.")?;
            if read != before.len() || !metadata_same(&before, &after) {
                return Err("A local library file changed during handover.".into());
            }
            if let Some(file) = output {
                file.sync_all().map_err(|_| "Cannot flush a private rollback file.")?;
            }
            summary.add(&child_relative, b'f', read, Some(hash.finalize().as_slice()))?;
        } else {
            return Err("A special local library file needs explicit review before handover.".into());
        }
    }
    if let Some(target) = destination {
        open_directory(&target.join(relative))?.sync_all()
            .map_err(|_| "Cannot flush a private rollback folder.")?;
    }
    Ok(())
}

struct Lease {
    marker: PathBuf,
    token: String,
    device: u64,
    inode: u64,
    published: bool,
    directory: File,
}
impl Lease {
    fn acquire(data: &Path) -> Result<Self> {
        let directory = open_directory(data)?;
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another PhotoFindry host still owns this library. Close it before handover.".into());
        }
        let marker = data.join(LOCK);
        match fs::symlink_metadata(&marker) {
            Ok(_) => return Err("An existing worker marker needs explicit recovery before handover; it will not be removed automatically.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot establish that the old worker marker is absent.".into()),
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| "Cannot allocate handover ownership.")?;
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&marker).map_err(|_| "The library acquired a worker marker during handover.")?;
        let metadata = file.metadata().map_err(|_| "Cannot verify handover ownership.")?;
        let mut lease = Self { marker, token, device: metadata.dev(), inode: metadata.ino(),
            published: false, directory };
        let bytes = serde_json::to_vec(&json!({"pid":std::process::id(),
            "token":lease.token,"implementation":"companion-handover","readOnly":true}))
            .map_err(|_| "Cannot encode handover ownership.")?;
        file.write_all(&bytes).and_then(|_| file.sync_all())
            .map_err(|_| "Cannot publish handover ownership.")?;
        lease.published = true;
        lease.directory.sync_all().map_err(|_| "Cannot flush handover ownership.")?;
        Ok(lease)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let Ok(mut file) = OpenOptions::new().read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&self.marker) else { return; };
        let Ok(metadata) = file.metadata() else { return; };
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.dev() != self.device
            || metadata.ino() != self.inode || metadata.len() > MAX_MARKER { return; }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() { return; }
        let owned = !self.published || serde_json::from_slice::<Value>(&bytes)
            .is_ok_and(|value| value["pid"] == std::process::id() && value["token"] == self.token);
        if owned && fs::symlink_metadata(&self.marker).is_ok_and(|now|
            !now.file_type().is_symlink() && now.dev() == self.device && now.ino() == self.inode) {
            let _ = fs::remove_file(&self.marker);
            let _ = self.directory.sync_all();
        }
    }
}

fn small_json(path: &Path, limit: u64) -> Result<Value> {
    let before = fs::symlink_metadata(path).map_err(|_| "A current-format library marker is missing.")?;
    if !before.is_file() || before.file_type().is_symlink() || before.nlink() != 1 || before.len() > limit {
        return Err("Unsafe or oversized library marker. Keep the old app for recovery.".into());
    }
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path).map_err(|_| "Cannot read a library marker.")?;
    let opened = file.metadata().map_err(|_| "Cannot inspect a library marker.")?;
    if !metadata_same(&before, &opened) { return Err("A library marker changed during handover.".into()); }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|_| "Cannot read a library marker.")?;
    if bytes.len() as u64 > limit { return Err("A library marker exceeds its size limit.".into()); }
    let after = fs::symlink_metadata(path).map_err(|_| "A library marker changed during handover.")?;
    if !metadata_same(&before, &after) { return Err("A library marker changed during handover.".into()); }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid current-format library marker.".into())
}
fn current_marker(data: &Path, name: &str, database: &str) -> Result<()> {
    let marker = small_json(&data.join(name), MAX_MARKER)?;
    if marker["version"] != 2 || marker["storage"] != "sqlite" || marker["database"] != database {
        return Err("This library needs a separately reviewed legacy migration in the old app.".into());
    }
    Ok(())
}
fn database_uri(path: &Path, immutable: bool) -> String {
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    // Library stores use a WAL-aware view. A fully checked portable backup
    // without sidecars may use immutable reads to avoid touching that package.
    uri.push_str(if immutable { "?immutable=1" } else { "?mode=ro" });
    uri
}
fn database(data: &Path, name: &str, version: i64, required: bool, cold: bool) -> Result<Option<Connection>> {
    database_with_cancel(data, name, version, required, cold, None)
}
fn database_query<T>(db: &Connection, cancelled: Option<&AtomicBool>, query: impl FnOnce() -> rusqlite::Result<T>, failure: &str) -> Result<T> {
    let Some(cancelled) = cancelled else { return query().map_err(|_| failure.to_owned()); };
    check_cancel(cancelled)?;
    let done = AtomicBool::new(false);
    struct Finished<'a>(&'a AtomicBool);
    impl Drop for Finished<'_> { fn drop(&mut self) { self.0.store(true, Ordering::Release); } }
    let result = std::thread::scope(|scope| {
        let interrupt = db.get_interrupt_handle();
        let done = &done;
        let watcher = scope.spawn(move || {
            while !done.load(Ordering::Acquire) {
                if cancelled.load(Ordering::Acquire) { interrupt.interrupt(); }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        let result = { let _finished = Finished(done); query() };
        let _ = watcher.join();
        result
    });
    check_cancel(cancelled)?;
    result.map_err(|_| failure.to_owned())
}
fn database_with_cancel(data: &Path, name: &str, version: i64, required: bool, cold: bool, cancelled: Option<&AtomicBool>) -> Result<Option<Connection>> {
    if let Some(cancelled) = cancelled { check_cancel(cancelled)?; }
    let path = data.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(_) => return Err(format!("The required current-format {name} is unavailable.")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 || metadata.len() == 0 {
        return Err(format!("The current-format {name} is unsafe or empty."));
    }
    if cold {
        ensure_absent(&data.join(format!("{name}-journal")),
            &format!("Recover the {name} rollback journal in the old app before handover."))?;
        for suffix in ["-wal", "-shm"] {
            let sidecar = data.join(format!("{name}{suffix}"));
            match fs::symlink_metadata(&sidecar) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink()
                    && metadata.nlink() == 1 => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(format!("The {name} SQLite sidecar is unsafe for a cold copy.")),
            }
        }
    }
    // The lease excludes cooperating writers. A read-only SQLite connection
    // includes committed WAL frames; its shared-memory bookkeeping is copied
    // only after this inspection connection closes.
    let uri = database_uri(&path, false);
    let db = Connection::open_with_flags(Path::new(&uri),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| format!("Cannot inspect current-format {name}."))?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA query_only=ON;PRAGMA temp_store=MEMORY;")
        .map_err(|_| format!("Cannot safely inspect current-format {name}."))?;
    let actual: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| format!("Cannot read the schema version of {name}."))?;
    if actual != version { return Err(format!("Unsupported {name} schema; use the old app to prepare a compatible copy.")); }
    // The first closed-library admission checks every database page before
    // copying. Routine admitted opens check the known schema and identity, not
    // the entire corpus again; SQLite still validates pages it actually reads.
    if cold {
        let check: String = database_query(&db, cancelled,
            || db.query_row("PRAGMA quick_check", [], |row| row.get(0)), &format!("Cannot check current-format {name}."))?;
        if check != "ok" { return Err(format!("The {name} database needs recovery before handover.")); }
    }
    let after = fs::symlink_metadata(&path).map_err(|_| format!("The {name} changed during handover."))?;
    if !metadata_same(&metadata, &after) { return Err(format!("The {name} changed during handover.")); }
    Ok(Some(db))
}
fn table(db: &Connection, name: &str) -> Result<()> {
    let exists: Option<String> = db.query_row("SELECT type FROM sqlite_master WHERE name=?", [name], |row| row.get(0))
        .optional().map_err(|_| "Cannot inspect current library tables.")?;
    if exists.as_deref() != Some("table") { return Err("The current library is missing a required table.".into()); }
    Ok(())
}
fn column(db: &Connection, table_name: &str, column_name: &str) -> Result<()> {
    let query = format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table_name}') WHERE name=?)");
    let exists: bool = db.query_row(&query, [column_name], |row| row.get(0))
        .map_err(|_| "Cannot inspect current library columns.")?;
    if !exists { return Err("An older saved-data schema needs separate review before handover.".into()); }
    Ok(())
}
fn columns(db: &Connection, table_name: &str, names: &[&str]) -> Result<()> {
    for name in names { column(db, table_name, name)?; }
    Ok(())
}
fn optional_store(data: &Path, name: &str, version: i64, tables: &[&str], cold: bool, cancelled: Option<&AtomicBool>) -> Result<()> {
    if let Some(db) = database_with_cancel(data, name, version, false, cold, cancelled)? {
        for table_name in tables { table(&db, table_name)?; }
    }
    Ok(())
}
fn checkpoint(data: &Path, cold: bool, cancelled: Option<&AtomicBool>) -> Result<()> {
    let Some(db) = database_with_cancel(data, "ai-search-progress.sqlite", 0, false, cold, cancelled)? else { return Ok(()); };
    // Generative checkpoints and an unused session predate ranked search. The
    // native worker still reads them; admission must not invent a schema upgrade.
    table(&db, "session")?; columns(&db, "session", &["id", "body"])?;
    table(&db, "batches")?; columns(&db, "batches", &["position", "cards", "matches"])?;
    let ranked: Option<String> = db.query_row("SELECT type FROM sqlite_master WHERE name='ranked_candidates'", [], |row| row.get(0))
        .optional().map_err(|_| "Cannot inspect saved AI search progress.")?;
    if ranked.is_some() {
        table(&db, "ranked_candidates")?;
        columns(&db, "ranked_candidates", &["id", "ordinal", "card", "match", "preview_done", "preview_score", "refine_done", "refine_score"])?;
    }
    let count: i64 = db.query_row("SELECT count(*) FROM session", [], |row| row.get(0))
        .map_err(|_| "Cannot inspect the saved AI search session.")?;
    if count > 1 { return Err("Saved AI search progress needs recovery in the current beta before switching.".into()); }
    if count == 1 {
        let body: String = db.query_row("SELECT body FROM session WHERE id=1 AND length(CAST(body AS BLOB))<=?", [MAX_HEADER as i64], |row| row.get(0))
            .map_err(|_| "The saved AI search header is invalid or exceeds its supported size. Keep it for recovery in the current beta.")?;
        let header: Value = serde_json::from_str(&body).map_err(|_| "The saved AI search header needs recovery in the current beta.")?;
        if header["version"] != 1 || !header.is_object() {
            return Err("This saved AI search format needs compatibility review. Continue using the current beta.".into());
        }
        if header.get("ranking").is_some_and(|value| !value.is_boolean()) {
            return Err("The saved AI search type needs recovery in the current beta before switching.".into());
        }
        if header["ranking"] == true && ranked.as_deref() != Some("table") {
            return Err("Saved ranked AI search results are incomplete. Continue using the current beta to review them before switching.".into());
        }
    }
    Ok(())
}
fn saved_checkpoints(data: &Path, cold: bool, cancelled: Option<&AtomicBool>) -> Result<()> {
    checkpoint(data, cold, cancelled)?;
    let folder = data.join("jobs");
    let metadata = match fs::symlink_metadata(&folder) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect saved AI job checkpoints.".into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Saved AI job checkpoint folder needs separate review.".into());
    }
    for item in fs::read_dir(&folder).map_err(|_| "Cannot list saved AI job checkpoints.")? {
        let item = item.map_err(|_| "Cannot list a saved AI job checkpoint.")?;
        let entry = item.path();
        let metadata = fs::symlink_metadata(&entry)
            .map_err(|_| "Cannot inspect a saved AI job checkpoint.")?;
        if metadata.file_type().is_symlink() {
            return Err("A saved AI job checkpoint path needs separate review.".into());
        }
        if metadata.is_dir() { checkpoint(&entry, cold, cancelled)?; }
    }
    Ok(())
}
fn current_format(data: &Path, cold: bool) -> Result<String> {
    current_format_with_cancel(data, cold, None)
}
fn current_format_with_cancel(data: &Path, cold: bool, cancelled: Option<&AtomicBool>) -> Result<String> {
    let database = |data: &Path, name: &str, version, required, cold| database_with_cancel(data, name, version, required, cold, cancelled);
    for marker in ["restore-transaction.json", "source-capsule-transaction.json", "catalog-journal"] {
        ensure_absent(&data.join(marker),
            "The old library has unfinished recovery work. Resolve it before handover.")?;
    }
    current_marker(data, "catalog.json", "catalog.sqlite")?;
    current_marker(data, "people.json", "people.sqlite")?;
    let catalog = database(data, "catalog.sqlite", 1, true, cold)?.unwrap();
    for name in ["library_meta", "photos", "filename_lookup", "search_generation",
        "source_archives", "source_roots", "source_locations", "source_moves",
        "source_archived_photos", "source_discovery_requests"] { table(&catalog, name)?; }
    columns(&catalog, "photos", &["id", "path", "folder", "status", "signature", "body"])?;
    columns(&catalog, "source_locations", &["photo_id", "root_id", "relative_path", "status"])?;
    let header: String = catalog.query_row("SELECT body FROM library_meta WHERE key='header' AND length(CAST(body AS BLOB))<=?", [MAX_HEADER as i64], |row| row.get(0))
        .map_err(|_| "The current library header is unavailable.")?;
    if header.len() as u64 > MAX_HEADER { return Err("The current library header exceeds its size limit.".into()); }
    let header: Value = serde_json::from_str(&header).map_err(|_| "The current library header is invalid.")?;
    let id = header["libraryId"].as_str().filter(|id| !id.is_empty() && id.len() <= 128
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
        .ok_or("The current library identity is invalid.")?;
    if header["version"] != 1 || !header["settings"].is_object() || header.get("photos").is_some() {
        return Err("The library header needs separate compatibility review before handover.".into());
    }
    let people = database(data, "people.sqlite", 1, true, cold)?.unwrap();
    for name in ["metadata", "faces", "scans", "purged_photos", "purged_faces"] { table(&people, name)?; }
    columns(&people, "faces", &["id", "photoId", "body", "embedding", "signature"])?;
    let jobs = database(data, "jobs.sqlite", 1, true, cold)?.unwrap();
    for name in ["jobs", "queue_meta", "targets"] { table(&jobs, name)?; }
    columns(&jobs, "jobs", &["id", "input", "state", "checkpoint", "continuation"])?;
    columns(&jobs, "targets", &["job_id", "id", "fingerprint", "state", "result"])?;
    let host = database(data, "source-host.sqlite", 0, true, cold)?.unwrap();
    for name in ["bindings", "root_alias_bindings", "scans", "scan_dirs", "inventory", "candidates"] {
        table(&host, name)?;
    }
    columns(&host, "bindings", &["root_id", "body"])?;
    if let Some(memory) = database(data, "evidence-memory.sqlite", 1, false, cold)? {
        for name in ["photos", "sources", "reviews"] { table(&memory, name)?; }
    }
    optional_store(data, "review-priority.sqlite", 1, &["entries", "purged", "metadata"], cold, cancelled)?;
    optional_store(data, "cleanup-host.sqlite", 0,
        &["native_cleanup_plans", "native_cleanup_items", "native_cleanup_batches"], cold, cancelled)?;
    optional_store(data, "purge-host.sqlite", 0,
        &["native_purge_plans", "native_purge_items", "native_purge_assets"], cold, cancelled)?;
    if let Some(index) = database(data, "search-index.sqlite", 0, false, cold)? {
        for name in ["metadata", "assets", "evidence", "semantic_meta"] { table(&index, name)?; }
        let version: i64 = index.query_row("SELECT version FROM metadata", [], |row| row.get(0))
            .map_err(|_| "The saved search index needs an explicit rebuild before handover.")?;
        if version != 7 { return Err("The saved search index needs an explicit rebuild before handover.".into()); }
    }
    saved_checkpoints(data, cold, cancelled)?;
    Ok(id.into())
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
fn valid_digest(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn optional_json(path: &Path, limit: u64) -> Result<Option<Value>> {
    match fs::symlink_metadata(path) {
        Ok(_) => small_json(path, limit).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot inspect a companion recovery marker.".into()),
    }
}
fn read_admission(data: &Path) -> Result<Admission> {
    let record: Admission = serde_json::from_value(small_json(&data.join(ADMISSION), MAX_MARKER)?)
        .map_err(|_| "Invalid companion library admission marker.")?;
    if !admission_valid(&record) {
        return Err("Unsupported companion library admission marker.".into());
    }
    Ok(record)
}
fn read_restore_intent(data: &Path) -> Result<Option<RestoreIntent>> {
    let Some(raw) = optional_json(&data.join(RESTORE_INTENT), MAX_MARKER)? else { return Ok(None); };
    let intent: RestoreIntent = serde_json::from_value(raw)
        .map_err(|_| "Invalid companion restore intent; keep the library for recovery.")?;
    if intent.format != "photofindry-companion-restore" || intent.version != 1
        || !valid_id(&intent.old_library_id) || !valid_id(&intent.target_library_id)
        || !valid_digest(&intent.backup_catalog_sha256) {
        return Err("Unsupported companion restore intent; keep the library for recovery.".into());
    }
    Ok(Some(intent))
}
fn catalog_identity(directory: &Path, cold: bool) -> Result<String> {
    let db = database(directory, "catalog.sqlite", 1, true, cold)?.unwrap();
    table(&db, "library_meta")?;
    let raw: String = db.query_row("SELECT body FROM library_meta WHERE key='header' AND length(CAST(body AS BLOB))<=?", [MAX_HEADER as i64], |row| row.get(0))
        .map_err(|_| "Cannot read the saved library identity.")?;
    if raw.len() as u64 > MAX_HEADER { return Err("The saved library identity exceeds its bound.".into()); }
    let header: Value = serde_json::from_str(&raw)
        .map_err(|_| "Invalid saved library identity.")?;
    let id = header["libraryId"].as_str().filter(|id| valid_id(id))
        .ok_or("Invalid saved library identity.")?;
    if header["version"] != 1 { return Err("Unsupported saved library identity format.".into()); }
    Ok(id.into())
}
fn immutable_backup_identity(backup: &Path) -> Result<String> {
    let path = backup.join("catalog.sqlite");
    let db = Connection::open_with_flags(Path::new(&database_uri(&path, true)),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| "Cannot inspect selected backup catalogue.")?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA query_only=ON;PRAGMA temp_store=MEMORY;")
        .map_err(|_| "Cannot safely inspect selected backup catalogue.")?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| "Cannot read selected backup catalogue format.")?;
    if version != 1 { return Err("Unsupported selected backup catalogue format.".into()); }
    table(&db, "library_meta")?;
    let raw: String = db.query_row("SELECT body FROM library_meta WHERE key='header' AND length(CAST(body AS BLOB))<=?", [MAX_HEADER as i64], |row| row.get(0))
        .map_err(|_| "Cannot read selected backup identity.")?;
    if raw.len() as u64 > MAX_HEADER { return Err("Selected backup identity exceeds its bound.".into()); }
    let header: Value = serde_json::from_str(&raw).map_err(|_| "Invalid selected backup identity.")?;
    let id = header["libraryId"].as_str().filter(|id| valid_id(id))
        .ok_or("Invalid selected backup identity.")?;
    if header["version"] != 1 { return Err("Unsupported selected backup identity format.".into()); }
    Ok(id.into())
}
fn live_catalog_identity(directory: &Path) -> Result<String> {
    let path = directory.join("catalog.sqlite");
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Cannot inspect current library identity.")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
        return Err("Unsafe current library catalogue.".into());
    }
    let db = Connection::open_with_flags(Path::new(&database_uri(&path, false)),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| "Cannot read current library identity.")?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA query_only=ON;")
        .map_err(|_| "Cannot safely read current library identity.")?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| "Cannot read current library format.")?;
    if version != 1 { return Err("Unsupported current library format.".into()); }
    let raw: String = db.query_row("SELECT body FROM library_meta WHERE key='header' AND length(CAST(body AS BLOB))<=?", [MAX_HEADER as i64], |row| row.get(0))
        .map_err(|_| "Cannot read current library header.")?;
    if raw.len() as u64 > MAX_HEADER { return Err("Current library header exceeds its bound.".into()); }
    let header: Value = serde_json::from_str(&raw).map_err(|_| "Invalid current library header.")?;
    let id = header["libraryId"].as_str().filter(|id| valid_id(id))
        .ok_or("Invalid current library identity.")?;
    if header["version"] != 1 { return Err("Unsupported current library header.".into()); }
    Ok(id.into())
}
fn file_sha256(path: &Path) -> Result<String> {
    let before = fs::symlink_metadata(path).map_err(|_| "Cannot inspect restore catalogue.")?;
    if !before.is_file() || before.file_type().is_symlink() || before.nlink() != 1 {
        return Err("Unsafe restore catalogue file.".into());
    }
    let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path).map_err(|_| "Cannot read restore catalogue.")?;
    let opened = file.metadata().map_err(|_| "Cannot inspect restore catalogue.")?;
    if !metadata_same(&before, &opened) { return Err("Restore catalogue changed during inspection.".into()); }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut read = 0u64;
    loop {
        let count = file.read(&mut buffer).map_err(|_| "Cannot read restore catalogue.")?;
        if count == 0 { break; }
        read = read.checked_add(count as u64).ok_or("Restore catalogue exceeds its bound.")?;
        if read > before.len() { return Err("Restore catalogue grew during inspection.".into()); }
        hash.update(&buffer[..count]);
    }
    let after = file.metadata().map_err(|_| "Cannot recheck restore catalogue.")?;
    if read != before.len() || !metadata_same(&before, &after) {
        return Err("Restore catalogue changed during inspection.".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn backup_target(backup: &Path) -> Result<(String, String)> {
    if !backup.is_absolute() { return Err("Choose an absolute PhotoFindry backup path.".into()); }
    let supplied = fs::symlink_metadata(backup).map_err(|_| "Choose an existing PhotoFindry backup.")?;
    if !supplied.is_dir() || supplied.file_type().is_symlink() {
        return Err("Choose an ordinary PhotoFindry backup directory.".into());
    }
    let backup = fs::canonicalize(backup).map_err(|_| "Cannot inspect selected backup.")?;
    for suffix in ["-wal", "-shm", "-journal"] {
        ensure_absent(&backup.join(format!("catalog.sqlite{suffix}")),
            "The selected backup has an unverified SQLite sidecar.")?;
    }
    let manifest = small_json(&backup.join("manifest.json"), 32 * 1024 * 1024)?;
    if manifest["format"] != "photorecall-backup" || manifest["version"] != 1 {
        return Err("Unsupported selected PhotoFindry backup.".into());
    }
    let entries = manifest["files"].as_array().filter(|rows| rows.len() <= 250_000)
        .ok_or("Invalid selected backup manifest.")?;
    let mut expected = None;
    for row in entries {
        if row["path"] == "catalog.sqlite" {
            if expected.is_some() { return Err("Duplicate backup catalogue entry.".into()); }
            expected = row["sha256"].as_str().filter(|sha| valid_digest(sha)).map(str::to_owned);
        }
    }
    let expected = expected.ok_or("The selected backup has no catalogue checksum.")?;
    let actual = file_sha256(&backup.join("catalog.sqlite"))?;
    if actual != expected { return Err("Selected backup catalogue checksum changed.".into()); }
    let target = immutable_backup_identity(&backup)?;
    Ok((target, actual))
}
fn admission_valid(record: &Admission) -> bool {
    record.format == "photofindry-companion-library" && record.version == 1
        && valid_id(&record.library_id)
        && match record.origin.as_str() {
            "handover" => record.rollback_sha256.as_deref().is_some_and(valid_digest),
            "new" => record.rollback_sha256.is_none(),
            _ => false,
        }
}
fn private_json(data: &Path, name: &str, value: &impl Serialize, replace: bool) -> Result<()> {
    safe_ancestors(data, false)?;
    let directory = open_directory(data)?;
    let marker = data.join(name);
    if !replace {
        ensure_absent(&marker, "A private companion marker already exists; review it before replacement.")?;
    }
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|_| "Cannot allocate admission publication.")?;
    let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = data.join(format!(".{name}-{suffix}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary).map_err(|_| "Cannot stage companion library admission.")?;
        serde_json::to_writer(&mut file, value)
            .map_err(|_| "Cannot encode companion library admission.")?;
        file.write_all(b"\n").and_then(|_| file.sync_all())
            .map_err(|_| "Cannot flush companion library admission.")?;
        same_directory(data, &directory)?;
        if replace {
            fs::rename(&temporary, &marker)
                .map_err(|_| "Cannot replace companion library admission.")?;
        } else {
            ensure_absent(&marker, "A companion marker appeared during publication.")?;
            fs::hard_link(&temporary, &marker)
                .map_err(|_| "Cannot publish companion marker without replacing an existing marker.")?;
            fs::remove_file(&temporary)
                .map_err(|_| "Cannot finalize companion library admission.")?;
        }
        directory.sync_all().map_err(|_| "Cannot flush companion library admission directory.")?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}
fn publish_admission(data: &Path, record: Admission) -> Result<()> {
    if !admission_valid(&record) { return Err("Invalid companion library admission.".into()); }
    private_json(data, ADMISSION, &record, false)
}
/// The trusted launcher calls this only after the native worker responds with
/// the same library ID. A website request is never allowed to call it.
pub fn publish_handover_admission(data: &Path, receipt: &Receipt) -> Result<()> {
    if !valid_id(&receipt.library_id) || !valid_digest(&receipt.sha256) {
        return Err("Invalid verified handover receipt.".into());
    }
    publish_admission(data, Admission { format: "photofindry-companion-library".into(),
        version: 1, library_id: receipt.library_id.clone(), origin: "handover".into(),
        rollback_sha256: Some(receipt.sha256.clone()), recovery: None })
}
/// Fresh-library admission follows a separately explicit empty-directory
/// launcher choice and a successful first native worker state response.
pub fn publish_new_admission(data: &Path, library_id: &str) -> Result<()> {
    publish_admission(data, Admission { format: "photofindry-companion-library".into(),
        version: 1, library_id: library_id.into(), origin: "new".into(),
        rollback_sha256: None, recovery: None })
}
fn remove_restore_intent(data: &Path) -> Result<()> {
    let directory = open_directory(data)?;
    read_restore_intent(data)?.ok_or("The companion restore intent is unavailable.")?;
    same_directory(data, &directory)?;
    fs::remove_file(data.join(RESTORE_INTENT))
        .map_err(|_| "Cannot clear completed companion restore intent.")?;
    directory.sync_all().map_err(|_| "Cannot flush companion restore completion.".to_owned())
}
/// Called only for the paired, explicitly confirmed native restore action,
/// before handing it to PhotoWorker. The native verifier remains authoritative.
pub fn begin_restore_intent(data: &Path, backup: &Path) -> Result<String> {
    safe_ancestors(data, false)?;
    let admission = read_admission(data)?;
    if read_restore_intent(data)?.is_some() {
        return Err("A previous companion restore needs recovery before another restore.".into());
    }
    for marker in ["restore-transaction.json", "source-capsule-transaction.json"] {
        ensure_absent(&data.join(marker), "Finish the current native publication before restoring another library.")?;
    }
    if live_catalog_identity(data)? != admission.library_id {
        return Err("The admitted library identity changed before restore.".into());
    }
    let (target, catalog_sha256) = backup_target(backup)?;
    let intent = RestoreIntent { format: "photofindry-companion-restore".into(), version: 1,
        old_library_id: admission.library_id, target_library_id: target.clone(),
        backup_catalog_sha256: catalog_sha256, stage_confirmed: false };
    private_json(data, RESTORE_INTENT, &intent, false)?;
    Ok(target)
}
/// After PhotoWorker reports restartRequired, bind the durable companion intent
/// to its validated v2 staged catalogue before stopping that worker. Startup
/// performs this step itself if a crash happened just before this call.
pub fn confirm_staged_restore(data: &Path) -> Result<()> {
    let mut intent = read_restore_intent(data)?.ok_or("No authorized companion restore intent exists.")?;
    let admission = read_admission(data)?;
    if admission.library_id != intent.old_library_id {
        return Err("The admitted library changed before restore staging was confirmed.".into());
    }
    ensure_absent(&data.join("source-capsule-transaction.json"),
        "A source publication conflicts with this restore.")?;
    let record = optional_json(&data.join("restore-transaction.json"), MAX_MARKER)?
        .ok_or("The native restore has no durable publication record.")?;
    if native_restore_target(data, &record)? != intent.target_library_id {
        return Err("Native restore staging differs from the authorized backup.".into());
    }
    if !intent.stage_confirmed {
        intent.stage_confirmed = true;
        private_json(data, RESTORE_INTENT, &intent, true)?;
    }
    Ok(())
}
/// A failed native restore may clear its intent only while the old catalog is
/// still authoritative and no native publication marker exists.
pub fn abort_restore_intent(data: &Path) -> Result<()> {
    let intent = read_restore_intent(data)?.ok_or("No companion restore intent exists.")?;
    if intent.stage_confirmed {
        return Err("Confirmed native restore staging must remain available for recovery.".into());
    }
    let admission = read_admission(data)?;
    if admission.library_id != intent.old_library_id
        || live_catalog_identity(data)? != intent.old_library_id {
        return Err("The library changed during restore; preserve the intent for recovery.".into());
    }
    ensure_absent(&data.join("restore-transaction.json"),
        "A native restore publication still needs recovery.")?;
    ensure_absent(&data.join("source-capsule-transaction.json"),
        "A native source publication still needs recovery.")?;
    remove_restore_intent(data)
}
/// Called after the owned worker restart and a successful state response. A
/// target ID alone never authorizes rebind without the prior durable intent.
pub fn finish_restore_admission(data: &Path, observed_id: &str) -> Result<()> {
    let intent = read_restore_intent(data)?.ok_or("No authorized companion restore intent exists.")?;
    if !intent.stage_confirmed {
        return Err("A native restore was not confirmed before admission rebind.".into());
    }
    if observed_id != intent.target_library_id || live_catalog_identity(data)? != observed_id {
        return Err("The restored library identity does not match the authorized backup.".into());
    }
    ensure_absent(&data.join("restore-transaction.json"),
        "The native restore publication has not completed.")?;
    ensure_absent(&data.join("source-capsule-transaction.json"),
        "A native source publication is still pending.")?;
    let mut admission = read_admission(data)?;
    if admission.library_id != intent.old_library_id && admission.library_id != intent.target_library_id {
        return Err("Companion admission changed during restore.".into());
    }
    if admission.library_id == intent.old_library_id {
        admission.library_id = intent.target_library_id;
        admission.recovery = None;
        private_json(data, ADMISSION, &admission, true)?;
    }
    remove_restore_intent(data)
}
fn staged_name(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix).is_some_and(|suffix| suffix.len() == 36
        && suffix.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-'))
}
fn staged_catalog_target(data: &Path, record: &Value, name_key: &str,
        prefix: &str) -> Result<String> {
    let name = record[name_key].as_str().filter(|name| staged_name(name, prefix))
        .ok_or("Invalid native publication staging identity.")?;
    let stage = data.join(name);
    let metadata = fs::symlink_metadata(&stage)
        .map_err(|_| "Native publication staging is missing.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Unsafe native publication staging.".into());
    }
    let expected = record["files"]["catalog.sqlite"].as_str().filter(|hash| valid_digest(hash))
        .ok_or("Missing native staged catalogue checksum.")?;
    if file_sha256(&stage.join("catalog.sqlite"))? != expected {
        return Err("Native staged catalogue changed; keep the library for recovery.".into());
    }
    catalog_identity(&stage, true)
}
fn native_restore_target(data: &Path, record: &Value) -> Result<String> {
    if record["format"] != "photofindry-rust-restore" || record["version"] != 2 {
        return Err("Unsupported native restore recovery record; use a reviewed recovery path.".into());
    }
    staged_catalog_target(data, record, "stage", ".restore-")
}
fn native_source_target(data: &Path, record: &Value) -> Result<String> {
    if record["format"] != "photofindry-rust-source-attach" || record["version"] != 2 {
        return Err("Unsupported native source recovery record; use a reviewed recovery path.".into());
    }
    staged_catalog_target(data, record, "commit", ".source-commit-")
}
fn restored_source_format(data: &Path, target_id: &str) -> Result<()> {
    current_marker(data, "catalog.json", "catalog.sqlite")?;
    current_marker(data, "people.json", "people.sqlite")?;
    if catalog_identity(data, false)? != target_id {
        return Err("The restored catalogue does not match the authorized target.".into());
    }
    let people = database(data, "people.sqlite", 1, true, false)?.unwrap();
    for name in ["metadata", "faces", "scans"] { table(&people, name)?; }
    let memory = database(data, "evidence-memory.sqlite", 1, true, false)?.unwrap();
    for name in ["photos", "sources", "reviews"] { table(&memory, name)?; }
    Ok(())
}
/// Check the response from the just-started native worker before any pairing
/// URL is advertised. It finalizes only the recovery path pre-authorized above.
pub fn complete_admitted_start(data: &Path, admission: &Admission, observed_id: &str) -> Result<()> {
    match &admission.recovery {
        None if observed_id == admission.library_id => Ok(()),
        Some(Recovery::Source) if observed_id == admission.library_id => {
            ensure_absent(&data.join("source-capsule-transaction.json"),
                "The native source publication has not completed.")
        }
        Some(Recovery::Restore { target_id }) if observed_id == target_id => {
            let intent = read_restore_intent(data)?
                .ok_or("The authorized restore intent disappeared before startup completed.")?;
            if intent.stage_confirmed { finish_restore_admission(data, observed_id) }
            else if observed_id == admission.library_id { abort_restore_intent(data) }
            else { Err("A new library appeared without confirmed native restore staging.".into()) }
        }
        Some(Recovery::Restore { .. }) if observed_id == admission.library_id => {
            abort_restore_intent(data)
        }
        _ => Err("The native library identity does not match companion admission or its authorized recovery.".into()),
    }
}
/// Subsequent launches do not make another rollback copy. This read-only
/// format check sees committed WAL state; the worker owns recovery afterwards.
pub fn validate_admission(data: &Path) -> Result<Admission> {
    safe_ancestors(data, false)?;
    let directory = open_directory(data)?;
    if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Another PhotoFindry host owns this library.".into());
    }
    let mut record = read_admission(data)?;
    ensure_absent(&data.join("catalog-journal"),
        "A legacy catalog migration needs reviewed recovery before companion startup.")?;
    let intent = read_restore_intent(data)?;
    if intent.as_ref().is_some_and(|intent| record.library_id != intent.old_library_id
        && record.library_id != intent.target_library_id) {
        return Err("Companion restore intent does not belong to this admitted library.".into());
    }
    let restore = optional_json(&data.join("restore-transaction.json"), MAX_MARKER)?;
    let source = optional_json(&data.join("source-capsule-transaction.json"), 8192)?;
    if restore.is_some() && source.is_some() {
        return Err("Conflicting native publications need reviewed recovery.".into());
    }
    if restore.is_some() {
        let intent = intent.ok_or("A native restore has no companion authorization; use reviewed recovery.")?;
        if record.library_id != intent.old_library_id {
            return Err("Native restore target differs from the authorized backup.".into());
        }
        // This is the last durable chance to prove that the explicit restore
        // reached native staging before recovery removes its transaction file.
        confirm_staged_restore(data)?;
        record.recovery = Some(Recovery::Restore { target_id: intent.target_library_id });
        return Ok(record);
    }
    if let Some(source) = source {
        if intent.is_some() || native_source_target(data, &source)? != record.library_id {
            return Err("Native source publication differs from the admitted library.".into());
        }
        record.recovery = Some(Recovery::Source);
        return Ok(record);
    }
    if let Some(intent) = intent {
        let current = catalog_identity(data, false)?;
        if !intent.stage_confirmed && record.library_id == intent.old_library_id
            && current == intent.old_library_id {
            current_format(data, false)?;
            record.recovery = Some(Recovery::Restore { target_id: intent.target_library_id });
            return Ok(record);
        }
        if intent.stage_confirmed
            && (record.library_id == intent.old_library_id || record.library_id == intent.target_library_id)
            && current == intent.target_library_id {
            restored_source_format(data, &intent.target_library_id)?;
            record.recovery = Some(Recovery::Restore { target_id: intent.target_library_id });
            return Ok(record);
        }
        return Err("The library no longer matches its authorized restore intent.".into());
    }
    let current = current_format(data, false)?;
    if current != record.library_id {
        return Err("The admitted companion library identity changed; use a reviewed recovery path.".into());
    }
    Ok(record)
}
/// Metadata-only launch assessment. It never creates a marker, opens a database,
/// or reads originals. Schema and recovery validation is repeated by migrate.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationInspection {
    pub state: &'static str,
    pub library_id: Option<String>,
    pub estimated_bytes: u64,
}
fn estimate_tree(root: &Path, depth: usize, count: &mut u64, bytes: &mut u64, cancelled:&AtomicBool) -> Result<()> {
    check_cancel(cancelled)?;
    if depth > MAX_DEPTH { return Err("The library tree is too deep. Keep the old app and contact support before switching.".into()); }
    let directory = open_directory(root)?;
    for name in sorted_names(root, &directory)? {
        check_cancel(cancelled)?;
        if skip_root(&name, depth == 0, true, false) || sqlite_shared_memory(&name) { continue; }
        *count += 1;
        if *count > MAX_ENTRIES { return Err("The library contains too many entries for this migration. Keep the old app and contact support.".into()); }
        let path = root.join(&name);
        let metadata = fs::symlink_metadata(&path).map_err(|_| "The library changed. Close the old app and retry the check.")?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && (!metadata.is_file() || metadata.nlink() != 1)) {
            return Err("The library contains a linked or special file. Keep the old app and review this library before switching.".into());
        }
        if metadata.is_dir() { estimate_tree(&path, depth + 1, count, bytes,cancelled)?; }
        else { *bytes = bytes.checked_add(metadata.len()).ok_or("The library size exceeds the migration bound.")?; }
    }
    same_directory(root, &directory)
}
fn existing_companion_busy(data: &Path) -> Result<bool> {
    let marker = data.join(".companion.lock");
    match fs::symlink_metadata(&marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Cannot inspect existing companion ownership. Close PhotoFindry and retry.".into()),
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
                return Err("The companion ownership marker needs recovery. Keep the old app and contact support.".into());
            }
            let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                .open(marker).map_err(|_| "Cannot inspect existing companion ownership.")?;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { Ok(false) }
            else if std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock { Ok(true) }
            else { Err("Cannot check companion ownership. Close PhotoFindry and retry.".into()) }
        }
    }
}
pub fn inspect_migration(data: &Path) -> Result<MigrationInspection> {
    if !data.is_absolute() || data.file_name().is_none() || data.components().any(|part| matches!(part, Component::CurDir | Component::ParentDir)) {
        return Err("Choose an absolute local library directory without links or dot components.".into());
    }
    match fs::symlink_metadata(data) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent=data.parent().ok_or("The library needs a parent directory.")?;
            let existing=parent.ancestors().find(|path|path.exists()).ok_or("The library parent is unavailable.")?;
            safe_ancestors(existing,true)?;
            return Ok(MigrationInspection {state:"new",library_id:None,estimated_bytes:0});
        }
        Err(_) => return Err("The saved library is unavailable. Reconnect its disk, then retry.".into()),
        Ok(_) => {},
    }
    safe_ancestors(data, false)?;
    let held = open_directory(data)?;
    // An admitted library may already be running. Only its bounded admission is
    // inspected; normal open still validates identity before issuing authority.
    if data.join(ADMISSION).symlink_metadata().is_ok() {
        let admitted = read_admission(data)?;
        return Ok(MigrationInspection {state:"ready",library_id:Some(admitted.library_id),estimated_bytes:0});
    }
    if unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 || existing_companion_busy(data)?
        || data.join(LOCK).symlink_metadata().is_ok() {
        return Ok(MigrationInspection {state:"busy",library_id:None,estimated_bytes:0});
    }
    let names=sorted_names(data,&held)?;
    if names.iter().all(|name| name == OsStr::new(".companion.lock")) {
        return Ok(MigrationInspection {state:"new",library_id:None,estimated_bytes:0});
    }
    // Do not label arbitrary folders as a migratable library. The full schema
    // checks and SQLite recovery checks run under the handover lease on consent.
    for name in ["catalog.sqlite","people.sqlite","jobs.sqlite","source-host.sqlite"] {
        let metadata=fs::symlink_metadata(data.join(name)).map_err(|_| format!("The saved library is missing {name}. Open the old app to recover or upgrade it, then retry."))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink()!=1 || metadata.len()==0 {
            return Err("A saved database is unsafe or empty. Keep the old app and recover the library before switching.".into());
        }
    }
    Ok(MigrationInspection {state:"needs-handover",library_id:None,estimated_bytes:0})
}
/// The complete metadata-only size walk belongs to the cancellable migration,
/// not the short first-launch status command. No image contents are opened.
pub fn migration_bytes(data:&Path,cancelled:&AtomicBool)->Result<u64>{
    safe_ancestors(data,false)?;
    let mut count=0;let mut bytes=0;
    estimate_tree(data,0,&mut count,&mut bytes,cancelled)?;Ok(bytes)
}
/// Retry may reuse a fully verified copy only while every source byte and the
/// library identity still match. Partial copies are retained for human recovery.
pub fn prepare_or_resume(data: &Path, rollback: &Path, cancelled: &AtomicBool) -> Result<Receipt> {
    if !rollback.exists() { return prepare_with_cancel(data,rollback,cancelled); }
    safe_ancestors(data,false)?;safe_ancestors(rollback,false)?;
    ensure_absent(&data.join(ADMISSION),"This library is already ready; reopen it normally.")?;
    let lease=Lease::acquire(data)?;
    let id=current_format_with_cancel(data,true,Some(cancelled))?;
    let receipt=verify_snapshot_with_cancel(rollback,cancelled).map_err(|message| if cancelled.load(Ordering::Acquire) {message} else {"The previous recovery copy is incomplete or changed. Choose a new recovery-copy folder and retry; the original library is kept.".to_owned()})?;
    check_cancel(cancelled)?;
    let mut summary=Summary::new();
    walk(data,&lease.directory,Path::new(""),None,&mut summary,0,true,false,cancelled)?;
    let actual=summary.finish();
    if id!=receipt.library_id || actual.entries!=receipt.entries || actual.bytes!=receipt.bytes || actual.sha256!=receipt.sha256 {
        return Err("The library changed since its recovery copy. Choose a new recovery-copy folder, close the old app, and retry.".into());
    }
    Ok(receipt)
}
/// Explicit migration may create its private missing parent directories. Status
/// never calls this, and existing directory permissions are never changed.
pub fn create_migration_parent(path:&Path)->Result<()> {
    if !path.is_absolute()||path.components().any(|part|matches!(part,Component::CurDir|Component::ParentDir)){return Err("Choose a normalized absolute recovery parent directory.".into());}
    let mut missing=Vec::new();
    let mut current=path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(_)=>{safe_ancestors(current,true)?;break;},
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>{missing.push(current.to_path_buf());current=current.parent().ok_or("The recovery parent is unavailable.")?;},
            Err(_)=>return Err("Cannot inspect the recovery parent. Reconnect its disk and Retry.".into()),
        }
    }
    for folder in missing.into_iter().rev(){
        safe_ancestors(folder.parent().ok_or("The recovery parent is unavailable.")?,true)?;
        fs::DirBuilder::new().mode(0o700).create(&folder).map_err(|_|"Cannot create the private recovery parent. Check permissions or choose another disk, then Retry.".to_owned())?;
        safe_ancestors(&folder,true)?;
    }
    Ok(())
}
pub fn use_empty_library(path: &Path) -> Result<PathBuf> {
    if !path.exists() { return create_new_library(path); }
    let status=inspect_migration(path)?;
    if status.state!="new" { return Err("This folder is no longer empty. Retry the library check before continuing.".into()); }
    Ok(fs::canonicalize(path).map_err(|_|"Cannot resolve the new library directory.")?)
}

/// Exclusive trusted launcher choice for a brand-new local library. The caller
/// then starts PhotoWorker and publishes admission only after its first state.
pub fn create_new_library(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || path.file_name().is_none() || path.components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir)) {
        return Err("Choose a new absolute library directory.".into());
    }
    let supplied_parent = path.parent().ok_or("Choose a new library directory with a parent.")?;
    safe_ancestors(supplied_parent, true)?;
    let parent_path = fs::canonicalize(supplied_parent)
        .map_err(|_| "The new library parent is unavailable.")?;
    safe_ancestors(&parent_path, true)?;
    let target = parent_path.join(path.file_name().unwrap());
    let parent = open_directory(&parent_path)?;
    ensure_absent(&target, "Choose a new library directory that does not already exist.")?;
    same_directory(&parent_path, &parent)?;
    fs::DirBuilder::new().mode(0o700).create(&target)
        .map_err(|_| "Cannot create the private new library directory.")?;
    let created = open_directory(&target)?;
    let metadata = created.metadata().map_err(|_| "Cannot inspect the new library directory.")?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err("The new library directory is not private; review it before use.".into());
    }
    same_directory(&parent_path, &parent)?;
    same_directory(&target, &created)?;
    created.sync_all().map_err(|_| "Cannot flush the new library directory.")?;
    parent.sync_all().map_err(|_| "Cannot flush the new library parent.")?;
    Ok(target)
}

fn prepare_destination(data: &Path, target: &Path) -> Result<PathBuf> {
    if !target.is_absolute() || target.file_name().is_none() || target.components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir)) {
        return Err("Choose a new absolute rollback directory outside the library.".into());
    }
    let parent = target.parent().ok_or("Choose a rollback directory with a parent.")?;
    safe_ancestors(parent, true)?;
    let real_parent = fs::canonicalize(parent).map_err(|_| "The rollback parent is unavailable.")?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| "Cannot inspect the rollback parent.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Choose an ordinary rollback parent directory, not a symlink.".into());
    }
    safe_ancestors(&real_parent, true)?;
    let candidate = real_parent.join(target.file_name().unwrap());
    if candidate.starts_with(data) || data.starts_with(&candidate) {
        return Err("The rollback copy must be outside the source library.".into());
    }
    ensure_absent(&candidate, "Choose a new rollback directory that does not already exist.")?;
    Ok(candidate)
}

/// Called only after an explicit trusted launch choice and before opening HTTP.
/// The same worker.lock protocol excludes the old app throughout the cold copy.
pub fn prepare_with_cancel(data: &Path, rollback: &Path, cancelled: &AtomicBool) -> Result<Receipt> {
    check_cancel(cancelled)?;
    safe_ancestors(data, false)?;
    ensure_absent(&data.join(ADMISSION),
        "This library is already admitted to the companion; use a normal launch.")?;
    let supplied = fs::symlink_metadata(data).map_err(|_| "Choose an existing PhotoFindry library.")?;
    if !supplied.is_dir() || supplied.file_type().is_symlink() {
        return Err("Choose a real PhotoFindry library directory, not a symlink.".into());
    }
    let data = fs::canonicalize(data).map_err(|_| "Cannot inspect the selected library.")?;
    safe_ancestors(&data, false)?;
    let target = prepare_destination(&data, rollback)?;
    let parent_path = target.parent().ok_or("The rollback parent is unavailable.")?;
    let parent = open_directory(parent_path)?;
    let lease = Lease::acquire(&data)?;
    let library_id = current_format_with_cancel(&data, true, Some(cancelled))?;
    check_cancel(cancelled)?;
    same_directory(parent_path, &parent)?;
    fs::DirBuilder::new().mode(0o700).create(&target)
        .map_err(|_| "Cannot create the private rollback directory.")?;
    let target_directory = open_directory(&target)?;
    let result = (|| {
        let mut copied = Summary::new();
        walk(&data, &lease.directory, Path::new(""), Some(&target), &mut copied, 0, true, false, cancelled)?;
        let copied = copied.finish();
        let mut source = Summary::new();
        walk(&data, &lease.directory, Path::new(""), None, &mut source, 0, true, false, cancelled)?;
        let source = source.finish();
        same_directory(parent_path, &parent)?;
        same_directory(&target, &target_directory)?;
        let destination = open_directory(&target)?;
        let mut verified = Summary::new();
        walk(&target, &destination, Path::new(""), None, &mut verified, 0, false, false, cancelled)?;
        if copied != source || copied != verified.finish() {
            return Err("The closed library changed or its rollback copy differs. Keep the old app and retry.".into());
        }
        check_cancel(cancelled)?;
        let receipt = Receipt { format: "photofindry-local-handover".into(), version: 1,
            library_id, entries: copied.entries, bytes: copied.bytes,
            sha256: copied.sha256, rollback: target.clone() };
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(target.join(RECEIPT)).map_err(|_| "Cannot publish the rollback receipt.")?;
        serde_json::to_writer(&mut file, &receipt).map_err(|_| "Cannot write the rollback receipt.")?;
        file.write_all(b"\n").and_then(|_| file.sync_all())
            .map_err(|_| "Cannot flush the rollback receipt.")?;
        same_directory(parent_path, &parent)?;
        same_directory(&target, &target_directory)?;
        destination.sync_all().map_err(|_| "Cannot flush the rollback directory.")?;
        parent.sync_all().map_err(|_| "Cannot flush the rollback parent.")?;
        Ok(receipt)
    })();
    // A failed copy remains private and has no valid receipt. Never recursively
    // remove a path that another process could have replaced during handover.
    drop(lease);
    result
}
#[cfg(test)]
pub fn prepare(data: &Path, rollback: &Path) -> Result<Receipt> {
    let cancelled = AtomicBool::new(false);
    prepare_with_cancel(data, rollback, &cancelled)
}

/// A later rollback must verify this copy independently; a path alone is not proof.
pub fn verify_snapshot(rollback: &Path) -> Result<Receipt> {
    verify_snapshot_with_cancel(rollback, &AtomicBool::new(false))
}
fn verify_snapshot_with_cancel(rollback: &Path, cancelled: &AtomicBool) -> Result<Receipt> {
    let supplied = fs::symlink_metadata(rollback).map_err(|_| "The rollback copy is unavailable.")?;
    if !supplied.is_dir() || supplied.file_type().is_symlink() {
        return Err("The rollback copy must be an ordinary directory.".into());
    }
    let rollback = fs::canonicalize(rollback).map_err(|_| "Cannot inspect the rollback copy.")?;
    let receipt_value = small_json(&rollback.join(RECEIPT), MAX_MARKER)?;
    let mut receipt: Receipt = serde_json::from_value(receipt_value)
        .map_err(|_| "Invalid rollback receipt.")?;
    if receipt.format != "photofindry-local-handover" || receipt.version != 1
        || receipt.library_id.is_empty() || receipt.library_id.len() > 128
        || receipt.sha256.len() != 64 || !receipt.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Unsupported rollback receipt.".into());
    }
    let directory = open_directory(&rollback)?;
    let mut summary = Summary::new();
    walk(&rollback, &directory, Path::new(""), None, &mut summary, 0, false, true, cancelled)?;
    let actual = summary.finish();
    if actual.entries != receipt.entries || actual.bytes != receipt.bytes || actual.sha256 != receipt.sha256 {
        return Err("The rollback copy no longer matches its verified receipt.".into());
    }
    receipt.rollback = rollback;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Barrier};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut bytes = [0u8; 12]; getrandom::fill(&mut bytes).unwrap();
            let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let root = std::env::temp_dir().join(format!("photofindry-handover-{name}"));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
        fn data(&self) -> PathBuf { self.0.join("PhotoRecall") }
        fn rollback(&self) -> PathBuf { self.0.join("rollback") }
        fn seed(&self) {
            let data = self.data(); fs::DirBuilder::new().mode(0o700).create(&data).unwrap();
            fs::write(data.join("catalog.json"), json!({"version":2,"storage":"sqlite","database":"catalog.sqlite"}).to_string()).unwrap();
            fs::write(data.join("people.json"), json!({"version":2,"storage":"sqlite","database":"people.sqlite"}).to_string()).unwrap();
            let db = Connection::open(data.join("catalog.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE library_meta(key TEXT PRIMARY KEY,body TEXT);CREATE TABLE photos(id TEXT,path TEXT,folder TEXT,status TEXT,signature TEXT,body TEXT);CREATE TABLE filename_lookup(id TEXT,name TEXT,folded TEXT);CREATE TABLE search_generation(id INTEGER,revision INTEGER);CREATE TABLE source_archives(id TEXT);CREATE TABLE source_roots(id TEXT);CREATE TABLE source_locations(photo_id TEXT,root_id TEXT,relative_path TEXT,status TEXT);CREATE TABLE source_moves(id TEXT);CREATE TABLE source_archived_photos(photo_id TEXT);CREATE TABLE source_discovery_requests(id TEXT);PRAGMA user_version=1;").unwrap();
            db.execute("INSERT INTO library_meta VALUES('header',?)", [json!({"version":1,"libraryId":"library-a","settings":{"modelPath":"/Models/owned.gguf"},"folders":[]}).to_string()]).unwrap();
            drop(db);
            let db = Connection::open(data.join("people.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE metadata(key TEXT,body TEXT);CREATE TABLE faces(id TEXT,photoId TEXT,body TEXT,embedding BLOB,signature TEXT);CREATE TABLE scans(id TEXT);CREATE TABLE purged_photos(photoId TEXT);CREATE TABLE purged_faces(faceId TEXT);PRAGMA user_version=1;").unwrap();drop(db);
            let db = Connection::open(data.join("jobs.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE jobs(id TEXT,input TEXT,state TEXT,checkpoint TEXT,continuation INTEGER NOT NULL DEFAULT 0);CREATE TABLE queue_meta(key TEXT);CREATE TABLE targets(job_id TEXT,id TEXT,fingerprint TEXT,state TEXT,result TEXT);PRAGMA user_version=1;").unwrap();drop(db);
            let db = Connection::open(data.join("source-host.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE bindings(root_id TEXT,body TEXT);CREATE TABLE root_alias_bindings(root_id TEXT);CREATE TABLE scans(id TEXT);CREATE TABLE scan_dirs(scan_id TEXT);CREATE TABLE inventory(scan_id TEXT);CREATE TABLE candidates(id TEXT);PRAGMA user_version=0;").unwrap();drop(db);
            let db = Connection::open(data.join("evidence-memory.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE photos(photoId TEXT);CREATE TABLE sources(photoId TEXT);CREATE TABLE reviews(id TEXT);PRAGMA user_version=1;").unwrap();drop(db);
            fs::DirBuilder::new().mode(0o700).create(data.join("previews")).unwrap();
            fs::write(data.join("previews/photo.jpg"), b"cached preview").unwrap();
            fs::write(data.join("companion-ui.json"), b"host guide").unwrap();
        }
        fn backup(&self, target_id: &str) -> PathBuf {
            let backup = self.0.join(format!("backup-{target_id}"));
            fs::DirBuilder::new().mode(0o700).create(&backup).unwrap();
            fs::copy(self.data().join("catalog.sqlite"), backup.join("catalog.sqlite")).unwrap();
            let db = Connection::open(backup.join("catalog.sqlite")).unwrap();
            db.execute("UPDATE library_meta SET body=? WHERE key='header'",
                [json!({"version":1,"libraryId":target_id,"settings":{},"folders":[]}).to_string()]).unwrap();
            drop(db);
            let sha = file_sha256(&backup.join("catalog.sqlite")).unwrap();
            fs::write(backup.join("manifest.json"), json!({"format":"photorecall-backup",
                "version":1,"files":[{"path":"catalog.sqlite","sha256":sha}]}).to_string()).unwrap();
            backup
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn cancelling_a_long_database_check_releases_its_owned_library_marker() {
        let fixture = Fixture::new(); fixture.seed();
        let cancelled = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| { std::thread::sleep(std::time::Duration::from_millis(25)); cancelled.store(true, Ordering::Release); });
            let _lease = Lease::acquire(&fixture.data()).unwrap();
            let db = Connection::open_in_memory().unwrap();
            database_query(&db, Some(&cancelled), || db.query_row(
                "WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<1000000000) SELECT sum(n) FROM numbers", [], |row| row.get::<_, i64>(0)), "Database query failed.")
        });
        assert!(result.unwrap_err().to_ascii_lowercase().contains("cancel"));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(!fixture.data().join(LOCK).exists());
        drop(Lease::acquire(&fixture.data()).unwrap());
    }
    #[test]
    fn cancelled_verification_keeps_the_completed_recovery_copy() {
        let fixture = Fixture::new(); fixture.seed();
        prepare(&fixture.data(), &fixture.rollback()).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(verify_snapshot_with_cancel(&fixture.rollback(), &cancelled).unwrap_err().to_ascii_lowercase().contains("cancel"));
        verify_snapshot(&fixture.rollback()).unwrap();
        assert!(!fixture.data().join(LOCK).exists());
    }
    #[test]
    fn empty_and_generative_checkpoints_are_admitted_without_creating_ranked_tables() {
        for populated in [false, true] {
            let fixture = Fixture::new(); fixture.seed();
            let file = fixture.data().join("ai-search-progress.sqlite");
            let db = Connection::open(&file).unwrap();
            db.execute_batch("CREATE TABLE session(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);CREATE TABLE batches(position INTEGER PRIMARY KEY,cards TEXT NOT NULL,matches TEXT);").unwrap();
            if populated {
                db.execute("INSERT INTO session VALUES(1,?)", [json!({"version":1,"id":"legacy-search","next":1,"input":{},"result":{},"status":"paused"}).to_string()]).unwrap();
                db.execute("INSERT INTO batches VALUES(0,'[]','[]')", []).unwrap();
            }
            drop(db);
            let before = fs::read(&file).unwrap();
            let receipt = prepare(&fixture.data(), &fixture.rollback()).unwrap();
            assert_eq!(receipt.library_id, "library-a");
            assert_eq!(fs::read(&file).unwrap(), before);
            assert_eq!(fs::read(fixture.rollback().join("ai-search-progress.sqlite")).unwrap(), before);
            verify_snapshot(&fixture.rollback()).unwrap();
        }
    }
    #[test]
    fn ranked_checkpoint_requires_its_candidates_and_checkpoint_columns_remain_strict() {
        let fixture = Fixture::new(); fixture.seed();
        let file = fixture.data().join("ai-search-progress.sqlite");
        let db = Connection::open(&file).unwrap();
        db.execute_batch("CREATE TABLE session(id INTEGER PRIMARY KEY,body TEXT);CREATE TABLE batches(position INTEGER PRIMARY KEY,cards TEXT,matches TEXT);").unwrap();
        db.execute("INSERT INTO session VALUES(1,?)", [json!({"version":1,"ranking":true}).to_string()]).unwrap();
        drop(db);
        assert!(prepare(&fixture.data(), &fixture.rollback()).unwrap_err().contains("ranked AI search"));
        assert!(!fixture.rollback().exists());
        let db = Connection::open(&file).unwrap();
        db.execute_batch("DELETE FROM session;DROP TABLE batches;CREATE TABLE batches(position INTEGER PRIMARY KEY,cards TEXT);").unwrap();drop(db);
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn nested_legacy_and_ranked_checkpoints_are_checked_independently() {
        let fixture = Fixture::new(); fixture.seed();
        for (name, ranked) in [("legacy", false), ("ranked", true)] {
            let directory = fixture.data().join("jobs").join(name);
            fs::create_dir_all(&directory).unwrap();
            let db = Connection::open(directory.join("ai-search-progress.sqlite")).unwrap();
            db.execute_batch("CREATE TABLE session(id INTEGER PRIMARY KEY,body TEXT);CREATE TABLE batches(position INTEGER PRIMARY KEY,cards TEXT,matches TEXT);").unwrap();
            if ranked { db.execute_batch("CREATE TABLE ranked_candidates(id TEXT PRIMARY KEY,ordinal INTEGER,card TEXT,match TEXT,preview_done INTEGER,preview_score REAL,refine_done INTEGER,refine_score REAL);").unwrap(); }
            db.execute("INSERT INTO session VALUES(1,?)", [json!({"version":1,"ranking":ranked}).to_string()]).unwrap();
        }
        prepare(&fixture.data(), &fixture.rollback()).unwrap();
        verify_snapshot(&fixture.rollback()).unwrap();
    }
    #[test]
    fn migration_status_is_metadata_only_and_resume_rechecks_the_recovery_copy() {
        let fixture=Fixture::new();
        let data=fixture.data();let parent=fixture.0.join("missing/parent");
        assert_eq!(inspect_migration(&parent.join("new")).unwrap().state,"new");
        assert!(!parent.exists(),"Read-only status never creates parents");
        fixture.seed();let before=fs::read(data.join("catalog.sqlite")).unwrap();
        assert_eq!(inspect_migration(&data).unwrap().state,"needs-handover");
        assert_eq!(fs::read(data.join("catalog.sqlite")).unwrap(),before);
        let cancelled=AtomicBool::new(false);
        let bytes=migration_bytes(&data,&cancelled).unwrap();assert!(bytes>0);
        let first=prepare_or_resume(&data,&fixture.rollback(),&cancelled).unwrap();
        let second=prepare_or_resume(&data,&fixture.rollback(),&cancelled).unwrap();assert_eq!(first.sha256,second.sha256);
        fs::write(data.join("new-fact.txt"),b"new saved fact").unwrap();
        assert!(prepare_or_resume(&data,&fixture.rollback(),&cancelled).unwrap_err().contains("changed since"));
        assert_eq!(verify_snapshot(&fixture.rollback()).unwrap().sha256,first.sha256);
    }
    #[test]
    fn closed_current_library_copies_all_host_data_without_migration() {
        let fixture = Fixture::new(); fixture.seed();
        let catalog_before = fs::read(fixture.data().join("catalog.sqlite")).unwrap();
        let receipt = prepare(&fixture.data(), &fixture.rollback()).unwrap();
        assert_eq!(receipt.library_id, "library-a");
        assert!(!fixture.data().join(LOCK).exists());
        assert_eq!(fs::read(fixture.data().join("catalog.sqlite")).unwrap(), catalog_before);
        assert_eq!(fs::read(fixture.rollback().join("catalog.sqlite")).unwrap(), catalog_before);
        assert_eq!(fs::read(fixture.rollback().join("previews/photo.jpg")).unwrap(), b"cached preview");
        assert_eq!(fs::read(fixture.rollback().join("companion-ui.json")).unwrap(), b"host guide");
        assert!(!fixture.rollback().join(LOCK).exists());
        assert_eq!(verify_snapshot(&fixture.rollback()).unwrap().sha256, receipt.sha256);
        fs::write(fixture.rollback().join("previews/photo.jpg"), b"changed").unwrap();
        assert!(verify_snapshot(&fixture.rollback()).is_err());
    }
    #[test]
    fn existing_marker_or_directory_lease_refuses_without_stealing() {
        let fixture = Fixture::new(); fixture.seed();
        let barrier = Arc::new(Barrier::new(2));
        let wait = barrier.clone(); let data = fixture.data();
        let owner = std::thread::spawn(move || {
            let lease = Lease::acquire(&data).unwrap(); wait.wait(); wait.wait(); drop(lease);
        });
        barrier.wait();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(fixture.data().join(LOCK).exists());
        barrier.wait(); owner.join().unwrap();
        fs::write(fixture.data().join(LOCK), b"stale marker").unwrap();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert_eq!(fs::read(fixture.data().join(LOCK)).unwrap(), b"stale marker");
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn legacy_or_uncertain_schema_refuses_before_copy() {
        let fixture = Fixture::new(); fixture.seed();
        fs::remove_file(fixture.data().join("catalog.sqlite")).unwrap();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().exists());
        assert!(!fixture.data().join(LOCK).exists());
        let db = Connection::open(fixture.data().join("catalog.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE library_meta(key TEXT,body TEXT);CREATE TABLE photos(id TEXT);PRAGMA user_version=2;").unwrap();drop(db);
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn symlink_hardlink_or_nested_target_refuses_without_copying_outside() {
        let fixture = Fixture::new(); fixture.seed();
        let outside = fixture.0.join("outside");fs::write(&outside, b"untouched").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.data().join("previews/link")).unwrap();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"untouched");
        fs::remove_file(fixture.data().join("previews/link")).unwrap();
        fs::hard_link(&outside, fixture.data().join("previews/hardlink")).unwrap();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        fs::remove_file(fixture.data().join("previews/hardlink")).unwrap();
        assert!(prepare(&fixture.data(), &fixture.data().join("inside")).is_err());
        assert!(!fixture.data().join("inside").exists());
    }
    #[test]
    fn missing_cache_reference_is_not_hydrated_or_deleted() {
        let fixture = Fixture::new(); fixture.seed();
        fs::write(fixture.data().join("preview-reference.txt"), b"previews/missing.jpg").unwrap();
        let receipt = prepare(&fixture.data(), &fixture.rollback()).unwrap();
        assert!(receipt.entries > 5);
        assert!(!fixture.data().join("previews/missing.jpg").exists());
        assert!(!fixture.rollback().join("previews/missing.jpg").exists());
    }
    #[test]
    fn older_jobs_schema_or_search_index_refuses_without_migration() {
        let fixture = Fixture::new(); fixture.seed();
        let jobs = Connection::open(fixture.data().join("jobs.sqlite")).unwrap();
        jobs.execute_batch("ALTER TABLE jobs RENAME TO old_jobs;CREATE TABLE jobs(id TEXT,input TEXT,state TEXT,checkpoint TEXT);DROP TABLE old_jobs;").unwrap();
        drop(jobs);
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().exists());
        let jobs = Connection::open(fixture.data().join("jobs.sqlite")).unwrap();
        jobs.execute_batch("ALTER TABLE jobs ADD COLUMN continuation INTEGER NOT NULL DEFAULT 0;").unwrap();
        drop(jobs);
        let index = Connection::open(fixture.data().join("search-index.sqlite")).unwrap();
        index.execute_batch("CREATE TABLE metadata(version INTEGER);INSERT INTO metadata VALUES(6);CREATE TABLE assets(id TEXT);CREATE TABLE evidence(text TEXT);CREATE TABLE semantic_meta(key TEXT);PRAGMA user_version=0;").unwrap();
        drop(index);
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn nested_sqlite_rollback_journal_refuses_cold_copy() {
        let fixture = Fixture::new(); fixture.seed();
        fs::DirBuilder::new().mode(0o700).create(fixture.data().join("jobs")).unwrap();
        fs::DirBuilder::new().mode(0o700).create(fixture.data().join("jobs/job-a")).unwrap();
        fs::write(fixture.data().join("jobs/job-a/ai-search-progress.sqlite-journal"), b"hot transaction").unwrap();
        assert!(prepare(&fixture.data(), &fixture.rollback()).is_err());
        assert!(!fixture.rollback().join(RECEIPT).exists());
        assert!(verify_snapshot(&fixture.rollback()).is_err());
    }
    #[test]
    fn admitted_library_reopens_without_recopy_and_detects_identity_change() {
        let fixture = Fixture::new(); fixture.seed();
        let receipt = prepare(&fixture.data(), &fixture.rollback()).unwrap();
        publish_handover_admission(&fixture.data(), &receipt).unwrap();
        assert_eq!(validate_admission(&fixture.data()).unwrap().origin, "handover");
        assert!(publish_handover_admission(&fixture.data(), &receipt).is_err());
        assert_eq!(verify_snapshot(&fixture.rollback()).unwrap().sha256, receipt.sha256);
        let db = Connection::open(fixture.data().join("catalog.sqlite")).unwrap();
        db.execute("UPDATE library_meta SET body=? WHERE key='header'",
            [json!({"version":1,"libraryId":"library-b","settings":{},"folders":[]}).to_string()]).unwrap();
        drop(db);
        assert!(validate_admission(&fixture.data()).is_err());
    }
    #[test]
    fn fresh_admission_accepts_a_current_worker_created_library() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        assert_eq!(validate_admission(&fixture.data()).unwrap().origin, "new");
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn admitted_relaunch_reads_committed_wal_without_another_cold_copy() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let db = Connection::open(fixture.data().join("catalog.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL;INSERT INTO source_archives VALUES('saved');").unwrap();
        assert!(fixture.data().join("catalog.sqlite-wal").exists());
        assert_eq!(validate_admission(&fixture.data()).unwrap().library_id, "library-a");
        drop(db);
        assert!(!fixture.rollback().exists());
    }
    #[test]
    fn closed_committed_wal_is_included_in_private_rollback() {
        let fixture = Fixture::new(); fixture.seed();
        let db = Connection::open(fixture.data().join("source-host.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA wal_autocheckpoint=0;INSERT INTO bindings VALUES('root-one','{}');").unwrap();
        assert!(fixture.data().join("source-host.sqlite-wal").exists());
        let receipt = prepare(&fixture.data(), &fixture.rollback()).unwrap();
        assert!(receipt.entries > 5);
        assert!(fixture.rollback().join("source-host.sqlite-wal").exists());
        assert_eq!(verify_snapshot(&fixture.rollback()).unwrap().sha256, receipt.sha256);
        let copied = Connection::open_with_flags(fixture.rollback().join("source-host.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let count: i64 = copied.query_row("SELECT count(*) FROM bindings", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(copied);
        assert_eq!(verify_snapshot(&fixture.rollback()).unwrap().sha256, receipt.sha256);
        drop(db);
    }
    #[test]
    fn explicit_new_library_creation_is_private_and_exclusive() {
        let fixture = Fixture::new();
        let target = fixture.0.join("new-library");
        assert_eq!(create_new_library(&target).unwrap(), target);
        assert_eq!(fs::symlink_metadata(&target).unwrap().permissions().mode() & 0o077, 0);
        assert!(create_new_library(&target).is_err());
        assert!(!target.join(ADMISSION).exists());
        assert!(!target.join("catalog.sqlite").exists());
    }
    #[test]
    fn cancelled_handover_does_not_copy_or_leave_ownership() {
        let fixture = Fixture::new(); fixture.seed();
        let cancelled = AtomicBool::new(true);
        assert!(prepare_with_cancel(&fixture.data(), &fixture.rollback(), &cancelled).is_err());
        assert!(!fixture.rollback().exists());
        assert!(!fixture.data().join(LOCK).exists());
    }
    #[test]
    fn shared_writable_or_symlink_parent_cannot_receive_new_library() {
        let fixture = Fixture::new();
        let shared = fixture.0.join("shared");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(create_new_library(&shared.join("child")).is_err());
        let ordinary = fixture.0.join("ordinary");
        fs::create_dir(&ordinary).unwrap();
        std::os::unix::fs::symlink(&ordinary, fixture.0.join("linked")).unwrap();
        assert!(create_new_library(&fixture.0.join("linked/child")).is_err());
        assert!(!ordinary.join("child").exists());
    }
    #[test]
    fn authorized_restore_rebinds_after_staged_native_recovery() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-b");
        assert_eq!(begin_restore_intent(&fixture.data(), &backup).unwrap(), "library-b");
        let pending = validate_admission(&fixture.data()).unwrap();
        assert!(matches!(pending.recovery, Some(Recovery::Restore { .. })));
        let stage = fixture.data().join(".restore-00000000-0000-4000-8000-000000000000");
        fs::create_dir(&stage).unwrap();
        fs::copy(backup.join("catalog.sqlite"), stage.join("catalog.sqlite")).unwrap();
        let sha = file_sha256(&stage.join("catalog.sqlite")).unwrap();
        fs::write(fixture.data().join("restore-transaction.json"), json!({"format":"photofindry-rust-restore",
            "version":2,"stage":stage.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        let pending = validate_admission(&fixture.data()).unwrap();
        assert!(matches!(pending.recovery, Some(Recovery::Restore { .. })));
        fs::copy(stage.join("catalog.sqlite"), fixture.data().join("catalog.sqlite")).unwrap();
        fs::remove_file(fixture.data().join("restore-transaction.json")).unwrap();
        let pending = validate_admission(&fixture.data()).unwrap();
        assert!(matches!(pending.recovery, Some(Recovery::Restore { .. })));
        complete_admitted_start(&fixture.data(), &pending, "library-b").unwrap();
        assert!(!fixture.data().join(RESTORE_INTENT).exists());
        let admitted = validate_admission(&fixture.data()).unwrap();
        assert_eq!(admitted.library_id, "library-b");
        assert!(admitted.recovery.is_none());
    }
    #[test]
    fn failed_restore_intent_clears_only_while_old_library_remains() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-b");
        begin_restore_intent(&fixture.data(), &backup).unwrap();
        let admission = validate_admission(&fixture.data()).unwrap();
        complete_admitted_start(&fixture.data(), &admission, "library-a").unwrap();
        assert!(!fixture.data().join(RESTORE_INTENT).exists());
        assert_eq!(validate_admission(&fixture.data()).unwrap().library_id, "library-a");
    }
    #[test]
    fn unapproved_or_mismatched_restore_never_rebinds() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-b");
        let wrong = fixture.backup("library-c");
        let stage = fixture.data().join(".restore-00000000-0000-4000-8000-000000000000");
        fs::create_dir(&stage).unwrap();
        fs::copy(wrong.join("catalog.sqlite"), stage.join("catalog.sqlite")).unwrap();
        let sha = file_sha256(&stage.join("catalog.sqlite")).unwrap();
        fs::write(fixture.data().join("restore-transaction.json"), json!({"format":"photofindry-rust-restore",
            "version":2,"stage":stage.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        assert!(validate_admission(&fixture.data()).is_err());
        fs::remove_file(fixture.data().join("restore-transaction.json")).unwrap();
        begin_restore_intent(&fixture.data(), &backup).unwrap();
        fs::write(fixture.data().join("restore-transaction.json"), json!({"format":"photofindry-rust-restore",
            "version":2,"stage":stage.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        assert!(validate_admission(&fixture.data()).is_err());
        assert_eq!(read_admission(&fixture.data()).unwrap().library_id, "library-a");
    }
    #[test]
    fn matching_source_publication_reaches_native_recovery() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let commit = fixture.data().join(".source-commit-00000000-0000-4000-8000-000000000000");
        fs::create_dir(&commit).unwrap();
        fs::copy(fixture.data().join("catalog.sqlite"), commit.join("catalog.sqlite")).unwrap();
        let sha = file_sha256(&commit.join("catalog.sqlite")).unwrap();
        fs::write(fixture.data().join("source-capsule-transaction.json"), json!({
            "format":"photofindry-rust-source-attach","version":2,
            "commit":commit.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        let admitted = validate_admission(&fixture.data()).unwrap();
        assert!(matches!(admitted.recovery, Some(Recovery::Source)));
        assert!(complete_admitted_start(&fixture.data(), &admitted, "library-a").is_err());
        fs::remove_file(fixture.data().join("source-capsule-transaction.json")).unwrap();
        complete_admitted_start(&fixture.data(), &admitted, "library-a").unwrap();
    }
    #[test]
    fn restore_intent_reads_wal_header_backup_without_creating_sidecars() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-b");
        let db = Connection::open(backup.join("catalog.sqlite")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        drop(db);
        for suffix in ["-wal", "-shm"] {
            let sidecar = backup.join(format!("catalog.sqlite{suffix}"));
            if sidecar.exists() { fs::remove_file(sidecar).unwrap(); }
        }
        let sha = file_sha256(&backup.join("catalog.sqlite")).unwrap();
        fs::write(backup.join("manifest.json"), json!({"format":"photorecall-backup",
            "version":1,"files":[{"path":"catalog.sqlite","sha256":sha}]}).to_string()).unwrap();
        assert_eq!(begin_restore_intent(&fixture.data(), &backup).unwrap(), "library-b");
        assert!(!backup.join("catalog.sqlite-wal").exists());
        assert!(!backup.join("catalog.sqlite-shm").exists());
    }
    #[test]
    fn same_id_unstaged_restore_cannot_hide_missing_host_stores() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-a");
        begin_restore_intent(&fixture.data(), &backup).unwrap();
        assert!(!read_restore_intent(&fixture.data()).unwrap().unwrap().stage_confirmed);
        fs::remove_file(fixture.data().join("jobs.sqlite")).unwrap();
        fs::remove_file(fixture.data().join("source-host.sqlite")).unwrap();
        assert!(validate_admission(&fixture.data()).is_err(),
            "Before native staging, same-ID intent must not admit missing host stores");
    }
    #[test]
    fn same_id_confirmed_stage_recovers_after_auxiliary_stores_were_moved() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-a");
        begin_restore_intent(&fixture.data(), &backup).unwrap();
        let stage = fixture.data().join(".restore-00000000-0000-4000-8000-000000000000");
        fs::create_dir(&stage).unwrap();
        fs::copy(backup.join("catalog.sqlite"), stage.join("catalog.sqlite")).unwrap();
        let sha = file_sha256(&stage.join("catalog.sqlite")).unwrap();
        fs::write(fixture.data().join("restore-transaction.json"), json!({
            "format":"photofindry-rust-restore","version":2,
            "stage":stage.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        confirm_staged_restore(&fixture.data()).unwrap();
        assert!(read_restore_intent(&fixture.data()).unwrap().unwrap().stage_confirmed);
        fs::remove_file(fixture.data().join("restore-transaction.json")).unwrap();
        fs::remove_file(fixture.data().join("jobs.sqlite")).unwrap();
        fs::remove_file(fixture.data().join("source-host.sqlite")).unwrap();
        let admission = validate_admission(&fixture.data()).unwrap();
        assert!(matches!(admission.recovery, Some(Recovery::Restore { .. })));
        assert!(abort_restore_intent(&fixture.data()).is_err(),
            "Confirmed staging cannot be misclassified as a cancelled restore");
    }
    #[test]
    fn same_id_pending_native_transaction_confirms_proof_before_worker_launch() {
        let fixture = Fixture::new(); fixture.seed();
        publish_new_admission(&fixture.data(), "library-a").unwrap();
        let backup = fixture.backup("library-a");
        begin_restore_intent(&fixture.data(), &backup).unwrap();
        let stage = fixture.data().join(".restore-00000000-0000-4000-8000-000000000000");
        fs::create_dir(&stage).unwrap();
        fs::copy(backup.join("catalog.sqlite"), stage.join("catalog.sqlite")).unwrap();
        let sha = file_sha256(&stage.join("catalog.sqlite")).unwrap();
        fs::write(fixture.data().join("restore-transaction.json"), json!({
            "format":"photofindry-rust-restore","version":2,
            "stage":stage.file_name().unwrap().to_str().unwrap(),
            "files":{"catalog.sqlite":sha}}).to_string()).unwrap();
        assert!(validate_admission(&fixture.data()).is_ok());
        assert!(read_restore_intent(&fixture.data()).unwrap().unwrap().stage_confirmed);
        fs::remove_file(fixture.data().join("restore-transaction.json")).unwrap();
        fs::remove_file(fixture.data().join("jobs.sqlite")).unwrap();
        fs::remove_file(fixture.data().join("source-host.sqlite")).unwrap();
        assert!(validate_admission(&fixture.data()).is_ok(),
            "Proven native staging admits canonical stores until worker recreates auxiliaries");
    }
}
