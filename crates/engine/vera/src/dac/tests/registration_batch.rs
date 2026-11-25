//! Several registrations of one owner go to the provider as lists.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use kovan_queue::seg_queue::SegQueue;

use super::*;
use crate::provider::{ProviderPolicyInfo, MAX_REGISTRATIONS_PER_REQUEST};

const POLICY: &str = "0707070707070707070707070707070707070707070707070707070707070707";

/// A provider that applies a list all or none and keeps what it was sent.
#[derive(Default)]
struct Lists {
    /// Each list sent, its objects in order.
    lists: SegQueue<Vec<String>>,
    /// Each registration sent alone.
    singles: SegQueue<String>,
    /// Registered objects.
    held: AtomicUsize,
    /// An object the provider refuses, alone or in a list.
    refused: Option<String>,
    /// The provider cannot be reached.
    away: AtomicBool,
    /// The provider takes no list.
    no_lists: bool,
}

impl Lists {
    fn refusing(object: &str) -> Self {
        Self {
            refused: Some(object.to_owned()),
            ..Self::default()
        }
    }

    fn lists(&self) -> Vec<Vec<String>> {
        std::iter::from_fn(|| self.lists.pop()).collect()
    }

    fn singles(&self) -> Vec<String> {
        std::iter::from_fn(|| self.singles.pop()).collect()
    }

    fn reach(&self) -> std::result::Result<(), ProviderError> {
        if self.away.load(Ordering::SeqCst) {
            return Err(ProviderError::Unavailable("away".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl VeraProvider for Lists {
    fn authorized_account(&self) -> String {
        "0x0".into()
    }

    async fn create_bearer_token(&self, did: &str) -> std::result::Result<String, ProviderError> {
        Ok(format!("token.of.{did}"))
    }

    fn self_did(&self) -> Option<String> {
        None
    }

    async fn create_policy(
        &self,
        _: &str,
        _: Option<acp::PolicyNonce>,
    ) -> std::result::Result<String, ProviderError> {
        unreachable!("no policy is created here")
    }

    async fn register_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        self.reach()?;
        self.singles.push(object_id.to_owned());
        if self.refused.as_deref() == Some(object_id) {
            return Err(ProviderError::Transaction(format!("{object_id} refused")));
        }
        self.held.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn register_objects(
        &self,
        _: &str,
        objects: &[acp::AcpObject<'_>],
    ) -> std::result::Result<(), ProviderError> {
        if self.no_lists {
            return Err(ProviderError::Unsupported("no lists".into()));
        }
        self.reach()?;
        assert!(objects.len() <= MAX_REGISTRATIONS_PER_REQUEST);
        self.lists
            .push(objects.iter().map(|o| o.object_id.to_owned()).collect());
        if objects
            .iter()
            .any(|object| self.refused.as_deref() == Some(object.object_id))
        {
            return Err(ProviderError::Transaction("list refused".into()));
        }
        self.held.fetch_add(objects.len(), Ordering::SeqCst);
        Ok(())
    }

    async fn archive_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<(), ProviderError> {
        unreachable!("nothing is archived here")
    }

    async fn set_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!("no relationship is written here")
    }

    async fn delete_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!("no relationship is written here")
    }

    async fn query_policy(
        &self,
        _: &str,
    ) -> std::result::Result<Option<ProviderPolicyInfo>, ProviderError> {
        unreachable!("no policy is read here")
    }

    async fn query_object_owner(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<(bool, String), ProviderError> {
        unreachable!("no owner is read here")
    }

    async fn verify_access(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<bool, ProviderError> {
        unreachable!("no access is checked here")
    }

    async fn create_access_decision(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> std::result::Result<Option<String>, ProviderError> {
        unreachable!("no decision is made here")
    }
}

fn ids(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("bae-{n}")).collect()
}

fn objects(ids: &[String]) -> Vec<acp::AcpObject<'_>> {
    ids.iter()
        .map(|id| acp::AcpObject {
            policy_id: POLICY,
            resource: "users",
            object_id: id,
        })
        .collect()
}

fn owner() -> Did {
    Did::new("did:key:z6MkalicE").unwrap()
}

fn acp_of(provider: &Arc<Lists>) -> VeraDocumentACP {
    VeraDocumentACP::without_access_cache(provider.clone())
}

#[nuclei::test]
async fn several_registrations_of_one_owner_are_one_list_in_order() {
    let provider = Arc::new(Lists::default());
    let docs = ids(3);
    acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap();
    assert_eq!(provider.lists(), vec![docs]);
    assert!(provider.singles().is_empty());
}

#[nuclei::test]
async fn one_registration_is_sent_alone_as_before() {
    let provider = Arc::new(Lists::default());
    let docs = ids(1);
    acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap();
    assert!(provider.lists().is_empty());
    assert_eq!(provider.singles(), docs);
}

#[nuclei::test]
async fn more_registrations_than_one_list_holds_go_in_several() {
    let provider = Arc::new(Lists::default());
    let docs = ids(MAX_REGISTRATIONS_PER_REQUEST + 6);
    acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap();
    let lists = provider.lists();
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[0], docs[..MAX_REGISTRATIONS_PER_REQUEST]);
    assert_eq!(lists[1], docs[MAX_REGISTRATIONS_PER_REQUEST..]);
    assert_eq!(provider.held.load(Ordering::SeqCst), docs.len());
}

#[nuclei::test]
async fn a_refused_list_is_sent_again_one_at_a_time_up_to_the_refused_one() {
    let docs = ids(5);
    let provider = Arc::new(Lists::refusing(&docs[2]));
    let failure = acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap_err();
    assert_eq!(failure.index, 2);
    assert!(
        failure.error.to_string().contains("bae-2 refused"),
        "the refused one's own answer: {}",
        failure.error
    );
    assert_eq!(provider.lists(), vec![docs.clone()]);
    assert_eq!(provider.singles(), docs[..3]);
    assert_eq!(provider.held.load(Ordering::SeqCst), 2);
}

#[nuclei::test]
async fn a_provider_found_away_is_not_asked_one_at_a_time() {
    let provider = Arc::new(Lists::default());
    provider.away.store(true, Ordering::SeqCst);
    let docs = ids(4);
    let failure = acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap_err();
    assert_eq!(failure.index, 0);
    assert!(matches!(failure.error, acp::Error::ProviderUnavailable(_)));
    assert!(provider.singles().is_empty());
}

#[nuclei::test]
async fn a_provider_that_takes_no_list_is_sent_each_alone() {
    let provider = Arc::new(Lists {
        no_lists: true,
        ..Lists::default()
    });
    let docs = ids(3);
    acp_of(&provider)
        .register_objects(&owner(), &objects(&docs))
        .await
        .unwrap();
    assert!(provider.lists().is_empty());
    assert_eq!(provider.singles(), docs);
}
