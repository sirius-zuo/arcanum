use std::{collections::HashMap, sync::Mutex};

pub struct RateLimiter {
    max_per_window: usize,
    counts: Mutex<HashMap<String, usize>>,
}

impl RateLimiter {
    pub fn new(max_per_window: usize) -> Self {
        Self { max_per_window, counts: Mutex::new(HashMap::new()) }
    }

    pub fn check_and_record(&self, key: &str) -> bool {
        let mut map = self.counts.lock().unwrap();
        let count = map.entry(key.to_string()).or_insert(0);
        if *count >= self.max_per_window { return false; }
        *count += 1;
        true
    }

    pub fn reset(&self, key: &str) {
        self.counts.lock().unwrap().remove(key);
    }
}
