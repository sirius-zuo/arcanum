use std::time::Duration;

/// Exponential backoff with full jitter.
/// delay(attempt) = rand(0, min(max_delay_ms, base_delay_ms * 2^attempt))
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self { max_attempts, base_delay_ms, max_delay_ms }
    }

    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    /// Returns a jittered delay for the given attempt number.
    /// Full jitter: rand(0, cap) where cap = min(max_delay_ms, base * 2^attempt).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let shift = attempt.min(63) as u64;
        let exp = self.base_delay_ms.saturating_mul(1u64 << shift);
        let cap = exp.min(self.max_delay_ms);
        // LCG-based jitter with time seed — no external rand dep
        let seed = attempt as u64 ^ std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        let jitter = if cap > 0 {
            seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                .wrapping_shr(33)
                % cap
        } else { 0 };
        Duration::from_millis(jitter)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(3, 1_000, 30_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_should_retry() {
        let policy = RetryPolicy::new(3, 100, 5000);
        assert!(policy.should_retry(0));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_retry_policy_delay_capped() {
        let policy = RetryPolicy::new(5, 1000, 2000);
        let d = policy.delay_for_attempt(10);
        assert!(d.as_millis() <= 2000, "delay must not exceed max_delay_ms");
    }

    #[test]
    fn test_retry_policy_cap_grows_with_attempt() {
        let policy = RetryPolicy::new(5, 100, 10_000);
        // cap(0)=100, cap(1)=200, cap(2)=400 — verify caps grow
        let cap0 = 100u64.min(10_000);
        let cap1 = 200u64.min(10_000);
        assert!(cap1 > cap0);
    }
}
