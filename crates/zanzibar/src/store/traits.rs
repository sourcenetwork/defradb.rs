use async_trait::async_trait;

use crate::did::Did;
use crate::thread_bounds::{MaybeBoxFuture, MaybeSendSync};

use super::StorePolicyOptions;
use crate::error::Result;
use crate::types::{ObjectRef, Policy, Relationship, Subject};

/// Trait for Zanzibar policy and relationship storage.
///
/// Provides operations for:
/// - Policy storage and retrieval
/// - Relationship tuple storage
/// - Relationship queries (for permission evaluation)
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ZanzibarStore: MaybeSendSync {
    /// Whether this store participates in an application's storage transaction.
    fn supports_transactions(&self) -> bool {
        false
    }

    /// Open one snapshot for a complete standalone authorization operation.
    /// A store already bound to a caller's transaction returns `None`.
    async fn begin_scope(&self, _readonly: bool) -> Result<Option<Box<dyn ZanzibarScope>>> {
        Ok(None)
    }

    /// Run `attempt` on a fresh write scope per try and commit the try that
    /// wins; a lost commit runs it again, `max_attempts` tries in all. The
    /// inner result is the winning try's value or the refusal that ended it.
    /// A store already bound to a caller's transaction returns `None`.
    async fn transact(
        &self,
        _max_attempts: u32,
        _attempt: ScopeAttempt,
    ) -> Result<Option<ScopeOutcome>> {
        Ok(None)
    }

    async fn store_policy(&self, policy: &Policy) -> Result<()>;

    async fn store_policy_with_options(
        &self,
        policy: &Policy,
        options: &StorePolicyOptions,
    ) -> Result<()> {
        if options.validate {
            policy.validate()?;
        }
        if options.enforce_dpi {
            policy.validate_dpi()?;
        }
        self.store_policy(policy).await
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>>;

    /// Whether a policy is stored under `policy_id`, without parsing it.
    async fn has_policy(&self, policy_id: &str) -> Result<bool> {
        Ok(self.get_policy(policy_id).await?.is_some())
    }

    async fn list_policies(&self) -> Result<Vec<Policy>>;

    /// Return the next policy-ID counter value, advancing the stored counter.
    async fn next_policy_counter(&self) -> Result<u64>;

    async fn delete_policy(&self, policy_id: &str) -> Result<bool>;

    async fn store_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<()>;

    async fn delete_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<bool>;

    async fn has_relationship(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Subject,
    ) -> Result<bool>;

    async fn check_permission_direct(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool>;

    async fn get_relation_subjects(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<Subject>>;

    async fn get_relation_targets(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<ObjectRef>>;

    async fn delete_object_relationships(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<()>;
}

/// Owns commit/discard for a standalone authorization operation. Dropping the
/// scope without committing must discard every staged change.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ZanzibarScope: crate::thread_bounds::MaybeSend {
    fn store(&self) -> std::sync::Arc<dyn ZanzibarStore>;
    async fn commit(self: Box<Self>) -> Result<()>;
}

/// What one try of [`ZanzibarStore::transact`] ends with, boxed so a store
/// carries any caller's value or refusal.
#[cfg(not(target_arch = "wasm32"))]
pub type ScopeValue = Box<dyn std::any::Any + Send>;
#[cfg(target_arch = "wasm32")]
pub type ScopeValue = Box<dyn std::any::Any>;

/// The winning try's value, or the refusal that ended the transaction
/// without a retry.
pub type ScopeOutcome = std::result::Result<ScopeValue, ScopeValue>;

/// One try of [`ZanzibarStore::transact`], run on that try's scope.
#[cfg(not(target_arch = "wasm32"))]
pub type ScopeAttempt = Box<
    dyn FnMut(std::sync::Arc<dyn ZanzibarStore>) -> MaybeBoxFuture<'static, ScopeOutcome> + Send,
>;
#[cfg(target_arch = "wasm32")]
pub type ScopeAttempt =
    Box<dyn FnMut(std::sync::Arc<dyn ZanzibarStore>) -> MaybeBoxFuture<'static, ScopeOutcome>>;
