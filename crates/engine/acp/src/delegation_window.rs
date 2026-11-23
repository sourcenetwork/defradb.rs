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

/// A delegation refused because it does not hold at the provider's time.
/// Writing it again at [`provider_time`](Self::provider_time), with the
/// signer's clock left out, is the cure, so the refusal may be retried with a
/// new delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DelegationWindow {
    /// The first second it holds.
    pub not_before: u64,
    /// The last second it holds.
    pub expires_at: u64,
    /// The time it was judged at: the node's clock, which its light client
    /// holds near the provider's finalized time, refusing a revision more
    /// than 15 seconds ahead of it and calling one older than
    /// [`CLOCK_SKEW_ALLOWANCE`] stale.
    pub provider_time: u64,
    /// Why it does not hold, and what that proves about the signer's clock.
    pub refusal: WindowRefusal,
}

/// Why a delegation does not hold at the provider's time. Each kind says only
/// what the delegation's own times prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowRefusal {
    /// It starts after the provider's time. Its signer cannot have written a
    /// start later than its own clock, so that clock is ahead of the
    /// provider's by at least this many seconds; by more when the signer set
    /// its start back, as DefraDB's delegations are.
    NotYetValid { ahead_by_at_least: u64 },
    /// It ended at or before the provider's time. That proves nothing about
    /// the signer's clock: a delegation held past its lifetime by a signer
    /// whose clock is right ends the same way.
    Expired,
    /// It is bound to a request whose operation identity stays open longer
    /// past the provider's time than the provider keeps a request open.
    OutlastsRequest,
}

impl std::fmt::Display for DelegationWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the delegation holds from {} to {}, and the provider's time is {}: ",
            self.not_before, self.expires_at, self.provider_time
        )?;
        match self.refusal {
            WindowRefusal::NotYetValid { ahead_by_at_least } => write!(
                f,
                "it is not valid yet, so the signer's clock is at least {ahead_by_at_least} \
                 seconds ahead of the provider's; issue it again at the provider's time"
            ),
            WindowRefusal::Expired => {
                write!(f, "it has expired; issue a new one at the provider's time")
            }
            WindowRefusal::OutlastsRequest => write!(
                f,
                "it stays open longer than the provider keeps a request open; \
                 issue it again at the provider's time with a shorter lifetime"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(provider_time: u64, refusal: WindowRefusal) -> DelegationWindow {
        DelegationWindow {
            not_before: 1_030,
            expires_at: 1_180,
            provider_time,
            refusal,
        }
    }

    #[test]
    fn a_delegation_not_valid_yet_names_a_lower_bound_on_the_signers_lead() {
        let early = window(
            1_000,
            WindowRefusal::NotYetValid {
                ahead_by_at_least: 30,
            },
        );
        assert!(early
            .to_string()
            .contains("at least 30 seconds ahead of the provider's"));
    }

    #[test]
    fn an_expired_delegation_makes_no_claim_about_the_clock() {
        let held = window(1_200, WindowRefusal::Expired).to_string();
        assert!(held.ends_with("it has expired; issue a new one at the provider's time"));
        assert!(!held.contains("clock"), "{held}");
        assert!(window(1_100, WindowRefusal::OutlastsRequest)
            .to_string()
            .contains("shorter lifetime"));
    }

    #[test]
    fn the_refusal_keeps_its_kind_on_the_wire() {
        let early = window(
            1_000,
            WindowRefusal::NotYetValid {
                ahead_by_at_least: 30,
            },
        );
        let wire = serde_json::to_value(early).unwrap();
        assert_eq!(wire["refusal"]["not_yet_valid"]["ahead_by_at_least"], 30);
        assert_eq!(
            serde_json::from_value::<DelegationWindow>(wire).unwrap(),
            early
        );
    }
}
