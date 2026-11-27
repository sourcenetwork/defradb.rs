//! The interlock between a route being added and a create on the node's own
//! deployment (`proofs/tla/ForeignPolicyCreate.tla`).
//!
//! An add claims its policy and then asks whether a create of it is in
//! flight; a create marks itself in flight and then asks whether its policy
//! is claimed or routed. Both mark first and look second, in one total
//! order of those steps, so of an add and a create of one policy that
//! overlap at least one sees the other and is refused: the create before
//! anything is sent, the add before it routes. A create that ended before
//! the add looked left the policy where the add's own read sees it
//! ([`crate::provider::VeraProvider::create_policy`]).
//!
//! A create whose policy ID is not known before it is sent may make any
//! policy: it counts as in flight for every one, and is refused while any
//! is claimed or routed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

/// The policies adds are routing now and creates are making now, each by
/// how many: an entry is kept once made, one per policy ever claimed or
/// created on this node.
#[derive(Default)]
pub(crate) struct Claims {
    claims: HopscotchMap<String, Arc<AtomicUsize>, RandomState>,
    creating: HopscotchMap<String, Arc<AtomicUsize>, RandomState>,
    unnamed: Arc<AtomicUsize>,
}

/// One add or create in progress, until it is dropped.
pub(crate) struct Held {
    count: Arc<AtomicUsize>,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Held {
    fn on(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self { count }
    }
}

impl Claims {
    /// Mark `policy` (in its one spelling) as being routed.
    pub(crate) fn claim(&self, policy: String) -> Held {
        Held::on(self.claims.get_or_insert(policy, Arc::default()))
    }

    /// Whether an add is routing `policy` now.
    pub(crate) fn claimed(&self, policy: &str) -> bool {
        self.claims
            .get(policy)
            .is_some_and(|count| count.load(Ordering::SeqCst) > 0)
    }

    /// Whether an add is routing any policy now.
    pub(crate) fn any_claimed(&self) -> bool {
        self.claims
            .iter()
            .any(|(_, count)| count.load(Ordering::SeqCst) > 0)
    }

    /// Mark a create of `policy`, or of a policy not known yet, as in
    /// flight.
    pub(crate) fn create(&self, policy: Option<String>) -> Held {
        match policy {
            Some(policy) => Held::on(self.creating.get_or_insert(policy, Arc::default())),
            None => Held::on(Arc::clone(&self.unnamed)),
        }
    }

    /// Whether a create in flight may make `policy`: one of it, or one of
    /// a policy not known yet.
    pub(crate) fn creating(&self, policy: &str) -> bool {
        self.unnamed.load(Ordering::SeqCst) > 0
            || self
                .creating
                .get(policy)
                .is_some_and(|count| count.load(Ordering::SeqCst) > 0)
    }
}
