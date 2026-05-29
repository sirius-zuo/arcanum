use arcanum_engine::rate_limit::RateLimiter;

#[test]
fn test_rate_limiter_allows_under_limit() {
    let rl = RateLimiter::new(10);
    for _ in 0..10 { assert!(rl.check_and_record("user-1")); }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let rl = RateLimiter::new(2);
    rl.check_and_record("user-1");
    rl.check_and_record("user-1");
    assert!(!rl.check_and_record("user-1"));
}
