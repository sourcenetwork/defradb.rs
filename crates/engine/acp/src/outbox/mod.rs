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
//!
//! A provider that journals its requests ([`OwedJournal`]) is sent changes
//! with an in-flight mark ([`InFlight`]) stored before each request goes,
//! so a change that landed before a crash is not sent again.

mod changes;
pub(crate) mod deny;
mod journal;
mod keys;
mod ops;
pub(crate) mod record;
mod report;
mod staging;
mod submission;

pub(crate) use ops::tuple_rest;
pub use ops::{tuple_head, tuple_unsettled_in};

pub use changes::{
    OwedChanges, StagedCount, MAX_OWED_CHANGES_PER_TRANSACTION, MAX_UNSETTLED_OWED_CHANGES,
    MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR, VALIDATED_WITHIN_OF_NODE_LIMIT,
};
pub use deny::{OwedDenial, OwedDenials};
pub use journal::{OwedJournal, PreparedRequest, RequestOutcome};
pub use keys::OwedTupleState;
pub(crate) use ops::legacy_changes;
pub use ops::MAX_DISCARD_NOTES;
pub use record::{
    run_epoch, InFlight, NewOwedChange, OwedAuthority, OwedChange, OwedOp, OwedState, OwedSubject,
    OwedTuple, MAX_OWED_CHANGE_BYTES, MAX_REQUEST_ID_BYTES, RESERVED_REASON_BYTES,
};
pub use report::{
    OwedAuthorityKind, OwedEntry, OwedFilter, OwedHold, OwedOutcome, OwedRef, OwedWait, OwedWhat,
};
pub use staging::{
    prepare_relationship, PreparedRelationship, StageArchive, StageRelationship, Staged,
    TransactionOutbox,
};
pub use submission::{submit_relationship, Submission};
