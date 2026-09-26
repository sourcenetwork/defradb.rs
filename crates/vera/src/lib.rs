mod access_cache;
mod circuit_breaker;
pub mod cosmos;
mod policy_cache;
mod provider;
mod tuning;
pub mod vera_rs;

pub use cosmos::{CosmosProvider, VeraDocumentACP};
pub use provider::{
    AcpLightClientStatus, ProviderError, ProviderPolicyInfo, SubjectRef, VeraProvider,
};
pub use tuning::AcpTuning;
pub use vera_rs::VeraRsProvider;

#[cfg(test)]
pub(crate) fn signing_state_test_guard() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}
