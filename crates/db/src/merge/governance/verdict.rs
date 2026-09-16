use cid::Cid;
use defra_core::merge::MergeOutcome;

/// A merge validator's judgement of one composite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeVerdict {
    /// The composite may merge.
    Accept,
    /// The composite is invalid on its own content and will never be accepted
    /// on replay. The host quarantines it locally.
    Reject { reason: String },
    /// The verdict depends on content this node does not hold yet. The host
    /// leaves the composite unmerged and re-drives it through the normal merge
    /// path as soon as a composite named in `awaiting` merges; a document is
    /// named by its genesis composite's CID. Only composite CIDs trigger
    /// re-drive: a composite awaiting any other CID, such as a signature or
    /// field block, is retried by the replication retry clock instead.
    Defer { reason: String, awaiting: Vec<Cid> },
}

impl MergeVerdict {
    pub fn reject(reason: impl Into<String>) -> Self {
        Self::Reject {
            reason: reason.into(),
        }
    }

    pub fn defer(reason: impl Into<String>, awaiting: impl IntoIterator<Item = Cid>) -> Self {
        Self::Defer {
            reason: reason.into(),
            awaiting: awaiting.into_iter().collect(),
        }
    }

    pub(crate) fn into_outcome(self) -> (Option<MergeOutcome>, Vec<Cid>) {
        match self {
            Self::Accept => (None, Vec::new()),
            Self::Reject { reason } => (Some(MergeOutcome::rejected(reason)), Vec::new()),
            Self::Defer { reason, awaiting } => {
                (Some(MergeOutcome::retryable_skip(reason)), awaiting)
            }
        }
    }
}
