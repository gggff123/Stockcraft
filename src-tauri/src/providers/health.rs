//! Circuit breaker for provider calls.
//!
//! Transient failures (timeouts, 5xx, connection resets) trip the breaker after three consecutive
//! failures, with a cooldown that starts at 60 s and doubles up to 15 min. Rate limits trip it
//! immediately and policy refusals block the provider until the user explicitly resets it. A
//! successful call closes the breaker.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Transient,
    RateLimited,
    Policy,
}

#[derive(Debug, Clone, Default)]
pub struct HealthSnapshot {
    pub failures: u32,
    pub blocked: bool,
    pub open: bool,
    pub open_ms_remaining: u64,
    pub last_success_ms: Option<u64>,
    pub message: Option<String>,
}

impl HealthSnapshot {
    pub fn circuit_open_until_ms(&self) -> Option<u64> {
        if self.open_ms_remaining == 0 {
            return None;
        }
        Some(now_ms().saturating_add(self.open_ms_remaining))
    }

    pub fn gate_message(&self) -> Option<String> {
        if self.blocked {
            Some(
                self.message.clone().unwrap_or_else(|| {
                    "blocked by provider policy; reset manually to retry".into()
                }),
            )
        } else if self.open {
            Some(format!(
                "temporarily paused after {} failure(s); retrying in {}s",
                self.failures,
                self.open_ms_remaining.div_ceil(1000)
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    failures: u32,
    open_until: Option<Instant>,
    open_for: Duration,
    blocked: bool,
    last_success: Option<SystemTime>,
    message: Option<String>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            failures: 0,
            open_until: None,
            open_for: Duration::ZERO,
            blocked: false,
            last_success: None,
            message: None,
        }
    }
}

#[derive(Default)]
pub struct HealthRegistry {
    entries: Mutex<HashMap<String, Entry>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn snapshot(&self, provider: &str) -> HealthSnapshot {
        let mut entries = self.lock();
        let entry = entries.entry(provider.to_string()).or_default();
        let now = Instant::now();
        let open = entry.open_until.map(|until| until > now).unwrap_or(false);
        if entry.open_until.is_some() && !open {
            entry.open_until = None;
        }
        HealthSnapshot {
            failures: entry.failures,
            blocked: entry.blocked,
            open,
            open_ms_remaining: entry
                .open_until
                .map(|until| until.saturating_duration_since(now).as_millis() as u64)
                .unwrap_or(0),
            last_success_ms: entry.last_success.and_then(system_ms),
            message: entry.message.clone(),
        }
    }

    pub fn record_success(&self, provider: &str) {
        let mut entries = self.lock();
        let entry = entries.entry(provider.to_string()).or_default();
        entry.failures = 0;
        entry.open_until = None;
        entry.open_for = Duration::ZERO;
        entry.last_success = Some(SystemTime::now());
        entry.message = None;
    }

    pub fn record_failure(&self, provider: &str, kind: FailureKind, message: impl Into<String>) {
        let mut entries = self.lock();
        let entry = entries.entry(provider.to_string()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        entry.message = Some(message.into());
        match kind {
            FailureKind::Policy => {
                entry.blocked = true;
                entry.open_for = Duration::from_secs(3_600);
                entry.open_until = Some(Instant::now() + entry.open_for);
            }
            FailureKind::RateLimited => {
                entry.open_for = Duration::from_secs(60);
                entry.open_until = Some(Instant::now() + entry.open_for);
            }
            FailureKind::Transient => {
                if entry.failures >= 3 {
                    let exponent = entry.failures.saturating_sub(3).min(4);
                    let seconds = 60u64.saturating_mul(1u64 << exponent).min(900);
                    entry.open_for = Duration::from_secs(seconds);
                    entry.open_until = Some(Instant::now() + entry.open_for);
                }
            }
        }
    }

    pub fn reset(&self, provider: Option<&str>) {
        let mut entries = self.lock();
        match provider {
            Some(provider) => {
                entries.insert(provider.to_string(), Entry::default());
            }
            None => entries.clear(),
        }
    }
}

fn now_ms() -> u64 {
    system_ms(SystemTime::now()).unwrap_or(0)
}

fn system_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_transient_failures_open_the_circuit() {
        let health = HealthRegistry::new();
        assert!(!health.snapshot("x").open);
        health.record_failure("x", FailureKind::Transient, "timeout");
        health.record_failure("x", FailureKind::Transient, "timeout");
        assert!(!health.snapshot("x").open);
        health.record_failure("x", FailureKind::Transient, "timeout");
        let snapshot = health.snapshot("x");
        assert!(snapshot.open);
        assert!(snapshot.open_ms_remaining > 0);
        assert!(snapshot.circuit_open_until_ms().is_some());
        assert!(snapshot.gate_message().unwrap().contains("paused"));
        health.record_success("x");
        let snapshot = health.snapshot("x");
        assert!(!snapshot.open);
        assert_eq!(snapshot.failures, 0);
        assert!(snapshot.last_success_ms.is_some());
    }

    #[test]
    fn policy_failures_block_until_manual_reset() {
        let health = HealthRegistry::new();
        health.record_failure("p", FailureKind::Policy, "HTTP 403");
        let snapshot = health.snapshot("p");
        assert!(snapshot.blocked);
        assert!(snapshot.gate_message().unwrap().contains("403"));
        health.record_success("p");
        assert!(
            health.snapshot("p").blocked,
            "success elsewhere must not unblock policy"
        );
        health.reset(Some("p"));
        let snapshot = health.snapshot("p");
        assert!(!snapshot.blocked);
        assert_eq!(snapshot.failures, 0);
    }

    #[test]
    fn rate_limits_trip_immediately() {
        let health = HealthRegistry::new();
        health.record_failure("r", FailureKind::RateLimited, "429");
        assert!(health.snapshot("r").open);
        assert!(!health.snapshot("r").blocked);
    }
}
