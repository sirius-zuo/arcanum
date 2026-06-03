use std::{
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
    time::{Duration, Instant},
    sync::Mutex,
};
use metrics;

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState { Closed, Open, HalfOpen }

pub struct CircuitBreaker {
    name: &'static str,
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    state: AtomicU8, // 0=Closed, 1=Open, 2=HalfOpen
    opened_at: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(name: &str, failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            name: Box::leak(name.to_owned().into_boxed_str()),
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
        let allowed = !matches!(self.state(), CircuitState::Open);
        if !allowed {
            tracing::warn!(circuit_state = ?self.state(), "circuit breaker blocking request");
        }
        allowed
    }

    pub fn record_failure(&self) {
        tracing::debug!(circuit_state = ?self.state(), "circuit breaker: failure recorded");
        let f = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        let state_value: f64 = if f >= self.failure_threshold && self.state.load(Ordering::SeqCst) == 0 {
            self.state.store(1, Ordering::SeqCst);
            *self.opened_at.lock().unwrap() = Some(Instant::now());
            1.0
        } else {
            match self.state.load(Ordering::SeqCst) {
                1 => 1.0,
                _ => 0.0,
            }
        };
        if state_value == 1.0 {
            metrics::counter!("arcanum_circuit_breaker_trips_total",
                "breaker" => self.name).increment(1);
        }
        metrics::gauge!("arcanum_circuit_breaker_state",
            "breaker" => self.name).set(state_value);
    }

    pub fn record_success(&self) {
        tracing::debug!(circuit_state = ?self.state(), "circuit breaker: success recorded");
        self.failures.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst);
        *self.opened_at.lock().unwrap() = None;
        metrics::gauge!("arcanum_circuit_breaker_state",
            "breaker" => self.name).set(0.0);
    }
}
