//! When a remote ACP's delegation holds, against the provider's time.

use std::time::Duration;

/// How far a signer's clock may run ahead of its provider's without its
/// delegations being refused: every delegation is written as holding from
/// this long before it was issued.
///
/// Thirty seconds is the allowance a plain vera.rs delegation always had, and
/// the age at which a node's light client stops taking the provider's newest
/// finalized state as fresh, so a node whose provider state is fresh is never
/// further than this behind the provider's time.
pub const CLOCK_SKEW_ALLOWANCE: Duration = Duration::from_secs(30);

/// A delegation refused because it does not hold at the provider's time: it
/// starts later, or has ended. Writing it again at
/// [`provider_time`](Self::provider_time), with the signer's clock left out,
/// is the cure, so the refusal may be retried with a new delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DelegationWindow {
    /// When the signer issued it, by its own clock, in Unix seconds.
    pub issued_at: u64,
    /// The first second it holds.
    pub not_before: u64,
    /// The last second it holds.
    pub expires_at: u64,
    /// The time it was judged at: the node's clock, which its light client
    /// holds near the provider's finalized time, refusing a revision more
    /// than 15 seconds ahead of it and calling one older than
    /// [`CLOCK_SKEW_ALLOWANCE`] stale.
    pub provider_time: u64,
}

impl DelegationWindow {
    /// How many seconds the signer's clock was ahead of the provider's when
    /// it issued the delegation; negative when behind.
    pub fn clock_offset(&self) -> i64 {
        i64::try_from(i128::from(self.issued_at) - i128::from(self.provider_time)).unwrap_or(
            if self.issued_at > self.provider_time {
                i64::MAX
            } else {
                i64::MIN
            },
        )
    }

    /// Whether the signer's clock is further from the provider's than the
    /// allowance, rather than the delegation having been held too long.
    pub fn clock_is_off(&self) -> bool {
        self.clock_offset().unsigned_abs() > CLOCK_SKEW_ALLOWANCE.as_secs()
    }
}

impl std::fmt::Display for DelegationWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the delegation holds from {} to {}, and the provider's time is {}",
            self.not_before, self.expires_at, self.provider_time
        )?;
        let offset = self.clock_offset();
        if self.clock_is_off() {
            let direction = if offset > 0 { "ahead of" } else { "behind" };
            write!(
                f,
                ": the signer's clock is {} seconds {direction} the provider's; \
                 issue it again at the provider's time",
                offset.unsigned_abs()
            )
        } else {
            write!(f, ": issue a new one")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(issued_at: u64, provider_time: u64) -> DelegationWindow {
        DelegationWindow {
            issued_at,
            not_before: issued_at - 30,
            expires_at: issued_at + 120,
            provider_time,
        }
    }

    #[test]
    fn a_clock_within_the_allowance_is_not_called_off() {
        for provider_time in [970, 1_000, 1_030] {
            assert!(!window(1_000, provider_time).clock_is_off());
        }
        // Held past a short lifetime by a signer whose clock is right.
        let held = DelegationWindow {
            expires_at: 1_010,
            ..window(1_000, 1_020)
        };
        assert!(!held.clock_is_off());
        assert!(held.to_string().ends_with("issue a new one"));
    }

    #[test]
    fn a_clock_beyond_the_allowance_is_called_off_and_says_which_way() {
        let ahead = window(1_060, 1_000);
        assert_eq!(ahead.clock_offset(), 60);
        assert!(ahead.clock_is_off());
        assert!(ahead.to_string().contains("60 seconds ahead of"));
        let behind = window(1_000, 1_660);
        assert_eq!(behind.clock_offset(), -660);
        assert!(behind.to_string().contains("660 seconds behind"));
        let far = DelegationWindow {
            issued_at: u64::MAX,
            not_before: 0,
            expires_at: u64::MAX,
            provider_time: 0,
        };
        assert_eq!(far.clock_offset(), i64::MAX);
        assert!(far.clock_is_off());
    }
}
