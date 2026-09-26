use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use kovan::AtomOption;
use kovan_queue::seg_queue::SegQueue;

use super::*;

/// Which relationship-emitting provider method a routing test drove, plus
/// the structured-subject codec tuple for the EntitySet path. Each field
/// stands alone: tests only ever read one after its own call completes, so
/// there is no compound state that needs a single shared guard.
struct MockProvider {
    decisions: SegQueue<bool>,
    created_decision: AtomOption<String>,
    verify_calls: AtomicUsize,
    set_relationship_called: AtomicBool,
    delete_relationship_called: AtomicBool,
    set_subject_call: AtomOption<(u8, String, String, String)>,
    delete_subject_call: AtomOption<(u8, String, String, String)>,
}

impl MockProvider {
    fn new(decisions: Vec<bool>) -> Self {
        let queue = SegQueue::new();
        for decision in decisions {
            queue.push(decision);
        }
        Self {
            decisions: queue,
            created_decision: AtomOption::none(),
            verify_calls: AtomicUsize::new(0),
            set_relationship_called: AtomicBool::new(false),
            delete_relationship_called: AtomicBool::new(false),
            set_subject_call: AtomOption::none(),
            delete_subject_call: AtomOption::none(),
        }
    }

    fn verify_calls(&self) -> usize {
        self.verify_calls.load(Ordering::Relaxed)
    }

    fn created_decision(&self) -> Option<String> {
        self.created_decision.load().map(|g| (*g).clone())
    }
}

#[async_trait]
impl VeraProvider for MockProvider {
    fn authorized_account(&self) -> String {
        "0x0".to_string()
    }

    async fn create_bearer_token(&self, _did: &str) -> std::result::Result<String, ProviderError> {
        Ok("mock.bearer.token".to_string())
    }

    fn self_did(&self) -> Option<String> {
        None
    }

    async fn create_policy(
        &self,
        _policy_yaml: &str,
    ) -> std::result::Result<String, ProviderError> {
        unreachable!("create_policy is not used in this test")
    }

    async fn register_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        unreachable!("register_object is not used in this test")
    }

    async fn archive_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        unreachable!("archive_object is not used in this test")
    }

    async fn set_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        self.set_relationship_called.store(true, Ordering::Relaxed);
        Ok(true)
    }

    async fn delete_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        self.delete_relationship_called
            .store(true, Ordering::Relaxed);
        Ok(true)
    }

    async fn set_relationship_subject(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.set_subject_call.store_some((
            kind,
            subject_resource.to_string(),
            subject_object_id.to_string(),
            subject_relation.to_string(),
        ));
        Ok(true)
    }

    async fn delete_relationship_subject(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.delete_subject_call.store_some((
            kind,
            subject_resource.to_string(),
            subject_object_id.to_string(),
            subject_relation.to_string(),
        ));
        Ok(true)
    }

    async fn query_policy(
        &self,
        _policy_id: &str,
    ) -> std::result::Result<Option<crate::provider::ProviderPolicyInfo>, ProviderError> {
        unreachable!("query_policy is not used in this test")
    }

    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(bool, String), ProviderError> {
        Ok((
            true,
            "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        ))
    }

    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _permission: &str,
        _actor_did: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.verify_calls.fetch_add(1, Ordering::Relaxed);
        let decision = self.decisions.pop().expect("mock verify_access decision");
        Ok(decision)
    }

    async fn create_access_decision(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _permission: &str,
        actor_did: &str,
    ) -> std::result::Result<Option<String>, ProviderError> {
        let decision_id = format!("decision-for-{actor_did}");
        self.created_decision.store_some(decision_id.clone());
        Ok(Some(decision_id))
    }
}

#[tokio::test]
async fn check_doc_access_does_not_cache_remote_denials() {
    let provider = Arc::new(MockProvider::new(vec![false, true]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let did = Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
    let identity = Identity::from(did);

    let denied = acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            "policy-1",
            "users",
            "doc-1",
        )
        .await
        .expect("initial denial");
    assert!(!denied);

    let allowed = acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            "policy-1",
            "users",
            "doc-1",
        )
        .await
        .expect("fresh remote decision");
    assert!(allowed);
    assert_eq!(provider.verify_calls(), 2);
}

#[tokio::test]
async fn check_doc_access_does_not_cache_denials_when_enabled() {
    let provider = Arc::new(MockProvider::new(vec![false, true]));
    let acp = VeraDocumentACP::new(provider.clone(), Duration::from_secs(300));
    let did = Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
    let identity = Identity::from(did);

    let denied = acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            "policy-1",
            "users",
            "doc-1",
        )
        .await
        .expect("initial denial");
    assert!(!denied);

    let allowed = acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            "policy-1",
            "users",
            "doc-1",
        )
        .await
        .expect("fresh remote decision");
    assert!(allowed);
    assert_eq!(provider.verify_calls(), 2);
}

