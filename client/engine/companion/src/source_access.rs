//! Private pipe transport to the retained native folder-access owner.
//! The browser sees typed picker IDs only. Neither bookmark form is presentation.
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const MAX_INPUT: usize = 32 * 1024;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_BOOKMARK: usize = 128 * 1024;
const BATCH: usize = 16;
const MAX_GENERATION: u64 = 9_007_199_254_740_991;
type Result<T> = std::result::Result<T, String>;
const INVALID: &str = "The installed folder access owner returned an invalid response. Use verified companion recovery; saved work is kept.";
const UNAVAILABLE: &str = "The installed folder access owner is unavailable. Repair the companion installation before choosing this folder.";
pub fn owner_failure(error: &str) -> bool {
    error == INVALID || error == UNAVAILABLE
}

#[derive(Clone)]
pub struct AccessGrant {
    pub owner_grant_id: String,
    pub path: String,
    // Deliberately no Debug/Display or serialization implementation.
    reader_bookmark: String,
}
impl AccessGrant {
    pub fn reader_bookmark(&self) -> &str {
        &self.reader_bookmark
    }
    pub fn internal_value(&self) -> Value {
        json!({"ownerGrantId":self.owner_grant_id,"path":self.path,"readerBookmark":self.reader_bookmark})
    }
}
pub enum Selection {
    Cancelled,
    Granted(AccessGrant),
}
#[derive(Clone, Copy)]
pub enum FailureKind {
    Unavailable,
    PermissionDenied,
    InvalidGrant,
}
pub struct Failure {
    pub owner_grant_id: String,
    pub kind: FailureKind,
}
pub struct Restored {
    pub grants: Vec<AccessGrant>,
    pub unavailable: Vec<String>,
    pub failures: Vec<Failure>,
}
#[derive(Clone)]
pub struct RootRef {
    pub root_id: String,
    pub owner_grant_id: String,
}
pub struct RefPage {
    pub generation: u64,
    pub roots: Vec<RootRef>,
    pub next_cursor: Option<String>,
}
pub fn ref_page(value: &Value, library_id: &str, expected: Option<u64>) -> Result<RefPage> {
    let generation = value["generation"]
        .as_u64()
        .filter(|value| *value <= MAX_GENERATION)
        .ok_or(INVALID)?;
    if expected.is_some_and(|old| old != generation) {
        return Err("The library changed while restoring folder access.".into());
    }
    let roots = root_refs(value, library_id, generation)?;
    let more = value["hasMore"].as_bool().ok_or(INVALID)?;
    let cursor = if value["nextCursor"].is_null() {
        None
    } else {
        Some(text(value, "nextCursor", 128)?)
    };
    if more && cursor.is_none() {
        return Err(INVALID.into());
    }
    Ok(RefPage {
        generation,
        roots,
        next_cursor: if more { cursor } else { None },
    })
}
pub fn root_refs(value: &Value, library_id: &str, generation: u64) -> Result<Vec<RootRef>> {
    if value["libraryId"] != library_id || value["generation"].as_u64() != Some(generation) {
        return Err("The library changed while restoring folder access.".into());
    }
    let roots = value["roots"]
        .as_array()
        .filter(|roots| roots.len() <= 16)
        .ok_or(INVALID)?;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for root in roots {
        let root_id = text(root, "rootId", 128)?;
        let owner_grant_id = text(root, "ownerGrantId", 128)?;
        if !owner_id(&owner_grant_id)
            || !seen.insert(root_id.clone())
            || root.get("readerBookmark").is_some()
            || root.get("ownerBookmark").is_some()
        {
            return Err(INVALID.into());
        }
        let _ = folder(root, "path")?;
        result.push(RootRef {
            root_id,
            owner_grant_id,
        });
    }
    Ok(result)
}
pub fn restored_carriers(roots: &[RootRef], restored: &Restored) -> Vec<Value> {
    roots
        .iter()
        .filter_map(|root| {
            restored
                .grants
                .iter()
                .find(|grant| grant.owner_grant_id == root.owner_grant_id)
                .map(|grant| {
                    let mut value = grant.internal_value();
                    value["rootId"] = json!(root.root_id);
                    value
                })
        })
        .collect()
}
pub struct Owner {
    executable: PathBuf,
}
fn text(value: &Value, key: &str, max: usize) -> Result<String> {
    value[key]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= max && !v.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| INVALID.into())
}
pub fn owner_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn folder(value: &Value, key: &str) -> Result<String> {
    let path = text(value, key, 16384)?;
    if !Path::new(&path).is_absolute() {
        return Err(INVALID.into());
    }
    Ok(path)
}
fn grant(value: &Value) -> Result<AccessGrant> {
    let owner_grant_id = text(value, "ownerGrantId", 128)?;
    if !owner_id(&owner_grant_id) {
        return Err(INVALID.into());
    }
    let reader_bookmark = text(value, "readerBookmark", MAX_BOOKMARK)?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&reader_bookmark)
        .map_err(|_| INVALID)?;
    if bytes.is_empty() || bytes.len() > MAX_BOOKMARK {
        return Err(INVALID.into());
    }
    // Reject accidentally returning the persistent scoped-owner bookmark.
    if value.get("ownerBookmark").is_some() || value.get("bookmark").is_some() {
        return Err(INVALID.into());
    }
    let _ = folder(value, "selectedPath")?;
    Ok(AccessGrant {
        owner_grant_id,
        path: folder(value, "path")?,
        reader_bookmark,
    })
}
fn response(value: &Value, operation: &str, nonce: &str, generation: u64) -> Result<()> {
    if value["schema"].as_u64() != Some(1)
        || value["operation"] != operation
        || value["requestNonce"] != nonce
        || value["generation"].as_u64() != Some(generation)
    {
        return Err(INVALID.into());
    }
    if value.get("ownerBookmark").is_some() || value.get("bookmark").is_some() {
        return Err(INVALID.into());
    }
    Ok(())
}
fn nonce() -> Result<String> {
    let mut value = [0u8; 32];
    getrandom::fill(&mut value).map_err(|_| "The system random source is unavailable.")?;
    Ok(value.iter().map(|v| format!("{v:02x}")).collect())
}
async fn drain<R: AsyncRead + Unpin>(mut input: R, limit: usize) -> Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    let mut buffer = [0u8; 8192];
    loop {
        let n = input.read(&mut buffer).await.map_err(|_| UNAVAILABLE)?;
        if n == 0 {
            break;
        }
        let keep = n.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        oversized |= keep < n;
    }
    Ok((bytes, oversized))
}
impl Owner {
    /// The caller must obtain this path from the lifecycle publisher/inventory
    /// verifier, never a browser field or mutable path supplied in JSON.
    pub fn verified(executable: PathBuf) -> Self {
        Self { executable }
    }
    async fn call(&self, request: Value, timeout: Duration) -> Result<Value> {
        let mut bytes = serde_json::to_vec(&request).map_err(|_| INVALID)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_INPUT {
            return Err(INVALID.into());
        }
        let mut command = super::launch::desktop_command(&self.executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("DYLD_") || key == "NODE_OPTIONS" {
                command.env_remove(key);
            }
        }
        let mut child = command.spawn().map_err(|_| UNAVAILABLE)?;
        let mut input = child.stdin.take().ok_or(UNAVAILABLE)?;
        let output = child.stdout.take().ok_or(UNAVAILABLE)?;
        let errors = child.stderr.take().ok_or(UNAVAILABLE)?;
        let work = async {
            let send = async move {
                input.write_all(&bytes).await.map_err(|_| UNAVAILABLE)?;
                input.shutdown().await.map_err(|_| UNAVAILABLE)?;
                drop(input);
                Ok::<(), &str>(())
            };
            let (sent, output, errors) =
                tokio::join!(send, drain(output, MAX_OUTPUT), drain(errors, 4096));
            sent?;
            let (bytes, oversized) = output?;
            let _ = errors?;
            let status = child.wait().await.map_err(|_| UNAVAILABLE)?;
            if !status.success() {
                return Err(UNAVAILABLE.to_owned());
            }
            if oversized {
                return Err(INVALID.into());
            }
            serde_json::from_slice(&bytes).map_err(|_| INVALID.into())
        };
        match tokio::time::timeout(timeout, work).await {
            Ok(value) => value,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err("The folder access owner timed out. Saved work is kept; try the folder selection again.".into())
            }
        }
    }
    pub async fn select(&self, kind: &str, generation: u64) -> Result<Selection> {
        if !["folder", "sourceFolder"].contains(&kind) || generation > MAX_GENERATION {
            return Err(INVALID.into());
        }
        let nonce = nonce()?;
        let value=self.call(json!({"schema":1,"operation":"select","kind":kind,"requestNonce":nonce,"generation":generation}),Duration::from_secs(300)).await?;
        response(&value, "select", &nonce, generation)?;
        if value["cancelled"] == true {
            if value.get("selection").is_some() {
                return Err(INVALID.into());
            }
            return Ok(Selection::Cancelled);
        }
        if value["cancelled"] != false {
            return Err(INVALID.into());
        }
        Ok(Selection::Granted(grant(&value["selection"])?))
    }
    pub async fn restore(&self, ids: &[String], generation: u64) -> Result<Restored> {
        let unique: HashSet<_> = ids.iter().collect();
        if ids.is_empty()
            || ids.len() > BATCH
            || unique.len() != ids.len()
            || !ids.iter().all(|id| owner_id(id))
            || generation > MAX_GENERATION
        {
            return Err(INVALID.into());
        }
        let nonce = nonce()?;
        let value=self.call(json!({"schema":1,"operation":"restore","requestNonce":nonce,"generation":generation,"grants":ids.iter().map(|id|json!({"ownerGrantId":id})).collect::<Vec<_>>()}),Duration::from_secs(30)).await?;
        response(&value, "restore", &nonce, generation)?;
        let entries = value["entries"]
            .as_array()
            .filter(|entries| entries.len() == ids.len())
            .ok_or(INVALID)?;
        let mut seen = HashSet::new();
        let mut restored = Restored {
            grants: Vec::new(),
            unavailable: Vec::new(),
            failures: Vec::new(),
        };
        for entry in entries {
            let id = text(entry, "ownerGrantId", 128)?;
            if !unique.contains(&id) || !seen.insert(id.clone()) {
                return Err(INVALID.into());
            }
            match entry["status"].as_str() {
                Some("available") => restored.grants.push(grant(entry)?),
                Some(
                    status @ ("offline" | "permission" | "missing" | "invalid" | "unavailable"),
                ) => {
                    if entry.get("readerBookmark").is_some() {
                        return Err(INVALID.into());
                    }
                    let kind = match status {
                        "permission" => FailureKind::PermissionDenied,
                        "missing" | "invalid" => FailureKind::InvalidGrant,
                        _ => FailureKind::Unavailable,
                    };
                    restored.failures.push(Failure {
                        owner_grant_id: id.clone(),
                        kind,
                    });
                    restored.unavailable.push(id);
                }
                _ => return Err(INVALID.into()),
            }
        }
        Ok(restored)
    }
}
