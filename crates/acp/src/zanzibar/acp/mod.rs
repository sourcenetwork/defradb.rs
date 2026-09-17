//! Zanzibar-based DocumentACP implementation.

mod document_acp;

use std::sync::Arc;

use identity::Did;
use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use zanzibar::engine::PermissionEngine;
use zanzibar::expression::RelationExpression;
use zanzibar::store::ZanzibarStore;
use zanzibar::types::{Policy, Relation, Resource};

use crate::error::{Error, Result};
use crate::permission::DocumentPermission;

pub const OWNER_RELATION: &str = "owner";
pub const READER_RELATION: &str = "reader";
pub const UPDATER_RELATION: &str = "updater";
pub const DELETER_RELATION: &str = "deleter";
pub const ADMIN_RELATION: &str = "admin";

pub struct ZanzibarDocumentACP<S: ZanzibarStore + ?Sized + Send + Sync + 'static> {
    store: Arc<S>,
    // The engine is rebuilt as a unit from `policies` (the canonical,
    // incrementally-updated cache) on every add/remove/reload, and always
    // swapped in with `rcu` so a rebuild observed via a CAS retry is
    // guaranteed (by the load/CAS happens-before edge) to see every insert
    // that predates it, even ones from other racing callers.
    engine: Atom<Arc<PermissionEngine<S>>>,
    policies: HopscotchMap<String, Policy, RandomState>,
}

impl<S: ZanzibarStore + ?Sized + Send + Sync + 'static> ZanzibarDocumentACP<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store: store.clone(),
            engine: Atom::new(Arc::new(PermissionEngine::new(store))),
            policies: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    fn rebuild_engine(&self) -> PermissionEngine<S> {
        let mut engine = PermissionEngine::new(self.store.clone());
        for policy in self.policies.values() {
            engine.add_policy(&policy);
        }
        engine
    }

    pub fn create_default_policy(policy_id: &str, resource_name: &str) -> Policy {
        Policy::new(policy_id, format!("Policy for {}", resource_name)).with_resource(
            Resource::new(resource_name)
                .with_relation(Relation::direct(OWNER_RELATION))
                .with_relation(
                    Relation::computed(
                        ADMIN_RELATION,
                        RelationExpression::union(vec![
                            RelationExpression::this(),
                            RelationExpression::computed_userset(OWNER_RELATION),
                        ]),
                    )
                    .with_manages(vec![
                        READER_RELATION,
                        UPDATER_RELATION,
                        DELETER_RELATION,
                    ]),
                )
                .with_relation(Relation::direct(READER_RELATION))
                .with_relation(Relation::direct(UPDATER_RELATION))
                .with_relation(Relation::direct(DELETER_RELATION))
                .with_relation(Relation::computed(
                    "read",
                    RelationExpression::union(vec![
                        RelationExpression::computed_userset(OWNER_RELATION),
                        RelationExpression::computed_userset(ADMIN_RELATION),
                        RelationExpression::computed_userset(READER_RELATION),
                        RelationExpression::computed_userset(UPDATER_RELATION),
                        RelationExpression::computed_userset(DELETER_RELATION),
                    ]),
                ))
                .with_relation(Relation::computed(
                    "update",
                    RelationExpression::union(vec![
                        RelationExpression::computed_userset(OWNER_RELATION),
                        RelationExpression::computed_userset(ADMIN_RELATION),
                        RelationExpression::computed_userset(UPDATER_RELATION),
                    ]),
                ))
                .with_relation(Relation::computed(
                    "delete",
                    RelationExpression::union(vec![
                        RelationExpression::computed_userset(OWNER_RELATION),
                        RelationExpression::computed_userset(ADMIN_RELATION),
                        RelationExpression::computed_userset(DELETER_RELATION),
                    ]),
                )),
        )
    }

    async fn ensure_policy(&self, policy_id: &str, resource_name: &str) -> Result<()> {
        if self.policies.contains_key(policy_id) {
            return Ok(());
        }

        let policy = if let Some(policy) = self.store.get_policy(policy_id).await? {
            policy
        } else {
            let policy = Self::create_default_policy(policy_id, resource_name);
            self.store.store_policy(&policy).await?;
            policy
        };
        self.policies.insert(policy_id.to_string(), policy);
        self.engine.rcu(|_| Arc::new(self.rebuild_engine()));

        Ok(())
    }

    async fn is_owner(
        &self,
        subject: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        Ok(self
            .store
            .check_permission_direct(policy_id, resource_name, doc_id, OWNER_RELATION, subject)
            .await?)
    }

    async fn check_manage_relation(
        &self,
        subject: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        target_relation: &str,
        operation: &str,
    ) -> Result<()> {
        if self
            .is_owner(subject, policy_id, resource_name, doc_id)
            .await?
        {
            return Ok(());
        }

        let policy = match self.store.get_policy(policy_id).await? {
            Some(p) => p,
            None => {
                return Err(Error::NotOwner {
                    operation: format!("{} actor relationship", operation),
                });
            }
        };

        let managers = policy.get_managers_for_relation(resource_name, target_relation);
        let has_managers = !managers.is_empty();

        for manager_relation in managers {
            let has_manager = self
                .store
                .check_permission_direct(
                    policy_id,
                    resource_name,
                    doc_id,
                    manager_relation,
                    subject,
                )
                .await?;

            if has_manager {
                tracing::debug!(
                    target: "acp::audit",
                    event = "manager_authorized",
                    subject = %subject,
                    manager_relation = %manager_relation,
                    target_relation = %target_relation,
                    collection = %resource_name,
                    doc_id = %doc_id,
                    "Subject authorized via manager relation"
                );
                return Ok(());
            }
        }

        if has_managers {
            Err(Error::NotManager {
                operation: format!("{} relationship", operation),
            })
        } else {
            Err(Error::NotOwner {
                operation: format!("{} actor relationship", operation),
            })
        }
    }

    fn permission_to_relation(permission: DocumentPermission) -> &'static str {
        match permission {
            DocumentPermission::Read => "read",
            DocumentPermission::Update => "update",
            DocumentPermission::Delete => "delete",
        }
    }

    pub async fn invalidate_policy_cache(&self, policy_id: &str) {
        self.policies.force_remove(policy_id);
        self.engine.rcu(|_| Arc::new(self.rebuild_engine()));
    }

    pub async fn reload_policy(&self, policy_id: &str) -> Result<()> {
        self.policies.force_remove(policy_id);
        if let Some(policy) = self.store.get_policy(policy_id).await? {
            self.policies.insert(policy_id.to_string(), policy);
        }
        self.engine.rcu(|_| Arc::new(self.rebuild_engine()));
        Ok(())
    }

    pub async fn clear_policy_cache(&self) {
        self.policies.clear();
        self.engine.rcu(|_| Arc::new(self.rebuild_engine()));
    }
}
