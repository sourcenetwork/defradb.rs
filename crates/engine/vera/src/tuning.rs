use std::time::Duration;

/// Tuning parameters for ACP network calls, caching, and circuit breaking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpTuning {
    pub request_timeout: Duration,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_reset_timeout: Duration,
    pub cache_ttl: Duration,
    pub receipt_timeout: Duration,
    /// Longest wait between two asks for a submission's receipt when no new
    /// finalized header arrives. A new header ends the wait sooner. Values
    /// below 5 ms are taken as 5 ms.
    pub receipt_fallback: Duration,
}

impl Default for AcpTuning {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(5),
            circuit_breaker_threshold: 3,
            circuit_breaker_reset_timeout: Duration::from_secs(30),
            cache_ttl: Duration::from_secs(300),
            receipt_timeout: Duration::from_secs(30),
            receipt_fallback: Duration::from_millis(200),
        }
    }
}
