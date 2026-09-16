//! App-supplied validation of replicated composites in collections the app
//! has claimed, and re-drive of the composites it defers.

mod deferred;
mod judge;
mod signature;
mod validator;
mod verdict;
mod view;

pub(crate) use deferred::DeferredMerges;
pub use deferred::{MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY, REDRIVE_BUDGET};
pub(crate) use judge::Judgement;
pub use signature::SignatureStatus;
pub use validator::{MergeCandidate, MergeGovernance, MergeValidator};
pub use verdict::MergeVerdict;
pub use view::{FieldValue, MergeView};
