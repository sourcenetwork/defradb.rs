//! App-supplied validation of replicated composites in collections the app
//! has claimed, and re-drive of the composites it defers.

mod awaited;
mod deferred;
mod judge;
mod local_commit;
mod redrive;
mod redriven;
mod signature;
mod state;
mod sweep;
mod validator;
mod verdict;
mod view;
mod view_index;

pub use awaited::Awaited;
pub(crate) use awaited::{is_immutable_scalar_field, WaitKey};
pub use deferred::{
    MAX_AWAITED_PER_COMPOSITE, MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY, REDRIVE_BUDGET,
};
pub use redriven::{RedrivenMerge, RedrivenMergeSink};
pub use signature::SignatureStatus;
pub(crate) use state::GovernanceState;
#[cfg(not(target_arch = "wasm32"))]
pub use sweep::start_governance_sweep;
pub use sweep::{run_governance_sweep, SWEEP_BUDGET, SWEEP_INTERVAL};
pub use validator::{MergeCandidate, MergeGovernance, MergeValidator};
pub use verdict::MergeVerdict;
pub use view::{FieldValue, MergeView};
