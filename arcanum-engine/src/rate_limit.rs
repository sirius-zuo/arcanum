use std::{collections::HashMap, sync::Mutex, time::{Duration, Instant}};
use tracing::instrument;

struct WindowEntry {
    count: usize,
    window_start: Instant,
}

pub struct RateLimiter {
    max_per_window: usize,
    window_duration: Duration,
    entries: Mutex<HashMap<String, WindowEntry>>,
}

impl RateLimiter {
    pub fn new(max_per_window: usize) -> Self {
        Self::with_window(max_per_window, Duration::from_secs(60))
    }

    pub fn with_window(max_per_window: usize, window_duration: Duration) -> Self {
        Self { max_per_window, window_duration, entries: Mutex::new(HashMap::new()) }
    }

    #[instrument(skip(self), fields(key, allowed))]
    pub fn check_and_record(&self, key: &str) -> bool {
        let mut map = self.entries.lock().unwrap();
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert(WindowEntry {
            count: 0,
            window_start: now,
        });
        // Rotate window when the current one has expired.
        if now.duration_since(entry.window_start) >= self.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }
        let allowed = entry.count < self.max_per_window;
        if allowed { entry.count += 1; }
        tracing::debug!(key, allowed, "rate limit check");
        allowed
    }

    // Not public — clearing rate-limit state is an admin operation that must go
    // through an explicit privileged path, not an open API any caller can invoke.
    #[cfg(test)]
    pub(crate) fn reset_for_test(&self, key: &str) {
        self.entries.lock().unwrap().remove(key);
    }
}
