//! What the node still owes a remote document ACP provider: reporting it,
//! waiting for it, handing in a fresh credential for it, and abandoning a
//! refused change.
//!
//! A write on a remote provider commits before the provider holds the
//! registration and the relationship changes it owes (see `acp::pending` and
//! `acp::outbox`). The commit stands whatever their submission comes to, so
//! these calls are how a caller learns what it came to and acts on it.
//!
//! Who may call. Every call takes the context of an authenticated caller.
//! An actor's entries are read by that actor and by the node's own
//! principal ([`crate::Node::node_context`]), and by no one else; to anyone else
//! an entry does not exist. A fresh credential is taken from the entry's
//! actor or the node's principal, and counts only if it is that actor's own:
//! it is checked as the credential of the write was. Only the actor that
//! staged a change may abandon it.

use std::time::Duration;

use defra_core::{RequestContext, TxnId};
use identity::Did;

use super::NodeCore;
use crate::{Error, Result, SignerCredential};

/// Whose entries a caller may see.
enum Reader {
    /// The node's own principal: everything.
    Node,
    /// One actor: its own.
    Actor(Did),
}

impl Reader {
    fn reads(&self, acp: &dyn acp::DocumentACP, actor: &Did) -> bool {
        match self {
            Self::Node => true,
            Self::Actor(caller) => acp.canonical_actor(caller) == acp.canonical_actor(actor),
        }
    }
}

fn denied(reason: &str) -> Error {
    query::QueryError::permission_denied(reason).into()
}

