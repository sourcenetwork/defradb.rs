//! A bounded, resumable rebuild from the node's recovery ledger.

use async_trait::async_trait;
use identity::Did;

use super::ExternalDocumentAcp;
use crate::error::{Error, Result};
use crate::outbox::{OwedAuthority, OwedChange, OwedOp, OwedSubject, OwedTuple};
use crate::recovery::{
    Observed, RecoveryCursor, RecoveryFact, RecoveryLedger, RecoveryPage, RecoverySource,
    RecoveryState, RecoveryVerifier, Slot, RECOVERY_PAGE_BYTES,
};

/// What the deployment says of the object a page is walking.
enum Standing {
    /// No registration of the object was recorded here.
    Unrecorded,
    /// The node unregistered it.
    Unregistered,
    /// The deployment holds no registration of it.
    Missing,
    /// The provider never answered its registration, and the deployment
    /// holds none.
    Unanswered,
    /// The deployment holds it for another owner.
    Foreign,
    /// The deployment holds it for the owner recorded here.
    Held(Did),
}

fn name(fact: &RecoveryFact) -> String {
    format!(
        "{}/{}/{}",
        fact.policy_id, fact.resource_name, fact.object_id
    )
}

fn change(
    fact: &RecoveryFact,
    actor: &Did,
    relation: &str,
    subject: &OwedSubject,
    op: OwedOp,
) -> crate::VoidedEffect {
    crate::VoidedEffect::Change(crate::NewOwedChange {
        txn: None,
        tuple: OwedTuple {
            policy_id: fact.policy_id.clone(),
            resource: fact.resource_name.clone(),
            object: fact.object_id.clone(),
            relation: relation.to_owned(),
            subject: subject.clone(),
        },
        op,
        actor: actor.clone(),
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    })
}

/// One page of a pass, as it fills.
#[derive(Default)]
struct Walk {
    effects: Vec<crate::VoidedEffect>,
    unrestored: Vec<String>,
}

impl Walk {
    /// Ask the deployment how the object of an owner slot stands. With
    /// `report`, what the slot itself needs is owed or reported; a page that
    /// starts inside an object asks without it.
    async fn owner(
        &mut self,
        verifier: &dyn RecoveryVerifier,
        term: u64,
        slot: Option<&Slot>,
        report: bool,
    ) -> Result<Standing> {
        let Some(slot) = slot else {
            return Ok(Standing::Unrecorded);
        };
        let fact = &slot.fact;
        let RecoveryState::Owner { owner } = &fact.state else {
            return Ok(Standing::Unrecorded);
        };
        let remote = verifier
            .object_owner(term, &fact.policy_id, &fact.resource_name, &fact.object_id)
            .await
            .map_err(Error::Storage)?;
        Ok(match (owner, remote) {
            (None, None) => Standing::Unregistered,
            (None, Some(_)) => {
                if report {
                    self.unrestored.push(format!(
                        "{}: the deployment holds a registration this node removed",
                        name(fact)
                    ));
                }
                Standing::Unregistered
            }
            (Some(owner), None) if slot.confirmed => {
                if report {
                    self.effects.push(crate::VoidedEffect::Registration {
                        creator: owner.clone(),
                        policy_id: fact.policy_id.clone(),
                        resource: fact.resource_name.clone(),
                        object: fact.object_id.clone(),
                        authority: OwedAuthority::NodeKey,
                        request_bound: false,
                    });
                }
                Standing::Missing
            }
            (Some(_), None) => {
                if report {
                    self.unrestored.push(format!(
                        "{}: the provider never answered its registration",
                        name(fact)
                    ));
                }
                Standing::Unanswered
            }
            (Some(owner), Some(remote)) if remote == owner.as_str() => {
                Standing::Held(owner.clone())
            }
            (Some(_), Some(remote)) => {
                if report {
                    self.unrestored.push(format!(
                        "{}: the deployment holds it for another owner, {remote}",
                        name(fact)
                    ));
                }
                Standing::Foreign
            }
        })
    }

    /// A grant the deployment lost: owed again when the provider confirmed
    /// it, reported when it never answered.
    fn grant(&mut self, slot: &Slot, actor: &Did, relation: &str, subject: &OwedSubject) {
        if slot.confirmed {
            self.effects
                .push(change(&slot.fact, actor, relation, subject, OwedOp::Add));
        } else {
            self.unrestored.push(format!(
                "{} {relation}: the provider never answered this grant",
                name(&slot.fact)
            ));
        }
    }

