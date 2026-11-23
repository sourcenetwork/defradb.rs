//! A bounded, resumable rebuild from the node's recovery ledger.

use async_trait::async_trait;

use super::ExternalDocumentAcp;
use crate::error::{Error, Result};
use crate::outbox::{OwedAuthority, OwedOp, OwedTuple};
use crate::recovery::{
    RecoveryCursor, RecoveryLedger, RecoveryPage, RecoverySource, RecoveryVerifier,
    RECOVERY_PAGE_BYTES,
};

fn add_effect(
    policy_id: &str,
    resource_name: &str,
    object_id: &str,
    relationship: crate::recovery::RecoveryRelationship,
) -> crate::VoidedEffect {
    crate::VoidedEffect::Change(crate::NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: policy_id.to_owned(),
            resource: resource_name.to_owned(),
            object: object_id.to_owned(),
            relation: relationship.relation,
            subject: relationship.subject,
        },
        op: OwedOp::Add,
        actor: relationship.actor,
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    })
}

fn delete_effect(
    policy_id: &str,
    resource_name: &str,
    object_id: &str,
    relationship: crate::recovery::RecoveryRelationship,
) -> crate::VoidedEffect {
    crate::VoidedEffect::Change(crate::NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: policy_id.to_owned(),
            resource: resource_name.to_owned(),
            object: object_id.to_owned(),
            relation: relationship.relation,
            subject: relationship.subject,
        },
        op: OwedOp::Delete,
        actor: relationship.actor,
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    })
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> RecoverySource for ExternalDocumentAcp<S>
where
    S: storage::corekv::Store + 'static,
{
    async fn next(&self, verifier: &dyn RecoveryVerifier) -> Result<Option<RecoveryPage>> {
        let term = verifier.term().await.map_err(Error::Storage)?;
        let txn = self.begin(true).await?;
        let view = txn.acpstore();
        let ledger = RecoveryLedger::new(view.clone());
        let cursor = match ledger.state().await? {
            Some(cursor) => cursor,
            None => RecoveryCursor {
                term,
                cursor: None,
                complete: false,
            },
        };
        if cursor.term != term || cursor.complete {
            return Ok(Some(RecoveryPage {
                effects: Vec::new(),
                cursor: RecoveryCursor {
                    term,
                    cursor: None,
                    complete: true,
                },
                unrestored: Vec::new(),
            }));
        }
        let records = ledger
            .page(cursor.cursor.as_deref(), RECOVERY_PAGE_BYTES)
            .await?;
        let mut effects = Vec::new();
        let mut unrestored = Vec::new();
        let next = match records.last() {
            Some((suffix, _)) => Some(suffix.clone()),
            None => cursor.cursor.clone(),
        };
        for (_, record) in &records {
            let object = record.resolved();
            let Some(owner) = object.owner.clone() else {
                unrestored.push(format!(
                    "{}/{}/{}: no registration was recorded",
                    object.policy_id, object.resource_name, object.object_id
                ));
                continue;
            };
            let (registered, remote_owner) = verifier
                .object_owner(
                    term,
                    &object.policy_id,
                    &object.resource_name,
                    &object.object_id,
                )
                .await
                .map_err(Error::Storage)?;
            if !registered || remote_owner != owner.as_str() {
                effects.push(crate::VoidedEffect::Registration {
                    creator: owner.clone(),
                    policy_id: object.policy_id.clone(),
                    resource: object.resource_name.clone(),
                    object: object.object_id.clone(),
                    authority: OwedAuthority::NodeKey,
                    request_bound: false,
                });
            }
            for relationship in object.relationships {
                let holds = verifier
                    .actor_relationship(
                        term,
                        &object.policy_id,
                        &object.resource_name,
                        &object.object_id,
                        &relationship.relation,
                        &relationship.subject,
                    )
                    .await
                    .map_err(Error::Storage)?;
                if holds {
                    continue;
                }
                effects.push(add_effect(
                    &object.policy_id,
                    &object.resource_name,
                    &object.object_id,
                    relationship,
                ));
            }
            for tombstone in object.tombstones {
                let holds = verifier
                    .actor_relationship(
                        term,
                        &object.policy_id,
                        &object.resource_name,
                        &object.object_id,
                        &tombstone.relation,
                        &tombstone.subject,
                    )
                    .await
                    .map_err(Error::Storage)?;
                if !holds {
                    continue;
                }
                effects.push(delete_effect(
                    &object.policy_id,
                    &object.resource_name,
                    &object.object_id,
                    tombstone,
                ));
            }
        }
        let next_cursor = RecoveryCursor {
            term,
            cursor: next,
            complete: true,
        };
        drop(ledger);
        drop(view);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        Ok(Some(RecoveryPage {
            effects,
            cursor: next_cursor,
            unrestored,
        }))
    }

    async fn delivered(&self, cursor: RecoveryCursor) -> Result<()> {
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        RecoveryLedger::new(view.clone()).set_state(&cursor).await?;
        drop(view);
        txn.commit()
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }
}
use identity::Did;

use crate::recovery::RecoveryOperation;

pub(super) async fn prepare<S>(
    gate: &ExternalDocumentAcp<S>,
    policy_id: &str,
    resource_name: &str,
    object_id: &str,
    operation: RecoveryOperation,
) -> crate::Result<()>
where
    S: storage::corekv::Store + 'static,
{
    let txn = gate.begin(false).await?;
    let view = txn.acpstore();
    RecoveryLedger::new(view.clone())
        .prepare(policy_id, resource_name, object_id, operation)
        .await?;
    drop(view);
    txn.commit()
        .await
        .map_err(|error| Error::Storage(error.to_string()))
}

pub(super) async fn settle<S>(
    gate: &ExternalDocumentAcp<S>,
    policy_id: &str,
    resource_name: &str,
    object_id: &str,
    operation: RecoveryOperation,
    applied: bool,
) -> crate::Result<()>
where
    S: storage::corekv::Store + 'static,
{
    let txn = gate.begin(false).await?;
    let view = txn.acpstore();
    RecoveryLedger::new(view.clone())
        .settle(policy_id, resource_name, object_id, operation, applied)
        .await?;
    drop(view);
    txn.commit()
        .await
        .map_err(|error| Error::Storage(error.to_string()))
}

#[allow(dead_code)]
fn unused(_: &Did) {}
