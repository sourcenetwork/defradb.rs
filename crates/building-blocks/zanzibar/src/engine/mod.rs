mod cache;
mod evaluate;
mod limits;
pub use limits::{MAX_EVALUATION_DEPTH, MAX_EVALUATION_STEPS};
mod trace;

use std::sync::Arc;

use crate::did::Did;

use crate::error::Result;
use crate::lookup::PolicyLookupTable;
use crate::store::ZanzibarStore;
use crate::types::Policy;

use cache::{CheckCache, NodeId, NodeTrail};

#[derive(Debug, Clone)]
pub struct PermissionCheckRequest<'a> {
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object_id: &'a str,
    pub relation: &'a str,
    pub subject: &'a Did,
}

impl<'a> PermissionCheckRequest<'a> {
    pub fn new(
        policy_id: &'a str,
        resource: &'a str,
        object_id: &'a str,
        relation: &'a str,
        subject: &'a Did,
    ) -> Self {
        Self {
            policy_id,
            resource,
            object_id,
            relation,
            subject,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionExplanation {
    pub granted: bool,
    pub resource: String,
    pub object_id: String,
    pub relation: String,
    pub subject: String,
    pub trace: EvaluationTrace,
}

#[derive(Debug, Clone, Default)]
pub struct EvaluationTrace {
    pub steps: Vec<EvaluationStep>,
}

impl EvaluationTrace {
    pub(crate) fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub(crate) fn add_step(&mut self, step: EvaluationStep) {
        self.steps.push(step);
    }
}

#[derive(Debug, Clone)]
pub struct EvaluationStep {
    pub expression_type: String,
    pub resource: String,
    pub object_id: String,
    pub relation: String,
    pub result: StepResult,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StepResult {
    Granted,
    Denied,
    Skipped,
    Continuing,
}

impl std::fmt::Display for StepResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepResult::Granted => write!(f, "GRANTED"),
            StepResult::Denied => write!(f, "DENIED"),
            StepResult::Skipped => write!(f, "SKIPPED"),
            StepResult::Continuing => write!(f, "..."),
        }
    }
}

pub struct PermissionEngine<S: ZanzibarStore + ?Sized> {
    store: Arc<S>,
    pub lookup: Arc<PolicyLookupTable>,
}

impl<S: ZanzibarStore + ?Sized> Clone for PermissionEngine<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            lookup: self.lookup.clone(),
        }
    }
}

