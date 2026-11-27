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

type Counts = HopscotchMap<String, Arc<AtomicUsize>, RandomState>;
type Tracked<'a> = (&'a Counts, String);

/// The policies adds are routing now and creates are making now, each by
/// how many.
#[derive(Default)]
pub(crate) struct Claims {
    claims: Counts,
    creating: Counts,
    unnamed: Arc<AtomicUsize>,
}

/// One add or create in progress, until it is dropped.
pub(crate) struct Held<'a> {
    count: Arc<AtomicUsize>,
    tracked: Option<Tracked<'a>>,
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 && self.count.load(Ordering::SeqCst) == 0
        {
            if let Some((map, key)) = &self.tracked {
                map.remove_if(key, |held| {
                    Arc::ptr_eq(held, &self.count) && held.load(Ordering::SeqCst) == 0
                });
            }
        }
    }
}

impl<'a> Held<'a> {
    fn untracked(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self {
            count,
            tracked: None,
        }
    }

    fn tracked(map: &'a Counts, key: String) -> Self {
        loop {
            let count = map.get_or_insert(key.clone(), Arc::default());
            count.fetch_add(1, Ordering::SeqCst);
            if map.get(&key).is_some_and(|held| Arc::ptr_eq(&held, &count)) {
                return Self {
                    count,
                    tracked: Some((map, key)),
                };
            }
            count.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl Claims {
    /// Mark `policy` (in its one spelling) as being routed.
    pub(crate) fn claim(&self, policy: String) -> Held<'_> {
        Held::tracked(&self.claims, policy)
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
    pub(crate) fn create(&self, policy: Option<String>) -> Held<'_> {
        match policy {
            Some(policy) => Held::tracked(&self.creating, policy),
            None => Held::untracked(Arc::clone(&self.unnamed)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ended_claims_and_creates_leave_no_entries() {
        let claims = Claims::default();
        let first = claims.claim("policy-a".into());
        let second = claims.claim("policy-a".into());
        drop(first);
        assert_eq!(claims.claims.len(), 1, "another add still holds the entry");
        drop(second);
        assert_eq!(claims.claims.len(), 0);

        let first = claims.create(Some("policy-b".into()));
        let second = claims.create(Some("policy-b".into()));
        drop(first);
        assert_eq!(claims.creating.len(), 1, "another create still holds it");
        drop(second);
        assert_eq!(claims.creating.len(), 0);

        drop(claims.create(None));
        assert_eq!(claims.unnamed.load(Ordering::SeqCst), 0);
    }
}
