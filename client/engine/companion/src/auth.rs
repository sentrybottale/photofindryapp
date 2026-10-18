//! Local terminal pairing and short-lived, native-launch bootstrap authority.
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

const PAIR_SECONDS: u64 = 300;
const MAX_ATTEMPTS: u8 = 8;
const BOOTSTRAP_SECONDS: u64 = 30;
const MAX_BOOTSTRAPS: usize = 4;

fn secret() -> Result<String, &'static str> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| "The operating system random source is unavailable.")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn equal(candidate: &str, expected: &str) -> bool {
    candidate.len() == 64
        && expected.len() == 64
        && bool::from(candidate.as_bytes().ct_eq(expected.as_bytes()))
}

pub struct Authority {
    instance_id: String,
    pairing: Option<(String, Instant)>,
    session: Option<String>,
    bootstraps: Vec<(String, Instant)>,
    attempts: u8,
}

impl Authority {
    pub fn new() -> Result<Self, &'static str> {
        Ok(Self {
            instance_id: secret()?,
            pairing: None,
            session: None,
            bootstraps: Vec::new(),
            attempts: 0,
        })
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn issue(&mut self, now: Instant) -> Result<String, &'static str> {
        let code = secret()?;
        self.pairing = Some((code.clone(), now + Duration::from_secs(PAIR_SECONDS)));
        self.session = None;
        self.bootstraps.clear();
        self.attempts = 0;
        Ok(code)
    }
    pub fn pair(&mut self, code: &str, now: Instant) -> Result<String, &'static str> {
        if self.attempts >= MAX_ATTEMPTS {
            return Err("Pairing locked. Enter pair in the companion terminal.");
        }
        self.attempts += 1;
        if !self
            .pairing
            .as_ref()
            .is_some_and(|(expected, expiry)| now < *expiry && equal(code, expected))
        {
            return Err("Invalid or expired pairing code.");
        }
        let token = secret()?;
        self.pairing = None;
        self.session = Some(token.clone());
        self.bootstraps.clear();
        Ok(token)
    }
    pub fn issue_bootstrap(&mut self, now: Instant) -> Result<String, &'static str> {
        self.bootstraps.retain(|(_, expiry)| now < *expiry);
        if self.bootstraps.len() >= MAX_BOOTSTRAPS {
            return Err("A local browser launch is already pending. Try again shortly.");
        }
        let ticket = secret()?;
        self.bootstraps.push((ticket.clone(), now + Duration::from_secs(BOOTSTRAP_SECONDS)));
        Ok(ticket)
    }
    pub fn exchange_bootstrap(&mut self, ticket: &str, now: Instant) -> Result<String, &'static str> {
        self.bootstraps.retain(|(_, expiry)| now < *expiry);
        let index = self.bootstraps.iter().position(|(expected, _)| equal(ticket, expected))
            .ok_or("Invalid or expired local launch. Open PhotoFindry again.")?;
        let token = match &self.session {
            Some(existing) => existing.clone(),
            None => secret()?,
        };
        self.bootstraps.remove(index);
        self.session = Some(token.clone());
        Ok(token)
    }
    // Time does not revoke a process session. Only explicit local revocation or
    // this Authority's destruction does; the companion does not persist its authority to disk.
    pub fn authorized(&self, token: &str, _now: Instant) -> bool {
        self.session.as_ref().is_some_and(|expected| equal(token, expected))
    }
    pub fn active(&self, _now: Instant) -> bool {
        self.session.is_some()
    }
    pub fn disconnect(&mut self) {
        self.session = None;
        self.pairing = None;
        self.bootstraps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_use_pairing_expiry_and_terminal_revocation() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let code = auth.issue(now).unwrap();
        let token = auth.pair(&code, now).unwrap();
        assert!(auth.pair(&code, now).is_err());
        assert!(auth.authorized(&token, now));
        assert!(auth.authorized(&token, now + Duration::from_secs(365 * 24 * 3600)));
        auth.issue(now).unwrap();
        assert!(!auth.authorized(&token, now));
        let code = auth.issue(now).unwrap();
        assert!(
            auth.pair(&code, now + Duration::from_secs(PAIR_SECONDS))
                .is_err()
        );
    }
    #[test]
    fn guessing_is_bounded_until_terminal_action() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let code = auth.issue(now).unwrap();
        for _ in 0..MAX_ATTEMPTS {
            assert!(auth.pair(&"0".repeat(64), now).is_err());
        }
        assert!(auth.pair(&code, now).is_err());
        let next = auth.issue(now).unwrap();
        assert!(auth.pair(&next, now).is_ok());
        auth.disconnect();
        assert!(!auth.authorized(&code, now));
    }
    #[test]
    fn process_sessions_do_not_survive_a_new_authority_or_explicit_revocation() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        assert!(!auth.active(now));
        let code = auth.issue(now).unwrap();
        let token = auth.pair(&code, now).unwrap();
        assert!(auth.active(now));
        let replacement = Authority::new().unwrap();
        assert_ne!(auth.instance_id(), replacement.instance_id());
        assert!(!replacement.authorized(&token, now));
        auth.disconnect();
        assert!(!auth.authorized(&token, now));
        assert!(!auth.active(now));
    }
    #[test]
    fn bootstrap_is_one_use_short_lived_and_cannot_revoke_existing_access() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let code = auth.issue(now).unwrap();
        let token = auth.pair(&code, now).unwrap();
        let ticket = auth.issue_bootstrap(now).unwrap();
        assert!(auth.exchange_bootstrap(&"0".repeat(64), now).is_err());
        assert!(auth.authorized(&token, now));
        assert_eq!(auth.exchange_bootstrap(&ticket, now).unwrap(), token);
        assert!(auth.exchange_bootstrap(&ticket, now).is_err());
        assert!(auth.authorized(&token, now));
        let ticket = auth.issue_bootstrap(now).unwrap();
        assert!(auth.exchange_bootstrap(&ticket, now + Duration::from_secs(BOOTSTRAP_SECONDS)).is_err());
        assert!(auth.authorized(&token, now + Duration::from_secs(BOOTSTRAP_SECONDS)));
    }
    #[test]
    fn concurrent_bootstraps_share_a_session_and_full_capacity_preserves_every_grant() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let tickets: Vec<_> = (0..MAX_BOOTSTRAPS).map(|_| auth.issue_bootstrap(now).unwrap()).collect();
        assert!(auth.issue_bootstrap(now).is_err());
        let token = auth.exchange_bootstrap(&tickets[0], now).unwrap();
        for ticket in &tickets[1..] {
            assert_eq!(auth.exchange_bootstrap(ticket, now).unwrap(), token);
        }
        assert!(auth.authorized(&token, now));
        assert!(auth.issue_bootstrap(now).is_ok());
    }
    #[test]
    fn terminal_pair_and_disconnect_clear_all_pending_bootstraps() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let stale = auth.issue_bootstrap(now).unwrap();
        let code = auth.issue(now).unwrap();
        assert!(auth.exchange_bootstrap(&stale, now).is_err());
        let after_issue = auth.issue_bootstrap(now).unwrap();
        let token = auth.pair(&code, now).unwrap();
        assert!(auth.exchange_bootstrap(&after_issue, now).is_err());
        let pending = auth.issue_bootstrap(now).unwrap();
        auth.disconnect();
        assert!(auth.exchange_bootstrap(&pending, now).is_err());
        assert!(!auth.authorized(&token, now));
    }
    #[test]
    fn expired_bootstraps_release_capacity_without_replacing_a_live_session() {
        let now = Instant::now();
        let mut auth = Authority::new().unwrap();
        let first = auth.issue_bootstrap(now).unwrap();
        let token = auth.exchange_bootstrap(&first, now).unwrap();
        for _ in 0..MAX_BOOTSTRAPS { auth.issue_bootstrap(now).unwrap(); }
        assert!(auth.issue_bootstrap(now).is_err());
        let later = now + Duration::from_secs(BOOTSTRAP_SECONDS);
        let fresh = auth.issue_bootstrap(later).unwrap();
        assert_eq!(auth.exchange_bootstrap(&fresh, later).unwrap(), token);
    }
}