impl<S: ZanzibarStore + ?Sized> PermissionEngine<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            lookup: Arc::new(PolicyLookupTable::new()),
        }
    }

    /// Build an evaluator over one immutable policy snapshot.
    pub fn with_lookup(store: Arc<S>, lookup: Arc<PolicyLookupTable>) -> Self {
        Self { store, lookup }
    }

    pub fn add_policy(&mut self, policy: &Policy) {
        Arc::make_mut(&mut self.lookup).add_policy(policy);
    }

    pub fn remove_policy(&mut self, policy_id: &str) {
        Arc::make_mut(&mut self.lookup).remove_policy(policy_id);
    }

    pub fn update_policy(&mut self, policy: &Policy) {
        Arc::make_mut(&mut self.lookup).update_policy(policy);
    }

    pub async fn load_policy(&mut self, policy_id: &str) -> Result<()> {
        if let Some(policy) = self.store.get_policy(policy_id).await? {
            Arc::make_mut(&mut self.lookup).add_policy(&policy);
        }
        Ok(())
    }

    pub async fn reload_policy(&mut self, policy_id: &str) -> Result<()> {
        Arc::make_mut(&mut self.lookup).remove_policy(policy_id);
        self.load_policy(policy_id).await
    }

    pub fn clear_cache(&mut self) {
        Arc::make_mut(&mut self.lookup).clear();
    }

    /// Evaluate with deterministic depth and work limits; exhaustion returns an error.
    pub async fn check(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        let expression = self.lookup.get_expression(policy_id, resource, relation)?;

        let node_id = NodeId::new(resource, object_id, relation);
        let trail = NodeTrail::new().with_node(node_id);

        let cache = Arc::new(CheckCache::new());

        let (granted, _tainted) = self
            .evaluate_expr_cached(
                policy_id, resource, object_id, relation, subject, expression, trail, cache,
            )
            .await?;
        Ok(granted)
    }

    /// Synchronous, runtime-free form of [`check`](Self::check) for
    /// consensus / deterministic evaluation (e.g. on-chain or replicated-log
    /// authorization), where no async reactor is available and every validator
    /// must independently reach the identical decision.
    ///
    /// Polls the [`check`](Self::check) future exactly once with a no-op
    /// waker, so it never parks the thread.
    ///
    /// # Contract
    ///
    /// The backing [`ZanzibarStore`] must honour:
    /// - **all-Ready**: every store future resolves on its first poll, with no
    ///   real I/O, timers, or network. A check that has not finished after
    ///   that one poll fails with [`Error::EvaluationPending`] instead of
    ///   waiting.
    /// - **side-effect-free**: the check performs reads only and must not
    ///   mutate observable state.
    /// - **order-stable**: policy and relationship traversal is deterministically
    ///   ordered, so identical inputs yield the identical decision on every node.
    ///
    /// `MemoryZanzibarStore` satisfies all three.
    ///
    /// [`Error::EvaluationPending`]: crate::error::Error::EvaluationPending
    pub fn check_blocking(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        let mut check =
            std::pin::pin!(self.check(policy_id, resource, object_id, relation, subject));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match std::future::Future::poll(check.as_mut(), &mut context) {
            std::task::Poll::Ready(decision) => decision,
            std::task::Poll::Pending => Err(crate::error::Error::EvaluationPending),
        }
    }

    /// Evaluate a batch with one shared cache and work budget.
    pub async fn check_many(&self, requests: &[PermissionCheckRequest<'_>]) -> Vec<Result<bool>> {
        let cache = Arc::new(CheckCache::new());

        let mut results = Vec::with_capacity(requests.len());

        for req in requests {
            let result = self
                .check_with_cache(
                    req.policy_id,
                    req.resource,
                    req.object_id,
                    req.relation,
                    req.subject,
                    cache.clone(),
                )
                .await;
            results.push(result);
        }

        results
    }

    async fn check_with_cache(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
        cache: Arc<CheckCache>,
    ) -> Result<bool> {
        let expression = self.lookup.get_expression(policy_id, resource, relation)?;

        let node_id = NodeId::new(resource, object_id, relation);
        let trail = NodeTrail::new().with_node(node_id);

        let (granted, _tainted) = self
            .evaluate_expr_cached(
                policy_id, resource, object_id, relation, subject, expression, trail, cache,
            )
            .await?;
        Ok(granted)
    }

    pub async fn explain(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<PermissionExplanation> {
        let expression = self.lookup.get_expression(policy_id, resource, relation)?;

        let node_id = NodeId::new(resource, object_id, relation);
        let trail = NodeTrail::new().with_node(node_id);

        let cache = Arc::new(CheckCache::new());
        let mut trace = EvaluationTrace::new();

        let granted = self
            .evaluate_expr_with_trace(
                policy_id, resource, object_id, relation, subject, expression, trail, cache,
                &mut trace,
            )
            .await?;

        Ok(PermissionExplanation {
            granted,
            resource: resource.to_string(),
            object_id: object_id.to_string(),
            relation: relation.to_string(),
            subject: subject.to_string(),
            trace,
        })
    }
}

#[cfg(test)]
mod check_blocking_tests {
    use std::sync::Arc;

    use crate::store::{MemoryZanzibarStore, ZanzibarStore};
    use crate::types::{Policy, Relation, Relationship, Resource};
    use crate::Did;

    use super::PermissionEngine;

    fn owner_did() -> Did {
        Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap()
    }
    fn stranger_did() -> Did {
        Did::new("did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH").unwrap()
    }

    // A plain `#[test]`, deliberately not an async test: there is no async
    // runtime present. This is the consensus-evaluation contract: `check_blocking`
    // must drive an all-Ready, side-effect-free store to a decision synchronously,
    // with no reactor, so independent validators reach the same answer.
    #[test]
    fn check_blocking_resolves_decision_without_a_runtime() {
        let store = Arc::new(MemoryZanzibarStore::new());
        let mut engine = PermissionEngine::new(store.clone());

        let policy = Policy::new("policy1", "Test")
            .with_resource(Resource::new("document").with_relation(Relation::direct("owner")));
        engine.add_policy(&policy);

        let owner = owner_did();
        let rel = Relationship::with_entity("document", "doc1", "owner", owner.clone());
        futures::executor::block_on(store.store_relationship("policy1", &rel)).unwrap();

        // Same decisions `check` would return (owner granted, stranger denied),
        // produced synchronously.
        assert!(engine
            .check_blocking("policy1", "document", "doc1", "owner", &owner)
            .unwrap());
        assert!(!engine
            .check_blocking("policy1", "document", "doc1", "owner", &stranger_did())
            .unwrap());
    }

    /// A store whose direct-permission read is not ready on its first poll.
    struct YieldingStore(MemoryZanzibarStore);

    struct YieldOnce(bool);

    impl std::future::Future for YieldOnce {
        type Output = ();

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            if self.0 {
                return std::task::Poll::Ready(());
            }
            self.0 = true;
            context.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl ZanzibarStore for YieldingStore {
        async fn store_policy(&self, policy: &Policy) -> crate::error::Result<()> {
            self.0.store_policy(policy).await
        }
        async fn get_policy(&self, policy_id: &str) -> crate::error::Result<Option<Policy>> {
            self.0.get_policy(policy_id).await
        }
        async fn list_policies(&self) -> crate::error::Result<Vec<Policy>> {
            self.0.list_policies().await
        }
        async fn next_policy_counter(&self) -> crate::error::Result<u64> {
            self.0.next_policy_counter().await
        }
        async fn delete_policy(&self, policy_id: &str) -> crate::error::Result<bool> {
            self.0.delete_policy(policy_id).await
        }
        async fn store_relationship(
            &self,
            policy_id: &str,
            rel: &Relationship,
        ) -> crate::error::Result<()> {
            self.0.store_relationship(policy_id, rel).await
        }
        async fn delete_relationship(
            &self,
            policy_id: &str,
            rel: &Relationship,
        ) -> crate::error::Result<bool> {
            self.0.delete_relationship(policy_id, rel).await
        }
        async fn has_relationship(
            &self,
            policy_id: &str,
            resource: &str,
            object_id: &str,
            relation: &str,
            subject: &crate::types::Subject,
        ) -> crate::error::Result<bool> {
            self.0
                .has_relationship(policy_id, resource, object_id, relation, subject)
                .await
        }
        async fn check_permission_direct(
            &self,
            policy_id: &str,
            resource: &str,
            object_id: &str,
            relation: &str,
            subject: &Did,
        ) -> crate::error::Result<bool> {
            YieldOnce(false).await;
            self.0
                .check_permission_direct(policy_id, resource, object_id, relation, subject)
                .await
        }
        async fn get_relation_subjects(
            &self,
            policy_id: &str,
            resource: &str,
            object_id: &str,
            relation: &str,
        ) -> crate::error::Result<Vec<crate::types::Subject>> {
            self.0
                .get_relation_subjects(policy_id, resource, object_id, relation)
                .await
        }
        async fn get_relation_targets(
            &self,
            policy_id: &str,
            resource: &str,
            object_id: &str,
            relation: &str,
        ) -> crate::error::Result<Vec<crate::types::ObjectRef>> {
            self.0
                .get_relation_targets(policy_id, resource, object_id, relation)
                .await
        }
        async fn delete_object_relationships(
            &self,
            policy_id: &str,
            resource: &str,
            object_id: &str,
        ) -> crate::error::Result<()> {
            self.0
                .delete_object_relationships(policy_id, resource, object_id)
                .await
        }
    }

    // The thread is never parked: a store read that is not ready fails the
    // synchronous check at once, while the asynchronous check still decides.
    #[test]
    fn check_blocking_fails_loud_when_the_store_is_not_ready() {
        let store = Arc::new(YieldingStore(MemoryZanzibarStore::new()));
        let mut engine = PermissionEngine::new(store.clone());
        engine.add_policy(
            &Policy::new("policy1", "Test")
                .with_resource(Resource::new("document").with_relation(Relation::direct("owner"))),
        );
        let owner = owner_did();
        let rel = Relationship::with_entity("document", "doc1", "owner", owner.clone());
        futures::executor::block_on(store.store_relationship("policy1", &rel)).unwrap();

        assert!(matches!(
            engine.check_blocking("policy1", "document", "doc1", "owner", &owner),
            Err(crate::error::Error::EvaluationPending)
        ));
        assert!(futures::executor::block_on(
            engine.check("policy1", "document", "doc1", "owner", &owner)
        )
        .unwrap());
    }
}
