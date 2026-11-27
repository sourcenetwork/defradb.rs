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

type Counts = HopscotchMap<String, usize, RandomState>;
type Tracked = (Arc<Counts>, String);

/// The most adds or creates that may be in flight at once.
pub(crate) const MAX_IN_FLIGHT: usize = 1024;

/// The policies adds are routing now and creates are making now, each by
/// how many.
pub(crate) struct Claims {
    claims: Arc<Counts>,
    creating: Arc<Counts>,
    unnamed: Arc<AtomicUsize>,
    claim_slots: Arc<AtomicUsize>,
    create_slots: Arc<AtomicUsize>,
    limit: usize,
}

impl Default for Claims {
    fn default() -> Self {
        Self {
            claims: Arc::default(),
            creating: Arc::default(),
            unnamed: Arc::default(),
            claim_slots: Arc::default(),
            create_slots: Arc::default(),
            limit: MAX_IN_FLIGHT,
        }
    }
}

/// One add or create in progress, until it is dropped.
pub(crate) struct Held {
    tracked: Option<Tracked>,
    unnamed: Option<Arc<AtomicUsize>>,
    slots: Arc<AtomicUsize>,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some((map, key)) = &self.tracked {
            map.compute(key.clone(), |count| match count {
                Some(count) if *count > 1 => Some(count - 1),
                Some(_) | None => None,
            });
        }
        if let Some(unnamed) = &self.unnamed {
            unnamed.fetch_sub(1, Ordering::SeqCst);
        }
        self.slots.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Why an add or create could not be marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "too many policy {operation} operations are in flight (limit {limit}); retry when one finishes"
)]
pub(crate) struct ClaimsFull {
    pub(crate) operation: &'static str,
    pub(crate) limit: usize,
}

impl Claims {
    #[cfg(test)]
    fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    /// Mark `policy` (in its one spelling) as being routed.
    pub(crate) fn claim(&self, policy: String) -> Result<Held, ClaimsFull> {
        let slots = reserve(&self.claim_slots, self.limit, "add")?;
        self.claims.compute(policy.clone(), |count| {
            Some(count.copied().unwrap_or(0) + 1)
        });
        Ok(Held {
            tracked: Some((Arc::clone(&self.claims), policy)),
            unnamed: None,
            slots,
        })
    }

    /// Whether an add is routing `policy` now.
    pub(crate) fn claimed(&self, policy: &str) -> bool {
        self.claims.get(policy).is_some_and(|count| count > 0)
    }

    /// Whether an add is routing any policy now.
    pub(crate) fn any_claimed(&self) -> bool {
        self.claims.iter().any(|(_, count)| count > 0)
    }

    /// Mark a create of `policy`, or of a policy not known yet, as in
    /// flight.
    pub(crate) fn create(&self, policy: Option<String>) -> Result<Held, ClaimsFull> {
        let slots = reserve(&self.create_slots, self.limit, "create")?;
        let Some(policy) = policy else {
            self.unnamed.fetch_add(1, Ordering::SeqCst);
            return Ok(Held {
                tracked: None,
                unnamed: Some(Arc::clone(&self.unnamed)),
                slots,
            });
        };
        self.creating.compute(policy.clone(), |count| {
            Some(count.copied().unwrap_or(0) + 1)
        });
        Ok(Held {
            tracked: Some((Arc::clone(&self.creating), policy)),
            unnamed: None,
            slots,
        })
    }

    /// Whether a create in flight may make `policy`: one of it, or one of
    /// a policy not known yet.
    pub(crate) fn creating(&self, policy: &str) -> bool {
        self.unnamed.load(Ordering::SeqCst) > 0
            || self.creating.get(policy).is_some_and(|count| count > 0)
    }

    /// The add and create operations in flight now.
    pub(crate) fn sizes(&self) -> (usize, usize) {
        (
            self.claim_slots.load(Ordering::SeqCst),
            self.create_slots.load(Ordering::SeqCst),
        )
    }
}

fn reserve(
    slots: &Arc<AtomicUsize>,
    limit: usize,
    operation: &'static str,
) -> Result<Arc<AtomicUsize>, ClaimsFull> {
    let mut current = slots.load(Ordering::SeqCst);
    loop {
        if current >= limit {
            return Err(ClaimsFull { operation, limit });
        }
        match slots.compare_exchange_weak(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => return Ok(Arc::clone(slots)),
            Err(seen) => current = seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ended_claims_and_creates_leave_no_entries() {
        let claims = Claims::default();
        let first = claims.claim("policy-a".into()).expect("first add");
        let second = claims.claim("policy-a".into()).expect("second add");
        drop(first);
        assert_eq!(claims.claims.len(), 1, "another add still holds the entry");
        drop(second);
        assert_eq!(claims.claims.len(), 0);

        let first = claims
            .create(Some("policy-b".into()))
            .expect("first create");
        let second = claims
            .create(Some("policy-b".into()))
            .expect("second create");
        drop(first);
        assert_eq!(claims.creating.len(), 1, "another create still holds it");
        drop(second);
        assert_eq!(claims.creating.len(), 0);

        drop(claims.create(None).expect("unnamed create"));
        assert_eq!(claims.unnamed.load(Ordering::SeqCst), 0);
    }

    #[test]
    #[mantra_macros::req_test("defra.acp.foreign_policy")]
    fn full_tables_refuse_another_add_or_create_until_one_ends() {
        let claims = Claims::with_limit(2);

        let first = claims.claim("policy-a".into()).expect("first add");
        let second = claims.claim("policy-b".into()).expect("second add");
        assert!(matches!(
            claims.claim("policy-c".into()),
            Err(ClaimsFull {
                operation: "add",
                limit: 2
            })
        ));
        assert_eq!(claims.sizes(), (2, 0));
        drop(first);
        assert_eq!(claims.claims.len(), 1);
        drop(claims.claim("policy-c".into()).expect("room again"));
        drop(second);

        let first = claims
            .create(Some("policy-a".into()))
            .expect("first create");
        let second = claims
            .create(Some("policy-b".into()))
            .expect("second create");
        assert!(matches!(
            claims.create(Some("policy-c".into())),
            Err(ClaimsFull {
                operation: "create",
                limit: 2
            })
        ));
        assert_eq!(claims.sizes(), (0, 2));
        assert_eq!(claims.creating.len(), 2);
        drop(first);
        drop(second);
    }

    #[test]
    #[mantra_macros::req_test("defra.acp.foreign_policy")]
    fn concurrent_reacquisition_leaves_no_orphaned_entries() {
        let claims = Arc::new(Claims::default());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let claims = Arc::clone(&claims);
                scope.spawn(move || {
                    for _ in 0..2_000 {
                        drop(claims.claim("policy-a".into()).expect("add"));
                        drop(claims.create(Some("policy-b".into())).expect("create"));
                    }
                });
            }
        });
        assert_eq!(claims.sizes(), (0, 0));
        assert!(claims.claims.is_empty());
        assert!(claims.creating.is_empty());
        assert_eq!(claims.unnamed.load(Ordering::SeqCst), 0);
    }
}
