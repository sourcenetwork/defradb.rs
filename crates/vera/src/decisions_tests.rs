//! The decision service against a counting provider with injected latency:
//! concurrent checks of one tuple make one call, a page's checks run at
//! once, and only a decision keyed by a verified state is cached.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kovan::Atom;

use super::*;
use crate::provider::{ProviderPolicyInfo, SubjectRef};

const LATENCY: Duration = Duration::from_millis(100);

/// Answers every check with an allow after `LATENCY`, counting the calls; its
/// verified state is whatever the test sets.
struct CountingProvider {
    calls: AtomicUsize,
    state: Atom<Option<String>>,
}

impl CountingProvider {
    fn new(state: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            state: Atom::new(state.map(str::to_string)),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl VeraProvider for CountingProvider {
    fn authorized_account(&self) -> String {
        String::new()
    }
    async fn create_bearer_token(&self, _: &str) -> Result<String, ProviderError> {
        unreachable!("decisions only")
    }
    fn self_did(&self) -> Option<String> {
        None
    }
    async fn create_policy(&self, _: &str) -> Result<String, ProviderError> {
        unreachable!("decisions only")
    }
    async fn register_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(), ProviderError> {
        unreachable!("decisions only")
    }
    async fn archive_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(), ProviderError> {
        unreachable!("decisions only")
    }
    async fn set_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        unreachable!("decisions only")
    }
    async fn delete_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        unreachable!("decisions only")
    }
    async fn query_policy(&self, _: &str) -> Result<Option<ProviderPolicyInfo>, ProviderError> {
        unreachable!("decisions only")
    }
    async fn query_object_owner(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(bool, String), ProviderError> {
        unreachable!("decisions only")
    }
    async fn verify_access(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        nuclei::time::sleep(LATENCY).await;
        Ok(true)
    }
    fn decision_state(&self) -> Option<String> {
        self.state.load_clone()
    }
}

/// Round trips as tasks of the test's nuclei runtime, whose time it pauses.
struct TestNetwork;

impl inputs::Network for TestNetwork {
    fn spawn(&self, work: defra_core::thread_bounds::MaybeBoxFuture<'static, ()>) {
        nuclei::spawn(work).detach();
    }

    fn sleep(&self, duration: Duration) -> defra_core::thread_bounds::MaybeBoxFuture<'static, ()> {
        Box::pin(nuclei::time::sleep(duration))
    }
}

fn test_network() -> inputs::Runtime {
    inputs::Runtime::new(Arc::new(TestNetwork), Arc::new(inputs::RuntimeClock::new()))
}

fn service(provider: &Arc<CountingProvider>) -> inputs::Service<RemoteDecisions> {
    inputs::Service::new(
        RemoteDecisions {
            provider: Arc::clone(provider) as Arc<dyn VeraProvider>,
        },
        config(Duration::from_secs(5)),
        test_network(),
    )
}

fn key(object: &str, state: Option<String>) -> DecisionKey {
    DecisionKey {
        actor: "did:key:reader".into(),
        policy: "policy".into(),
        resource: "files".into(),
        object: object.into(),
        permission: "read".into(),
        state,
    }
}

#[nuclei::test(start_paused = true)]
async fn concurrent_checks_of_one_tuple_make_one_call() {
    let provider = CountingProvider::new(None);
    let decisions = service(&provider);
    let answers = decisions
        .get_many((0..8).map(|_| key("doc", None)).collect())
        .await;
    assert!(answers.into_iter().all(|answer| matches!(answer, Ok(true))));
    assert_eq!(provider.calls(), 1);
}

#[nuclei::test(start_paused = true)]
async fn a_page_of_checks_runs_at_once() {
    let provider = CountingProvider::new(None);
    let decisions = service(&provider);
    let started = nuclei::time::Instant::now();
    let answers = decisions
        .get_many(
            (0..16)
                .map(|index| key(&format!("doc-{index}"), None))
                .collect(),
        )
        .await;
    assert!(answers.into_iter().all(|answer| matches!(answer, Ok(true))));
    assert_eq!(provider.calls(), 16);
    assert_eq!(
        started.elapsed(),
        LATENCY,
        "one round trip of time, not sixteen"
    );
}

#[nuclei::test(start_paused = true)]
async fn only_a_decision_at_a_verified_state_is_cached_and_a_new_state_asks_again() {
    let provider = CountingProvider::new(None);
    let decisions = service(&provider);
    decisions.get(key("doc", None)).await.unwrap();
    decisions.get(key("doc", None)).await.unwrap();
    assert_eq!(
        provider.calls(),
        2,
        "without a verified state nothing is cached"
    );

    let root = Some("0x01".to_string());
    decisions.get(key("doc", root.clone())).await.unwrap();
    decisions.get(key("doc", root)).await.unwrap();
    assert_eq!(provider.calls(), 3, "the same state answers from the cache");

    decisions
        .get(key("doc", Some("0x02".into())))
        .await
        .unwrap();
    assert_eq!(provider.calls(), 4, "a new state is a new key");
}
