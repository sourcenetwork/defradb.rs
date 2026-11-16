//! Relationship changes owed to an external ACP provider.
//!
//! An external provider (Vera) cannot join a transaction, so a relationship
//! change made in one is recorded there as owed, beside the registrations
//! creates owe (see [`crate::pending`]). The record commits and rolls back
//! with the transaction's rows, keeps its place in a queue until the provider
//! holds the change, and survives a crash or a provider failure.
//!
//! Until a change is settled, access is no wider than its transaction meant:
//! an owed add grants nothing, and an owed delete denies its subject on this
//! node from its commit until it is settled, whatever is staged after it
//! (see [`crate::ExternalDocumentAcp`]).
//!
//! A transaction stages through a [`TransactionOutbox`]
//! ([`crate::DocumentACP::transaction_outbox`]). After the commit the
//! provider is sent the queue in sequence order
//! ([`crate::ExternalDocumentAcp::settle_owed_changes_once`]), and each
//! change is settled, parked or left by what the provider answers
//! ([`Submission`]).
//!
//! Sequences order the queue. Changes of one tuple are ordered as their
//! transactions commit; across tuples the order is the one they were staged
//! in.

mod changes;
pub(crate) mod deny;
mod keys;
mod ops;
mod record;
mod staging;
mod submission;

pub use changes::{
    OwedChanges, StagedCount, MAX_OWED_CHANGES_PER_TRANSACTION, MAX_UNSETTLED_OWED_CHANGES,
    MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR,
};
pub use deny::{OwedDenial, OwedDenials};
pub use keys::OwedTupleState;
pub use record::{
    NewOwedChange, OwedAuthority, OwedChange, OwedOp, OwedState, OwedSubject, OwedTuple,
    MAX_OWED_CHANGE_BYTES,
};
pub use staging::{StageRelationship, Staged, TransactionOutbox};
pub use submission::{submit_relationship, Submission};
