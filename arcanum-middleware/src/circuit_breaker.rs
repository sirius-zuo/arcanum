use std::{
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
    time::{Duration, Instant},
    sync::Mutex,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState { Closed, Open, HalfOpen }

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    state: AtomicU8, // 0=Closed, 1=Open, 2=HalfOpen
    opened_at: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold, reset_timeout,
            failures: AtomicU32::new(0),
            state: AtomicU8::new(0),
            opened_at: Mutex::new(None),
        }
    }

    pub fn state(&self) -> CircuitState {
        match self.state.load(Ordering::SeqCst) {
            0 => CircuitState::Closed,
            1 => {
                let opened = self.opened_at.lock().unwrap();
                if let Some(t) = *opened {
                    if t.elapsed() >= self.reset_timeout {
                        drop(opened);
                        self.state.store(2, Ordering::SeqCst);
                        return CircuitState::HalfOpen;
                    }
                }
                CircuitState::Open
            }
            _ => CircuitState::HalfOpen,
        }
    }

    pub fn allow_request(&self) -> bool {
        !matches!(self.state(), CircuitState::Open)
    }

    pub fn record_failure(&self) {
        let f = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if f >= self.failure_threshold && self.state.load(Ordering::SeqCst) == 0 {
            self.state.store(1, Ordering::SeqCst);
            *self.opened_at.lock().unwrap() = Some(Instant::now());
        }
    }

    pub fn record_success(&self) {
        self.failures.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst);
        *self.opened_at.lock().unwrap() = None;
    }
}
