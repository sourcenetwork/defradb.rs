//! One immutable policy snapshot per ACP instance, shared by every
//! transaction and scope bound from it and swapped whole on change, so a check
//! sees the old or the new policies and never a mix. No epoch guard survives
//! an await.
use async_trait::async_trait;
use regolith::sync::Atom;
use std::sync::Arc;
use zanzibar::thread_bounds::MaybeSendSync;
use zanzibar::{engine::PermissionEngine, lookup::PolicyLookupTable, store::ZanzibarStore, Policy};

/// Reads policies as committed, never through a caller's open transaction, so
/// only committed policies are published to the shared snapshot.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
trait CommittedPolicies: MaybeSendSync {
    async fn get_policy(&self, policy_id: &str) -> zanzibar::error::Result<Option<Policy>>;
}

struct Committed<S: ?Sized>(Arc<S>);

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> CommittedPolicies for Committed<S>
where
    S: ZanzibarStore + ?Sized,
{
    async fn get_policy(&self, policy_id: &str) -> zanzibar::error::Result<Option<Policy>> {
        self.0.get_policy(policy_id).await
    }
}

struct SharedPolicies {
    current: Atom<Arc<PolicyLookupTable>>,
    committed: Arc<dyn CommittedPolicies>,
}

pub(crate) struct PolicyCache<S: ZanzibarStore + ?Sized> {
    store: Arc<S>,
    shared: Arc<SharedPolicies>,
}

impl<S> PolicyCache<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// A snapshot whose committed policies are read from `store`.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub(crate) fn new(store: Arc<S>) -> Self {
        Self {
            shared: Arc::new(SharedPolicies {
                current: Atom::new(Arc::new(PolicyLookupTable::new())),
                committed: Arc::new(Committed(store.clone())),
            }),
            store,
        }
    }

    /// The same snapshot, evaluated against `store` (a transaction or a scope).
    pub(crate) fn sharing<T>(&self, store: Arc<T>) -> PolicyCache<T>
    where
        T: ZanzibarStore + ?Sized,
    {
        PolicyCache {
            store,
            shared: self.shared.clone(),
        }
    }

    pub(crate) fn snapshot(&self) -> PermissionEngine<S> {
        PermissionEngine::with_lookup(self.store.clone(), self.shared.current.load_clone())
    }

    pub(crate) fn edit(&self, edit: impl Fn(&mut PermissionEngine<S>)) {
        self.shared.current.rcu(|old| {
            let mut next = PermissionEngine::with_lookup(self.store.clone(), old.clone());
            edit(&mut next);
            next.lookup
        });
    }

    // A fetch begun before invalidation/reload must not publish over it.
    pub(crate) fn publish_if_current(
        &self,
        expected: &PermissionEngine<S>,
        next: PermissionEngine<S>,
    ) -> bool {
        let current = self.shared.current.load();
        if !Arc::ptr_eq(&current, &expected.lookup) {
            return false;
        }
        self.shared
            .current
            .compare_and_swap(&current, next.lookup)
            .is_ok()
    }

    /// The committed policy under `policy_id`, read outside any transaction.
    pub(crate) async fn committed_policy(
        &self,
        policy_id: &str,
    ) -> zanzibar::error::Result<Option<Policy>> {
        self.shared.committed.get_policy(policy_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zanzibar::{MemoryZanzibarStore, Policy};

    #[test]
    fn invalidation_rejects_stale_reload_and_keeps_existing_snapshot_immutable() {
        let cache = PolicyCache::new(Arc::new(MemoryZanzibarStore::new()));
        cache.edit(|engine| engine.add_policy(&Policy::new("policy", "old")));
        let before_fetch = cache.snapshot();
        let mut fetched = before_fetch.clone();
        fetched.add_policy(&Policy::new("policy", "stale"));
        cache.edit(|engine| engine.remove_policy("policy"));
        assert!(!cache.publish_if_current(&before_fetch, fetched));
        assert!(!cache.snapshot().lookup.has_policy("policy"));
        assert!(before_fetch.lookup.has_policy("policy"));
    }

    #[test]
    fn independent_policy_publications_do_not_lose_updates() {
        let cache = PolicyCache::new(Arc::new(MemoryZanzibarStore::new()));
        std::thread::scope(|scope| {
            for id in 0..16 {
                let cache = &cache;
                scope.spawn(move || {
                    let policy = Policy::new(id.to_string(), "policy");
                    cache.edit(|engine| engine.add_policy(&policy));
                });
            }
        });
        let snapshot = cache.snapshot();
        for id in 0..16 {
            assert!(snapshot.lookup.has_policy(&id.to_string()));
        }
    }
}
