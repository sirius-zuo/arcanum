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
