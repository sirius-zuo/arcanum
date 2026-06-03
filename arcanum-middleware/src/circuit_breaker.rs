use std::{
    sync::atomic::{AtomicU32, AtomicU8, Ordering},
    sync::Arc,
    time::{Duration, Instant},
    sync::Mutex,
};
use metrics;

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState { Closed, Open, HalfOpen }

pub struct CircuitBreaker {
    name: Arc<str>,               // Arc<str> — shares allocation across clones, no per-call leak
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: AtomicU32,
    state: AtomicU8,                // 0=Closed, 1=Open, 2=HalfOpen
    opened_at: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(name: &str, failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            name: Arc::from(name),  // single heap alloc, no Box::leak
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

    fn label(&self) -> &'static str {
        // SAFETY: Arc::into_raw leaks the Arc, returning a raw pointer.
        // Casting to &'static str satisfies the metrics crate's label
        // requirement. The Arc's data lives for the remainder of the
        // process — circuit breakers are long-lived named instances.
        let ptr = Arc::into_raw(self.name.clone());
        unsafe { &*ptr }
    }

    pub fn record_failure(&self) {
        tracing::debug!(circuit_state = ?self.state(), "circuit breaker: failure recorded");
        let f = self.failures.fetch_add(1, Ordering::SeqCst) + 1;

        // Only transition to Open (and count the trip) once — when crossing the threshold
        // from Closed (state == 0). Subsequent failures while already Open must not
        // increment the trip counter again.
        let transitioned_to_open =
            f >= self.failure_threshold && self.state.load(Ordering::SeqCst) == 0;
        if transitioned_to_open {
            self.state.store(1, Ordering::SeqCst);
            *self.opened_at.lock().unwrap() = Some(Instant::now());
            metrics::counter!("arcanum_circuit_breaker_trips_total",
                "breaker" => self.label()).increment(1);
        }

        // Gauge: 0.0=Closed, 1.0=Open, 2.0=HalfOpen — three distinct values.
        let state_value = match self.state.load(Ordering::SeqCst) {
            1 => 1.0f64,
            2 => 2.0f64,
            _ => 0.0f64,
        };
        metrics::gauge!("arcanum_circuit_breaker_state",
            "breaker" => self.label()).set(state_value);
    }

    pub fn record_success(&self) {
        tracing::debug!(circuit_state = ?self.state(), "circuit breaker: success recorded");
        self.failures.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst);
        *self.opened_at.lock().unwrap() = None;
        metrics::gauge!("arcanum_circuit_breaker_state",
            "breaker" => self.label()).set(0.0);
    }
}
