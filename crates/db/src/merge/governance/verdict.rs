use defra_core::merge::MergeOutcome;

use super::awaited::Awaited;

/// A merge validator's judgement of one composite.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeVerdict {
    /// The composite may merge.
    Accept,
    /// The composite is invalid on its own content and will never be accepted
    /// on replay. The host quarantines it locally.
    Reject { reason: String },
    /// The verdict depends on content this node does not hold yet. The host
    /// leaves the composite unmerged and re-drives it through the normal merge
    /// path as soon as something in `awaiting` merges:
    ///
    /// - [`Awaited::Composite`]: that composite; a document is named by its
    ///   genesis composite's CID.
    /// - [`Awaited::ImmutableField`]: any composite merging into the collection
    ///   with that `@immutable` scalar LWW field value, for an input that does
    ///   not exist yet and so has no CID. Any other field is an error.
    ///
    /// Nothing else triggers re-drive: a composite awaiting a signature or
    /// field block's CID is retried by the replication retry clock instead.
    /// At most [`super::MAX_AWAITED_PER_COMPOSITE`] entries are indexed.
    Defer {
        reason: String,
        awaiting: Vec<Awaited>,
    },
}

impl MergeVerdict {
    pub fn reject(reason: impl Into<String>) -> Self {
        Self::Reject {
            reason: reason.into(),
        }
    }

    pub fn defer(
        reason: impl Into<String>,
        awaiting: impl IntoIterator<Item = impl Into<Awaited>>,
    ) -> Self {
        Self::Defer {
            reason: reason.into(),
            awaiting: awaiting.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn into_outcome(self) -> (Option<MergeOutcome>, Vec<Awaited>) {
        match self {
            Self::Accept => (None, Vec::new()),
            Self::Reject { reason } => (Some(MergeOutcome::rejected(reason)), Vec::new()),
            Self::Defer { reason, awaiting } => {
                (Some(MergeOutcome::retryable_skip(reason)), awaiting)
            }
        }
    }
}
