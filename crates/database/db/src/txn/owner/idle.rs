use std::time::Duration;

use web_time::Instant;

pub(super) fn expired(last_used: Instant, now: Instant, max_idle: Duration) -> bool {
    now.duration_since(last_used) >= max_idle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_uses_idle_age_not_creation_age() {
        let created = Instant::now();
        let last_used = created + Duration::from_millis(30);
        let max_idle = Duration::from_millis(40);
        let sweep = created + Duration::from_millis(50);

        assert!(expired(created, sweep, max_idle));
        assert!(!expired(last_used, sweep, max_idle));
        assert!(!expired(
            last_used,
            last_used + max_idle - Duration::from_nanos(1),
            max_idle
        ));
        assert!(expired(last_used, last_used + max_idle, max_idle));
        assert!(expired(
            last_used,
            last_used + max_idle + Duration::from_nanos(1),
            max_idle
        ));
    }
}
