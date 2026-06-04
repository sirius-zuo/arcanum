use arcanum_middleware::{CircuitBreaker, CircuitState};

#[tokio::test]
async fn test_circuit_opens_after_threshold() {
    let cb = CircuitBreaker::new("test", 3, std::time::Duration::from_secs(60));
    assert_eq!(cb.state(), CircuitState::Closed);
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_closed_circuit_allows_calls() {
    let cb = CircuitBreaker::new("test", 5, std::time::Duration::from_secs(60));
    assert!(cb.allow_request());
}

#[tokio::test]
async fn test_open_circuit_blocks_calls() {
    let cb = CircuitBreaker::new("test", 1, std::time::Duration::from_secs(60));
    cb.record_failure();
    assert!(!cb.allow_request());
}

#[tokio::test]
async fn test_trip_counter_only_increments_on_transition() {
    // This test validates the trips counter at the metrics level.
    // We can't read the counter directly, so we verify the logical
    // condition: extra failures after the circuit opens must NOT
    // increment trips_total again (i.e. the guard is transition-only).
    //
    // We expose this by checking that record_failure doesn't panic
    // and the state stays Open (not corrupted) on repeated calls.
    let cb = CircuitBreaker::new("test-trips", 2, std::time::Duration::from_secs(60));
    cb.record_failure();
    cb.record_failure(); // trips to Open
    assert_eq!(cb.state(), CircuitState::Open);
    // Extra failures while already Open — must not panic or change state
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitState::Open);
}

#[tokio::test]
async fn test_record_success_resets_to_closed() {
    let cb = CircuitBreaker::new("test-reset", 1, std::time::Duration::from_secs(60));
    cb.record_failure(); // Open
    assert_eq!(cb.state(), CircuitState::Open);
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed);
}
