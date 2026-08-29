//! Process-wide cancellation registry for in-flight provider job polling.
//! The cancel command signals a token; the polling loop observes it between
//! polls and aborts promptly.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

fn registry() -> &'static Mutex<HashMap<(String, String), Arc<AtomicBool>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<(String, String), Arc<AtomicBool>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers (or returns the existing) cancellation token for a job.
pub fn register(provider_id: &str, provider_job_id: &str) -> Arc<AtomicBool> {
    registry()
        .lock()
        .unwrap()
        .entry((provider_id.to_string(), provider_job_id.to_string()))
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

/// Signals cancellation for a job. Returns true when a submission was
/// actively polling this job.
pub fn signal(provider_id: &str, provider_job_id: &str) -> bool {
    let key = (provider_id.to_string(), provider_job_id.to_string());
    let mut tokens = registry().lock().unwrap();
    match tokens.get(&key) {
        Some(token) => {
            token.store(true, Ordering::SeqCst);
            true
        }
        None => false,
    }
}

/// Reads the cancellation state for a job.
pub fn is_cancelled(provider_id: &str, provider_job_id: &str) -> bool {
    registry()
        .lock()
        .unwrap()
        .get(&(provider_id.to_string(), provider_job_id.to_string()))
        .is_some_and(|token| token.load(Ordering::SeqCst))
}

/// Drops the token after the submission finishes.
pub fn unregister(provider_id: &str, provider_job_id: &str) {
    registry()
        .lock()
        .unwrap()
        .remove(&(provider_id.to_string(), provider_job_id.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_signal_and_clean_up() {
        let token = register("prov", "job-1");
        assert!(!is_cancelled("prov", "job-1"));
        assert!(signal("prov", "job-1"));
        assert!(token.load(Ordering::SeqCst));
        assert!(is_cancelled("prov", "job-1"));
        unregister("prov", "job-1");
        assert!(!is_cancelled("prov", "job-1"));
        assert!(!signal("prov", "job-unknown"));
    }
}