    /// Check a relationship slot of an object that stands as `standing`.
    async fn relationship(
        &mut self,
        verifier: &dyn RecoveryVerifier,
        term: u64,
        standing: &Standing,
        slot: &Slot,
    ) -> Result<()> {
        let fact = &slot.fact;
        let RecoveryState::Relationship {
            actor,
            relation,
            subject,
            held,
        } = &fact.state
        else {
            return Ok(());
        };
        match standing {
            Standing::Unrecorded => self.unrestored.push(format!(
                "{} {relation}: no registration was recorded",
                name(fact)
            )),
            // The object is gone here, is not this node's to restore, or was
            // reported with its registration.
            Standing::Unregistered | Standing::Foreign | Standing::Unanswered => {}
            // A deployment without the object holds none of its
            // relationships: every one held is owed behind its registration.
            Standing::Missing => {
                if *held {
                    self.grant(slot, actor, relation, subject);
                }
            }
            Standing::Held(owner) => {
                let observed = verifier
                    .relationship(
                        term,
                        owner,
                        &fact.policy_id,
                        &fact.resource_name,
                        &fact.object_id,
                        relation,
                        subject,
                    )
                    .await
                    .map_err(Error::Storage)?;
                match (observed, *held) {
                    (Observed::Absent, true) => self.grant(slot, actor, relation, subject),
                    (Observed::Held, false) => {
                        self.effects
                            .push(change(fact, actor, relation, subject, OwedOp::Delete));
                    }
                    (Observed::Held, true) | (Observed::Absent, false) => {}
                    (Observed::Unobservable(reason), _) => self
                        .unrestored
                        .push(format!("{} {relation}: {reason}", name(fact))),
                }
            }
        }
        Ok(())
    }
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
        let ledger = RecoveryLedger::new(txn.acpstore());
        let page = self.walk(verifier, term, &ledger).await;
        drop(ledger);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        page
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

impl<S> ExternalDocumentAcp<S>
where
    S: storage::corekv::Store + 'static,
{
    async fn walk(
        &self,
        verifier: &dyn RecoveryVerifier,
        term: u64,
        ledger: &RecoveryLedger,
    ) -> Result<Option<RecoveryPage>> {
        // A cursor of another term checked another deployment state.
        let resumed = ledger
            .state()
            .await?
            .filter(|cursor| cursor.term == term)
            .unwrap_or(RecoveryCursor {
                term,
                cursor: None,
                complete: false,
            });
        if resumed.complete {
            return Ok(None);
        }
        let page = ledger
            .page(resumed.cursor.as_deref(), RECOVERY_PAGE_BYTES)
            .await?;
        let mut walk = Walk::default();
        let mut object: Option<(RecoveryFact, Standing)> = None;
        for slot in &page.slots {
            if matches!(slot.fact.state, RecoveryState::Owner { .. }) {
                let standing = walk.owner(verifier, term, Some(slot), true).await?;
                object = Some((slot.fact.clone(), standing));
                continue;
            }
            let known = object
                .as_ref()
                .is_some_and(|(of, _)| of.same_object(&slot.fact));
            if !known {
                let owner = ledger.owner_of(&slot.fact).await?;
                let standing = walk.owner(verifier, term, owner.as_ref(), false).await?;
                object = Some((slot.fact.clone(), standing));
            }
            if let Some((_, standing)) = &object {
                walk.relationship(verifier, term, standing, slot).await?;
            }
        }
        Ok(Some(RecoveryPage {
            effects: walk.effects,
            cursor: RecoveryCursor {
                term,
                cursor: page.last.or(resumed.cursor),
                complete: page.end,
            },
            unrestored: walk.unrestored,
        }))
    }

    /// A relationship of `object` (its policy, resource and ID) as `actor`
    /// made it, its DIDs in the spelling the provider keeps them under.
    pub(super) fn relationship_fact(
        &self,
        actor: &Did,
        subject: OwedSubject,
        object: (&str, &str, &str),
        relation: &str,
        held: bool,
    ) -> RecoveryFact {
        let subject = match subject {
            OwedSubject::Actor { did } => OwedSubject::Actor {
                did: self.provider.canonical_actor(&did),
            },
            other => other,
        };
        RecoveryFact::relationship(
            object.0,
            object.1,
            object.2,
            self.provider.canonical_actor(actor),
            relation,
            subject,
            held,
        )
    }

    /// The fact a settled outbox change leaves.
    pub(super) fn settled_fact(&self, change: &OwedChange) -> RecoveryFact {
        let tuple = &change.tuple;
        self.relationship_fact(
            &change.actor,
            tuple.subject.clone(),
            (&tuple.policy_id, &tuple.resource, &tuple.object),
            &tuple.relation,
            change.op == OwedOp::Add,
        )
    }

    /// Record, in a transaction of its own, that `fact` is about to be asked
    /// of the provider outside any transaction.
    pub(super) async fn recovery_prepare(&self, fact: &RecoveryFact) -> Result<()> {
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        RecoveryLedger::new(view.clone()).prepare(fact).await?;
        drop(view);
        txn.commit()
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }

    /// Record the provider's answer to a `fact` prepared before it was
    /// asked. The provider has answered whatever this write comes to, so a
    /// failure is logged and the answer stands: the intent it leaves is read
    /// by a pass as unanswered.
    pub(super) async fn recovery_answered(&self, fact: &RecoveryFact, applied: bool) {
        if let Err(error) = self.recovery_settle(fact, applied).await {
            tracing::warn!(
                target: "acp::audit",
                %error,
                policy_id = %fact.policy_id,
                resource = %fact.resource_name,
                object = %fact.object_id,
                applied,
                "Could not record what the ACP provider answered for a recovery pass"
            );
        }
    }

    async fn recovery_settle(&self, fact: &RecoveryFact, applied: bool) -> Result<()> {
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        RecoveryLedger::new(view.clone())
            .settle(fact, applied)
            .await?;
        drop(view);
        txn.commit()
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }

    /// Ask the provider for `fact` outside a transaction: its intent first,
    /// then `ask`, then what the provider answered. A provider that could
    /// not be reached may still have taken the request, so its intent stays.
    pub(super) async fn recorded<T, F>(&self, fact: RecoveryFact, ask: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.recovery_prepare(&fact).await?;
        let answer = ask.await;
        match &answer {
            Ok(_) => self.recovery_answered(&fact, true).await,
            Err(Error::ProviderUnavailable(_)) => {}
            Err(_) => self.recovery_answered(&fact, false).await,
        }
        answer
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