impl NodeCore<'_> {
    /// The caller `context` authenticates, as a reader of owed entries.
    fn owed_reader(&self, context: &RequestContext) -> Result<Reader> {
        if !context.authorization_valid_now() {
            return Err(denied("request authorization is invalid"));
        }
        let identity = context
            .identity()
            .ok_or_else(|| denied("an authenticated caller is required"))?;
        if self.node.node_principal().as_deref() == Some(identity) {
            return Ok(Reader::Node);
        }
        let caller = Did::new(identity).map_err(Error::InvalidIdentityDid)?;
        if caller.is_wildcard() {
            return Err(denied("an explicit caller is required"));
        }
        Ok(Reader::Actor(caller))
    }

    /// What `actor` owes the document ACP provider: the registrations of
    /// documents it created and the relationship changes it staged that the
    /// provider does not hold yet. Each entry says where it stands (staged,
    /// waiting for authority, refused), why its last submission failed, and
    /// what it waits behind. A submission that failed after its commit is
    /// found here: the commit itself returned success.
    ///
    /// Only `actor` itself and the node's principal may ask; anyone else is
    /// refused. Nothing is owed on a provider that joins the transaction.
    pub(crate) async fn owed_acp(
        &self,
        context: &RequestContext,
        actor: &str,
    ) -> Result<Vec<acp::OwedEntry>> {
        let reader = self.owed_reader(context)?;
        let actor = Did::new(actor).map_err(Error::InvalidIdentityDid)?;
        let acp = self.node.ops().document_acp;
        if !reader.reads(acp.as_ref(), &actor) {
            return Err(denied(
                "only an actor and the node read what the actor owes",
            ));
        }
        self.node
            .on_acp(
                move || async move { Ok(acp.owed_entries(acp::OwedFilter::Actor(&actor)).await?) },
            )
            .await
    }

    /// Wait, for at most `deadline`, until what the committed transaction
    /// `txn` owes the provider is settled or has come to rest, and say
    /// which: [`acp::OwedWait::Settled`] when the provider holds all of it,
    /// [`acp::OwedWait::Parked`] when what is left waits for a credential or
    /// was refused, [`acp::OwedWait::Pending`] when the deadline passed or
    /// the provider could not be reached with entries still to send.
    ///
    /// What can still be sent is submitted once first; after that the call
    /// sleeps until a record changes. The node's principal sees every entry
    /// of the transaction, an actor its own, and the answer is about the
    /// entries the caller sees.
    pub(crate) async fn await_owed_acp(
        &self,
        context: &RequestContext,
        txn: TxnId,
        deadline: Duration,
    ) -> Result<acp::OwedWait> {
        let reader = self.owed_reader(context)?;
        let acp = self.node.ops().document_acp;
        self.node
            .on_acp(move || async move {
                // An actor's wait is for its own entries: another's neither
                // show in it nor keep it.
                let only = match &reader {
                    Reader::Node => None,
                    Reader::Actor(actor) => Some(actor),
                };
                let wait = match workers::timeout(deadline, acp.await_owed(txn.route, only)).await {
                    Ok(wait) => wait?,
                    // The wait is dropped with whatever it was submitting still
                    // in flight, as a commit that stops waiting leaves it.
                    Err(workers::Error::TimedOut) => {
                        let mut left = acp
                            .owed_entries(acp::OwedFilter::Transaction(txn.route))
                            .await?;
                        left.retain(|entry| reader.reads(acp.as_ref(), &entry.actor));
                        match left.is_empty() {
                            true => acp::OwedWait::Settled,
                            false if left.iter().any(|entry| entry.sendable) => {
                                acp::OwedWait::Pending {
                                    entries: left,
                                    provider_unavailable: false,
                                }
                            }
                            false => acp::OwedWait::Parked(left),
                        }
                    }
                    Err(error) => {
                        return Err(query::QueryError::execution(format!(
                            "could not wait for owed ACP changes: {error}"
                        ))
                        .into())
                    }
                };
                Ok(wait)
            })
            .await
    }

    /// Hand the node a fresh credential for one owed registration or one
    /// owed relationship change, and submit it.
    ///
    /// `credential` is the entry's actor's own, as a verified write brings
    /// one: a [`SignerCredential::Delegation`] to this node, or a
    /// [`SignerCredential::RequestBound`] one that names exactly the owed
    /// command ([`SignerCredential::None`] counts when the node holds a key
    /// for the actor). It is checked against that command as the credential
    /// of a write is checked when the write is admitted, and a credential
    /// that would not have admitted it fails the call with
    /// `acp::Error::OwedAuthorityUnavailable`, or with
    /// `acp::Error::DelegationOutsideWindow` when it does not hold at the
    /// provider's time, and changes nothing.
    ///
    /// Admitted, it is recorded with the entry and what is owed on the
    /// entry's document is submitted once, in order. A registration that
    /// settles lets the changes staged behind it go, each with the authority
    /// recorded with it; one whose own authority has run out is then parked
    /// and reported, and is given its own credential by another call. The
    /// answer is [`acp::OwedOutcome::Settled`] once the provider holds the
    /// entry, [`acp::OwedOutcome::Owed`] with the entry as it now stands,
    /// or [`acp::OwedOutcome::Discarded`] when its record was deleted
    /// without the provider holding it, with the reason.
    ///
    /// Every fresh credential is checked as a verified write's is: a
    /// provider that gives the node nothing to verify a delegation against
    /// takes none here, and an entry there is sent again only with a key
    /// the node holds or by a request that carries the actor's bearer.
    ///
    /// Only the entry's actor and the node's principal may call; to anyone
    /// else the entry is `acp::Error::OwedEntryNotFound`.
    pub(crate) async fn reauthorize_owed_acp(
        &self,
        context: &RequestContext,
        entry: &acp::OwedRef,
        credential: SignerCredential,
    ) -> Result<acp::OwedOutcome> {
        let reader = self.owed_reader(context)?;
        let caller = context.identity().unwrap_or_default().to_owned();
        let acp = self.node.ops().document_acp;
        let entry = entry.clone();
        self.node
            .on_acp(move || async move {
                let unknown = || acp::Error::OwedEntryNotFound {
                    entry: entry.to_string(),
                    actor: caller.clone(),
                };
                let owed = acp
                    .owed_entries(acp::OwedFilter::Entry(&entry))
                    .await?
                    .pop()
                    .filter(|owed| reader.reads(acp.as_ref(), &owed.actor))
                    .ok_or_else(unknown)?;
                // The credential is carried for the entry's actor and no one
                // else, under the spelling the entry keeps it in. It is to be
                // recorded and used when no request is there to vouch for it,
                // so it counts only where the provider gives the node something
                // to check it against: the requirement a verified write makes.
                let carried = credential.carried_by(
                    RequestContext::default().with_transactional_authority_required(),
                    owed.actor.as_str(),
                );
                Ok(acp::bound_to_request(&acp, &carried)
                    .reauthorize_owed(&owed.actor, &entry)
                    .await?)
            })
            .await
    }

    /// Give up the relationship change at place `seq` of the node's queue,
    /// which the caller staged and the provider refused. Its record is
    /// deleted without the provider holding the change, and the later
    /// changes of its relationship, which waited behind it, are submitted.
    ///
    /// An abandoned delete stops denying its subject on this node: the
    /// provider, which refused the delete and so still holds the
    /// relationship, decides again. That is why only the actor that staged
    /// the change may abandon it, and not the subject the delete names. The
    /// node's principal abandons the changes it staged itself and no
    /// others; to anyone else the change is `acp::Error::OwedEntryNotFound`.
    /// When a transaction that committed meanwhile changed what is owed on
    /// the relationship the call fails with `acp::Error::OwedEntryConflict`
    /// and may be made again. A change that is staged or waits for
    /// authority is `acp::Error::OwedChangeNotRefused`: a request sent for it
    /// earlier may still land, so it is given a fresh credential instead
    /// ([`Self::reauthorize_owed_acp`]).
    pub(crate) async fn abandon_owed_acp_change(
        &self,
        context: &RequestContext,
        seq: u64,
    ) -> Result<()> {
        let actor = match self.owed_reader(context)? {
            Reader::Actor(actor) => actor,
            // The node abandons what it staged itself, like any actor.
            Reader::Node => {
                let own = self.node.node_principal().unwrap_or_default();
                Did::new(own).map_err(Error::InvalidIdentityDid)?
            }
        };
        let acp = self.node.ops().document_acp;
        self.node
            .on_acp(move || async move { Ok(acp.abandon_owed_change(&actor, seq).await?) })
            .await
    }
}