#[tokio::test]
async fn create_access_decision_delegates_to_provider() {
    let provider = Arc::new(MockProvider::new(vec![]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let did = Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
    let identity = Identity::from(did.clone());

    let decision_id = acp
        .create_access_decision(&identity, "policy-1", "transcript", "transcript", "writer")
        .await
        .expect("create access decision should succeed");

    assert_eq!(decision_id, Some(format!("decision-for-{}", did.as_str())));
    assert_eq!(provider.created_decision(), decision_id);
}

fn requestor() -> Did {
    Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap()
}

#[tokio::test]
async fn add_relationship_routes_actor_to_set_relationship() {
    let provider = Arc::new(MockProvider::new(vec![]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let target = Subject::Entity(zanzibar::Did::new_unchecked(
        "did:key:zActorTarget".to_string(),
    ));

    let result = acp
        .add_relationship(
            &requestor(),
            target,
            "policy-1",
            "users",
            "doc-1",
            "reader",
            &[],
        )
        .await
        .expect("actor relationship should route to set_relationship");
    assert!(result);

    assert!(
        provider.set_relationship_called.load(Ordering::Relaxed),
        "actor must hit set_relationship"
    );
    assert!(
        provider.set_subject_call.is_none(),
        "actor must not hit subject path"
    );
}

#[tokio::test]
async fn add_relationship_routes_object_edge_to_subject_kind_2() {
    let provider = Arc::new(MockProvider::new(vec![]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let target = Subject::entity_set("directory", "d1", "");

    let result = acp
        .add_relationship(
            &requestor(),
            target,
            "policy-1",
            "users",
            "doc-1",
            "reader",
            &[],
        )
        .await
        .expect("object edge should route to set_relationship_subject");
    assert!(result);

    assert!(
        !provider.set_relationship_called.load(Ordering::Relaxed),
        "object edge must not hit actor path"
    );
    assert_eq!(
        provider.set_subject_call.load().map(|g| (*g).clone()),
        Some((2, "directory".to_string(), "d1".to_string(), String::new()))
    );
}

#[tokio::test]
async fn add_relationship_routes_userset_to_subject_kind_3() {
    let provider = Arc::new(MockProvider::new(vec![]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let target = Subject::entity_set("directory", "d1", "member");

    acp.add_relationship(
        &requestor(),
        target,
        "policy-1",
        "users",
        "doc-1",
        "reader",
        &[],
    )
    .await
    .expect("userset should route to set_relationship_subject");

    assert_eq!(
        provider.set_subject_call.load().map(|g| (*g).clone()),
        Some((
            3,
            "directory".to_string(),
            "d1".to_string(),
            "member".to_string()
        ))
    );
}

#[tokio::test]
async fn delete_relationship_routes_object_edge_to_subject_kind_2() {
    let provider = Arc::new(MockProvider::new(vec![]));
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let target = Subject::entity_set("directory", "d1", "");

    acp.delete_relationship(
        &requestor(),
        target,
        "policy-1",
        "users",
        "doc-1",
        "reader",
        &[],
    )
    .await
    .expect("object edge delete should route to delete_relationship_subject");

    assert!(!provider.delete_relationship_called.load(Ordering::Relaxed));
    assert_eq!(
        provider.delete_subject_call.load().map(|g| (*g).clone()),
        Some((2, "directory".to_string(), "d1".to_string(), String::new()))
    );
}

/// A provider that omits the structured-subject overrides, so the trait
/// default (`Unsupported`) is exercised on the EntitySet path.
struct NoSubjectProvider;

#[async_trait]
impl VeraProvider for NoSubjectProvider {
    fn authorized_account(&self) -> String {
        "0x0".to_string()
    }
    async fn create_bearer_token(&self, _did: &str) -> std::result::Result<String, ProviderError> {
        Ok("mock.bearer.token".to_string())
    }
    fn self_did(&self) -> Option<String> {
        None
    }
    async fn create_policy(
        &self,
        _policy_yaml: &str,
    ) -> std::result::Result<String, ProviderError> {
        unreachable!()
    }
    async fn register_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        unreachable!()
    }
    async fn archive_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        unreachable!()
    }
    async fn set_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!()
    }
    async fn delete_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!()
    }
    async fn query_policy(
        &self,
        _policy_id: &str,
    ) -> std::result::Result<Option<crate::provider::ProviderPolicyInfo>, ProviderError> {
        unreachable!()
    }
    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(bool, String), ProviderError> {
        unreachable!()
    }
    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _permission: &str,
        _actor_did: &str,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!()
    }
}

#[tokio::test]
async fn add_relationship_subject_default_is_unsupported() {
    let provider = Arc::new(NoSubjectProvider);
    let acp = VeraDocumentACP::without_access_cache(provider);
    let target = Subject::entity_set("directory", "d1", "");

    let err = acp
        .add_relationship(
            &requestor(),
            target,
            "policy-1",
            "users",
            "doc-1",
            "reader",
            &[],
        )
        .await
        .expect_err("provider without subject support must fail");
    let message = err.to_string();
    assert!(
        message.contains("unsupported"),
        "expected Unsupported error, got: {message}"
    );
}
