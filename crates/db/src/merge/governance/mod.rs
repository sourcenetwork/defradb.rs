//! App-supplied validation of replicated composites in collections the app
//! has claimed, and re-drive of the composites it defers.

mod awaited;
mod deferred;
mod judge;
mod local_commit;
mod local_write;
mod redriven;
mod signature;
mod sweep;
mod validator;
mod verdict;
mod view;
mod view_index;

pub use awaited::Awaited;
pub(crate) use awaited::{is_immutable_scalar_field, WaitKey};
pub(crate) use deferred::DeferredMerges;
pub use deferred::{
    MAX_AWAITED_PER_COMPOSITE, MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY, REDRIVE_BUDGET,
};
pub(crate) use judge::{GovernedFrame, Judgement};
pub use local_commit::LocalCommitRelease;
pub(crate) use local_write::judge_local_write;
pub use local_write::LocalWriteJudge;
pub use redriven::{RedrivenMerge, RedrivenMergeSink};
pub use signature::SignatureStatus;
pub use sweep::{run_governance_sweep, SWEEP_BUDGET, SWEEP_INTERVAL};
pub use validator::{DefinitionCandidate, MergeCandidate, MergeGovernance, MergeValidator};
pub use verdict::MergeVerdict;
pub use view::{FieldValue, MergeView};
