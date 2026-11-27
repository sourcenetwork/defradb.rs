//! A provider that keeps the request it sends in a journal until a verified
//! receipt lets it go.
//!
//! The drainer sends owed changes through it in steps, each stored before
//! the next (`AcpOutbox.tla`, the actions named in brackets):
//!
//! 1. [`OwedJournal::prepare`] builds one request for one or more changes,
//!    and the drainer stores the request's digest with each change (JMark);
//! 2. [`OwedJournal::journal`] has the worker keep the request (JPrepare),
//!    and the drainer stores its transaction hash with each change (JHash);
//! 3. [`OwedJournal::outcome`] sends it and waits for its receipt (JLand,
//!    JResolve).
//!
//! A change found marked after a crash or a dropped wait is decided by
//! [`OwedJournal::outcome`] too (JAdopt, JDecide): the request still in the
//! journal is resolved, else a verified receipt found by the stored hash or
//! the provider's note found by the digest decides it. Only when none of
//! them knows the request is the change sent again: a change takes effect
//! at most once for as long as one of them can still be read.

use async_trait::async_trait;
use storage::corekv::MaybeSendSync;

use super::record::{InFlight, OwedChange};
use super::submission::Submission;

/// One request built for owed changes, not yet journaled or sent.
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedRequest {
    /// The digest of `bytes`, as the provider names it.
    pub digest: String,
    /// The request, as the provider's worker journals it.
    pub bytes: Vec<u8>,
    /// How many of the changes asked for it carries: the first ones, in
    /// order. At least one.
    pub carries: usize,
    /// The provider's worker sequence when it was built, where the provider
    /// numbers its requests: see [`InFlight::floor`].
    pub floor: Option<u64>,
}

impl std::fmt::Debug for PreparedRequest {
    /// The request holds credentials: only its digest and size are printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRequest")
            .field("digest", &self.digest)
            .field("bytes", &self.bytes.len())
            .field("carries", &self.carries)
            .field("floor", &self.floor)
            .finish()
    }
}

/// What became of a journaled request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestOutcome {
    /// It executed and every change it carries took effect.
    Applied,
    /// It executed and was rejected: none of its changes took effect.
    Rejected { reason: String },
    /// Neither the journal, a receipt nor the note knows it: as far as this
    /// node can tell it never executed.
    Unknown,
    /// The provider could not be reached or did not answer in time. The
    /// request stays where it was.
    Unavailable { reason: String },
}

/// A provider whose worker journals each request until a verified receipt
/// lets it go, and keeps a note of the outcome of a request resolved by
/// anyone but the caller that journaled it.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait OwedJournal: MaybeSendSync {
    /// Build one request for `changes`, in order, applied together or not at
    /// all. It carries as many of the first changes as fit one request and
    /// can be signed now; the answer that ends the first one's submission
    /// before anything is sent is returned instead when it cannot go.
    async fn prepare(&self, changes: &[OwedChange]) -> Result<PreparedRequest, Submission>;

    /// Have the worker keep `request` in its journal, after resolving one
    /// it already keeps, and return its transaction hash. A request the
    /// journal already holds is kept as it is.
    async fn journal(&self, request: &PreparedRequest) -> Result<String, Submission>;

    /// What became of the request `flight` names: the journaled one is sent
    /// and waited for, one out of the journal is looked up by its hash, and
    /// failing that in the note by its digest, among the requests numbered
    /// from its floor on.
    async fn outcome(&self, flight: &InFlight) -> RequestOutcome;

    /// What a rejection of a request that carried `change` alone says of
    /// it: the credential it was sent with no longer holds
    /// ([`Submission::AwaitingAuthority`]), or the provider refused it
    /// ([`Submission::Refused`]).
    fn refusal(&self, change: &OwedChange, reason: String) -> Submission;
}
