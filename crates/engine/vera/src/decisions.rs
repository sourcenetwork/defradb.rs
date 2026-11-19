//! Remote access decisions as an [`inputs::Source`]: one call per decision,
//! keyed by the full (actor, policy, resource, object, permission) tuple, so
//! concurrent checks of one tuple share one call; a page's checks run at once
//! under the in-flight cap; every call has a deadline and a timeout is tried
//! again.
//!
//! A decision the provider read at a verified state (the vera.rs light
//! client's module state root) carries that state in its key, which makes it
//! exact: the service caches it, bounded by bytes, and a new state is a new
//! key. Every other decision lives in the access cache
//! (`access_cache::AccessCache`), which its invalidation events reach. A
//! circuit-breaker refusal is an error, so it is never cached anywhere.

use std::sync::Arc;
use std::time::Duration;

use defra_core::thread_bounds::MaybeBoxFuture;

use crate::provider::{ProviderError, VeraProvider};

/// One decision: who, on what, for which permission.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DecisionKey {
    pub(crate) actor: String,
    pub(crate) policy: String,
    pub(crate) resource: String,
    pub(crate) object: String,
    pub(crate) permission: String,
    /// The verified state the decision is read at, when the provider proves
    /// one.
    pub(crate) state: Option<String>,
}

pub(crate) struct RemoteDecisions {
    pub(crate) provider: Arc<dyn VeraProvider>,
}

/// One call per decision (the provider has no batch call), as many at once
/// as the cap allows.
pub(crate) fn config(request_timeout: Duration) -> inputs::Config {
    inputs::Config {
        deadline: request_timeout.saturating_mul(2),
        attempts: 2,
        backoff: Duration::from_millis(100),
        max_backoff: Duration::from_secs(1),
        max_batch: 1,
        flights_per_group: 32,
        max_in_flight: 32,
        cache_bytes: 4 << 20,
        ttl: None,
    }
}

impl inputs::Source for RemoteDecisions {
    type Key = DecisionKey;
    type Group = ();
    type Value = bool;
    type Error = Arc<ProviderError>;

    fn name(&self) -> &'static str {
        "acp-decisions"
    }

    fn group(&self, _key: &DecisionKey) {}

    fn fetch<'a>(
        &'a self,
        _group: &'a (),
        keys: &'a [DecisionKey],
    ) -> MaybeBoxFuture<'a, inputs::Answers<bool, Arc<ProviderError>>> {
        Box::pin(async move {
            let mut answers = Vec::with_capacity(keys.len());
            for key in keys {
                let answer = self
                    .provider
                    .verify_access(
                        &key.policy,
                        &key.resource,
                        &key.object,
                        &key.permission,
                        &key.actor,
                    )
                    .await
                    .map_err(Arc::new);
                // The only key of the round trip failed in a way worth
                // another call: the round trip goes again.
                if keys.len() == 1 {
                    if let Err(error) = &answer {
                        if self.retryable(error) {
                            return Err(Arc::clone(error));
                        }
                    }
                }
                answers.push(answer);
            }
            Ok(answers)
        })
    }

    /// A timeout or a refusal of an open circuit: the next call may reach
    /// Vera (an open circuit refuses at once, so trying costs nothing).
    fn retryable(&self, error: &Arc<ProviderError>) -> bool {
        matches!(error.as_ref(), ProviderError::Unavailable(_))
    }

    /// Only a decision keyed by a verified state is exact enough to cache.
    fn cache_bytes(&self, key: &DecisionKey, _value: &bool) -> Option<usize> {
        key.state.as_ref().map(|state| {
            key.actor.len()
                + key.policy.len()
                + key.resource.len()
                + key.object.len()
                + key.permission.len()
                + state.len()
        })
    }
}

#[cfg(test)]
#[path = "decisions_tests.rs"]
mod tests;
