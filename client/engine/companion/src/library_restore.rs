//! Linux/Windows ownership of an explicitly confirmed whole-library restore.
//!
//! The worker verifies and stages the backup, snapshots the current library
//! into a rollback folder and writes a durable restore record before it replies.
//! Only the next worker start publishes that record (catalog, People and
//! evidence together, replayed after an interruption). This owner performs that
//! restart outside the HTTP request, so a closed browser cannot strand a staged
//! restore, and it revokes selections and folder scopes of the old library.
//! macOS keeps its separate admission handover in `handover.rs`.
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
pub struct Replacement {
    replacing: AtomicBool,
    generation: AtomicU64,
}
impl Replacement {
    pub fn replacing(&self) -> bool {
        self.replacing.load(Ordering::Acquire)
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    /// Requests that remain useful while the restored library opens.
    pub fn allowed_while_replacing(path: &str) -> bool {
        matches!(
            path,
            "/api/state"
                | "/api/session"
                | "/api/pair"
                | "/api/bootstrap"
                | "/api/disconnect"
                | "/api/quit"
                | "/api/capabilities"
                | "/api/library-info"
                | "/api/appearance"
                | "/api/appearance/update"
        )
    }
    pub fn opening_snapshot(&self) -> Value {
        json!({"loading":true,"libraryId":null,"libraryGeneration":self.generation(),
            "runtime":{"status":"switching","message":"Opening the restored local library."}})
    }
}

struct Opening<'a>(&'a AtomicBool);
impl Drop for Opening<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Run the staged restore and the restart that publishes it. `revoke` clears
/// grants and scopes bound to the replaced library before the new one opens.
pub async fn run<C, R>(call: C, restart: R, replacement: &Replacement, revoke: impl FnOnce()) -> Result<Value, String>
where
    C: std::future::Future<Output = Result<Value, String>>,
    R: AsyncFnOnce() -> Result<Value, String>,
{
    let mut value = call.await?;
    if value["restartRequired"] != true {
        return Err("The restore did not stage a replacement. The current library is kept.".into());
    }
    replacement.replacing.store(true, Ordering::Release);
    let opening = Opening(&replacement.replacing);
    replacement.generation.fetch_add(1, Ordering::AcqRel);
    revoke();
    let snapshot = restart().await?;
    drop(opening);
    if let Some(error) = snapshot["libraryError"].as_str() {
        return Err(format!("The restored library could not open: {error} Its staged copy and the previous library's rollback copy are kept."));
    }
    value["restarted"] = json!(true);
    value["libraryId"] = snapshot["libraryId"].clone();
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn restart_follows_only_a_staged_restore_and_revokes_old_grants() {
        let replacement = Replacement::default();
        let revoked = std::cell::Cell::new(false);
        let refused = run(async { Ok(json!({"restartRequired":false})) },
            async || -> Result<Value, String> { panic!("must not restart") }, &replacement, || revoked.set(true)).await;
        assert!(refused.is_err() && !revoked.get() && replacement.generation() == 0);
        let failed = run(async { Err::<Value, String>("checksum".into()) },
            async || -> Result<Value, String> { panic!("must not restart") }, &replacement, || revoked.set(true)).await;
        assert_eq!(failed.unwrap_err(), "checksum");
        let observed = std::cell::Cell::new(false);
        let done = run(async { Ok(json!({"restartRequired":true,"photos":3})) },
            async || { observed.set(replacement.replacing()); Ok(json!({"libraryId":"restored"})) }, &replacement, || revoked.set(true)).await.unwrap();
        assert!(observed.get(), "requests are held while the restored library opens");
        assert!(revoked.get() && !replacement.replacing() && replacement.generation() == 1);
        assert_eq!(done["restarted"], true);
        assert_eq!(done["libraryId"], "restored");
        let broken = run(async { Ok(json!({"restartRequired":true})) },
            async || Ok(json!({"libraryError":"Restore staging changed."})), &replacement, || {}).await;
        assert!(broken.unwrap_err().contains("rollback copy are kept"));
        assert!(!replacement.replacing() && replacement.generation() == 2);
        let stopped = run(async { Ok(json!({"restartRequired":true})) },
            async || Err("The local worker could not restart.".to_owned()), &replacement, || {}).await;
        assert!(stopped.is_err() && !replacement.replacing());
    }
}
