//! Why selective block fetches made no progress. Diagnostics only: rotation,
//! stall budget and retries never read these counts.

use crate::transport::{FetchFailureCause, PeerId};

/// How one provider window ended without a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowFailure {
    TimedOut,
    Error,
    SendFailed,
    NoBlocks,
    Unservable,
    Refused,
    Busy,
    Superseded,
}

impl WindowFailure {
    /// In the order an operator acts on them; [`FetchFailures::dominant`]
    /// breaks ties toward the earlier one.
    pub(super) const ALL: [Self; 8] = [
        Self::TimedOut,
        Self::Error,
        Self::SendFailed,
        Self::NoBlocks,
        Self::Unservable,
        Self::Refused,
        Self::Busy,
        Self::Superseded,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::TimedOut => "timed_out",
            Self::Error => "error",
            Self::SendFailed => "send_failed",
            Self::NoBlocks => "no_blocks",
            Self::Unservable => "unservable",
            Self::Refused => "refused",
            Self::Busy => "busy",
            Self::Superseded => "superseded",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::TimedOut => "providers did not answer within the response deadline",
            Self::Error => "provider requests failed with a transport error",
            Self::SendFailed => "requests could not be sent to providers",
            Self::NoBlocks => "providers answered without the requested blocks",
            Self::Unservable => "providers cannot serve it within their CAR byte limits",
            Self::Refused => "providers refused it (capacity, rate limit or access)",
            Self::Busy => "providers shed it as busy",
            Self::Superseded => "the pending root was superseded",
        }
    }
}

impl From<FetchFailureCause> for WindowFailure {
    fn from(cause: FetchFailureCause) -> Self {
        match cause {
            FetchFailureCause::Refused => Self::Refused,
            FetchFailureCause::TimedOut => Self::TimedOut,
            FetchFailureCause::NoBlocks => Self::NoBlocks,
            FetchFailureCause::Error => Self::Error,
        }
    }
}

/// Window failures of one batch or one attempt, with each provider's last one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct FetchFailures {
    counts: [usize; WindowFailure::ALL.len()],
    /// Batches failed without a query because the attempt's stall budget ran out.
    pub(super) budget_exhausted: usize,
    providers: Vec<(PeerId, WindowFailure)>,
}

impl FetchFailures {
    pub(super) fn record(&mut self, provider: &PeerId, failure: WindowFailure) {
        self.counts[failure as usize] += 1;
        self.set_last(provider, failure);
    }

    pub(super) fn merge(&mut self, other: &Self) {
        for (count, other) in self.counts.iter_mut().zip(other.counts) {
            *count += other;
        }
        self.budget_exhausted += other.budget_exhausted;
        for (provider, failure) in &other.providers {
            self.set_last(provider, *failure);
        }
    }

    pub(super) fn count(&self, failure: WindowFailure) -> usize {
        self.counts[failure as usize]
    }

    pub(super) fn dominant(&self) -> Option<WindowFailure> {
        WindowFailure::ALL
            .into_iter()
            .rev()
            .filter(|failure| self.count(*failure) > 0)
            .max_by_key(|failure| self.count(*failure))
    }

    pub(super) fn is_superseded_only(&self) -> bool {
        let superseded = self.count(WindowFailure::Superseded);
        superseded > 0
            && superseded == self.counts.iter().sum::<usize>()
            && self.budget_exhausted == 0
    }

    /// `peer:outcome` per provider, in the order they were tried.
    pub(super) fn provider_outcomes(&self) -> String {
        self.providers
            .iter()
            .map(|(provider, failure)| format!("{provider}:{}", failure.label()))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub(super) fn batch_reason(&self) -> &'static str {
        self.dominant().map_or(
            "the attempt's stall budget is exhausted",
            WindowFailure::reason,
        )
    }

    fn set_last(&mut self, provider: &PeerId, failure: WindowFailure) {
        match self.providers.iter_mut().find(|(peer, _)| peer == provider) {
            Some((_, last)) => *last = failure,
            None => self.providers.push((provider.clone(), failure)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(name: &str) -> PeerId {
        PeerId::new(name.to_string())
    }

    #[test]
    fn merged_attempts_keep_per_outcome_counts_and_each_providers_last_outcome() {
        let mut batch = FetchFailures::default();
        batch.record(&peer("hub"), WindowFailure::Refused);
        batch.record(&peer("spoke"), WindowFailure::TimedOut);
        let mut second = FetchFailures::default();
        second.record(&peer("hub"), WindowFailure::SendFailed);
        second.budget_exhausted += 1;
        let mut attempt = FetchFailures::default();
        attempt.merge(&batch);
        attempt.merge(&second);

        assert_eq!(
            [
                WindowFailure::Refused,
                WindowFailure::TimedOut,
                WindowFailure::SendFailed
            ]
            .map(|failure| attempt.count(failure)),
            [1, 1, 1]
        );
        assert_eq!(attempt.budget_exhausted, 1);
        assert_eq!(
            attempt.provider_outcomes(),
            "hub:send_failed,spoke:timed_out"
        );
        assert_eq!(
            attempt.dominant(),
            Some(WindowFailure::TimedOut),
            "a tie goes to the timeout"
        );
    }

    #[test]
    fn supersession_alone_and_an_exhausted_budget_are_their_own_outcomes() {
        let mut failures = FetchFailures::default();
        failures.record(&peer("hub"), WindowFailure::Superseded);
        assert!(failures.is_superseded_only());
        assert!(failures.batch_reason().contains("superseded"));
        failures.record(&peer("hub"), WindowFailure::Refused);
        assert!(!failures.is_superseded_only());

        let mut budget = FetchFailures::default();
        budget.budget_exhausted += 1;
        assert_eq!(budget.dominant(), None);
        assert!(!budget.is_superseded_only());
        assert!(budget.batch_reason().contains("stall budget"));
    }
}
