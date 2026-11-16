//! What authority an owed relationship change records, and how the answer
//! to its submission is read.

use acp::{OwedAuthority, OwedChange, OwedOp, OwedState, OwedSubject, OwedTuple, Submission};
use defra_core::RequestContext;

use super::*;

const HELD: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const REMOTE: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

/// How the provider answers the next relationship write.
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum Answer {
    Applied,
    Rejected,
    Unreachable,
    Unclear,
}

/// A provider that holds [`HELD`]'s key, takes `delegation:<did>` as that
/// DID's delegation until it is expired, and spells an actor without its
/// `#alias` suffix.
struct OwedProvider {
    answer: std::sync::atomic::AtomicU8,
    takes_delegations: bool,
    key_held: AtomicBool,
    expired: AtomicBool,
    /// Expire the delegation while the write is on its way.
    expires_in_flight: AtomicBool,
    /// The tokens the writes were sent with.
    tokens: SegQueue<String>,
}

impl OwedProvider {
    fn new(takes_delegations: bool) -> Arc<Self> {
        Arc::new(Self {
            answer: std::sync::atomic::AtomicU8::new(Answer::Applied as u8),
            takes_delegations,
            key_held: AtomicBool::new(true),
            expired: AtomicBool::new(false),
            expires_in_flight: AtomicBool::new(false),
            tokens: SegQueue::new(),
        })
    }

    fn answer(&self, answer: Answer) {
        self.answer.store(answer as u8, Ordering::Relaxed);
    }

    fn sent(&self) -> Vec<String> {
        std::iter::from_fn(|| self.tokens.pop()).collect()
    }

    fn write(&self, token: &str) -> std::result::Result<bool, ProviderError> {
        self.tokens.push(token.to_string());
        if self.expires_in_flight.load(Ordering::Relaxed) {
            self.expired.store(true, Ordering::Relaxed);
        }
        let answer = self.answer.load(Ordering::Relaxed);
        if answer == Answer::Applied as u8 {
            Ok(true)
        } else if answer == Answer::Rejected as u8 {
            Err(ProviderError::Transaction("rejected".into()))
        } else if answer == Answer::Unreachable as u8 {
            Err(ProviderError::Unavailable("no route".into()))
        } else {
            Err(ProviderError::Query("no answer".into()))
        }
    }
}

#[async_trait]
impl VeraProvider for OwedProvider {
    fn authorized_account(&self) -> String {
        "0x0".to_string()
    }

    async fn create_bearer_token(&self, did: &str) -> std::result::Result<String, ProviderError> {
        self.create_bearer_token_as(&RequestContext::ambient(), did)
            .await
    }

    async fn create_bearer_token_as(
        &self,
        context: &RequestContext,
        did: &str,
    ) -> std::result::Result<String, ProviderError> {
        if self.holds_key_for(did) {
            return Ok(format!("signed:{did}"));
        }
        context
            .bearer_token_for(did)
            .ok_or_else(|| ProviderError::Config("no signing config".into()))
    }

    fn self_did(&self) -> Option<String> {
        None
    }

    fn canonical_actor(&self, did: &str) -> String {
        did.split('#').next().unwrap_or(did).to_string()
    }

    fn holds_key_for(&self, did: &str) -> bool {
        did == HELD && self.key_held.load(Ordering::Relaxed)
    }

    fn write_refused(&self, error: &ProviderError) -> bool {
        matches!(error, ProviderError::Transaction(_))
    }

    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        self.takes_delegations.then(|| acp::DelegationAudience {
            worker: "did:key:worker".into(),
            deployment: 1,
        })
    }

    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<std::result::Result<String, ProviderError>> {
        let did = token.strip_prefix("delegation:")?;
        Some(if self.expired.load(Ordering::Relaxed) {
            Err(ProviderError::Config("caller delegation: expired".into()))
        } else {
            Ok(did.to_string())
        })
    }

    async fn create_policy(
        &self,
        _policy_yaml: &str,
    ) -> std::result::Result<String, ProviderError> {
        unreachable!("no policy is created here")
    }

    async fn register_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        Ok(())
    }

    async fn archive_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(), ProviderError> {
        Ok(())
    }

    async fn set_relationship(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        self.write(bearer_token)
    }

    async fn delete_relationship(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> std::result::Result<bool, ProviderError> {
        self.write(bearer_token)
    }

    async fn query_policy(
        &self,
        _policy_id: &str,
    ) -> std::result::Result<Option<crate::ProviderPolicyInfo>, ProviderError> {
        Ok(None)
    }

    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> std::result::Result<(bool, String), ProviderError> {
        Ok((false, String::new()))
    }

    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> std::result::Result<bool, ProviderError> {
        Ok(permission == "manager" && actor_did == HELD)
    }
}

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

fn change(actor: &str, authority: OwedAuthority, op: OwedOp) -> OwedChange {
    OwedChange {
        seq: 1,
        txn: None,
        tuple: OwedTuple {
            policy_id: "policy-1".into(),
            resource: "users".into(),
            object: "doc-1".into(),
            relation: "reader".into(),
            subject: OwedSubject::Actor { did: did(REMOTE) },
        },
        op,
        actor: did(actor),
        authority,
        state: OwedState::Staged,
        attempts: 0,
        last_error: None,
    }
}

fn delegation(actor: &str) -> OwedAuthority {
    OwedAuthority::Delegation {
        token: format!("delegation:{actor}"),
    }
}

fn carrying(acp: &VeraDocumentACP, actor: &str, token: &str) -> Arc<dyn DocumentACP> {
    acp.bind_request(
        &RequestContext::default().with_bearer(Some(actor.to_string()), Some(token.to_string())),
    )
    .expect("a Vera ACP serves the request it is bound to")
}

#[test]
fn a_change_records_the_nodes_key_before_the_requests_delegation() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let unbound = carrying(&acp, "did:key:nobody", "unused");

    assert_eq!(
        unbound.owed_authority(&did(HELD)).unwrap(),
        OwedAuthority::NodeKey
    );
    assert_eq!(
        unbound
            .owed_authority(&did(&format!("{HELD}#alias")))
            .unwrap(),
        OwedAuthority::NodeKey,
        "another spelling of the held key"
    );
    assert!(matches!(
        unbound.owed_authority(&did(REMOTE)),
        Err(acp::Error::OwedAuthorityUnavailable { actor, .. }) if actor == REMOTE
    ));

    let token = format!("delegation:{REMOTE}");
    assert_eq!(
        carrying(&acp, REMOTE, &token)
            .owed_authority(&did(REMOTE))
            .unwrap(),
        delegation(REMOTE)
    );
    // A token that is no delegation, another caller's, or an expired one
    // cannot be submitted later, so nothing is recorded.
    for refused in ["an.identity.token", &format!("delegation:{HELD}")] {
        assert!(matches!(
            carrying(&acp, REMOTE, refused).owed_authority(&did(REMOTE)),
            Err(acp::Error::OwedAuthorityUnavailable { .. })
        ));
    }
    provider.expired.store(true, Ordering::Relaxed);
    assert!(matches!(
        carrying(&acp, REMOTE, &token).owed_authority(&did(REMOTE)),
        Err(acp::Error::OwedAuthorityUnavailable { .. })
    ));
}

#[test]
fn a_provider_that_takes_no_delegation_is_passed_its_callers_token() {
    let acp = VeraDocumentACP::without_access_cache(OwedProvider::new(false));
    assert_eq!(
        carrying(&acp, REMOTE, "a.cosmos.token")
            .owed_authority(&did(REMOTE))
            .unwrap(),
        OwedAuthority::Delegation {
            token: "a.cosmos.token".into()
        }
    );
}

#[nuclei::test]
async fn a_submission_is_read_as_applied_refused_or_not_delivered() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    // The request a submission runs under lends it nothing.
    let bound = carrying(&acp, HELD, "the.running.requests.token");
    let keyed = change(HELD, OwedAuthority::NodeKey, OwedOp::Add);

    for op in [OwedOp::Add, OwedOp::Delete] {
        let keyed = change(HELD, OwedAuthority::NodeKey, op);
        assert_eq!(bound.submit_owed_change(&keyed).await, Submission::Applied);
    }
    assert_eq!(provider.sent(), vec![format!("signed:{HELD}"); 2]);

    provider.answer(Answer::Rejected);
    assert!(matches!(
        bound.submit_owed_change(&keyed).await,
        Submission::Refused { reason } if reason.contains("rejected")
    ));
    for unknown in [Answer::Unreachable, Answer::Unclear] {
        provider.answer(unknown);
        assert!(matches!(
            bound.submit_owed_change(&keyed).await,
            Submission::Unavailable { .. }
        ));
    }

    provider.answer(Answer::Applied);
    let typed = OwedChange {
        tuple: OwedTuple {
            subject: OwedSubject::TypedWildcard {
                resource: "groups".into(),
            },
            ..keyed.tuple.clone()
        },
        ..keyed.clone()
    };
    provider.sent();
    assert!(matches!(
        bound.submit_owed_change(&typed).await,
        Submission::Refused { .. }
    ));
    assert!(
        provider.sent().is_empty(),
        "nothing the provider cannot store is sent"
    );
}

#[nuclei::test]
async fn a_change_whose_authority_is_gone_is_not_sent_and_awaits_it() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let delegated = change(REMOTE, delegation(REMOTE), OwedOp::Delete);

    assert_eq!(
        acp.submit_owed_change(&delegated).await,
        Submission::Applied
    );
    assert_eq!(provider.sent(), [format!("delegation:{REMOTE}")]);

    // A rejection that comes back with the delegation expired is about the
    // delegation, not about the change.
    provider.answer(Answer::Rejected);
    provider.expires_in_flight.store(true, Ordering::Relaxed);
    assert!(matches!(
        acp.submit_owed_change(&delegated).await,
        Submission::AwaitingAuthority { reason } if reason.contains("expired")
    ));
    assert_eq!(provider.sent().len(), 1);

    provider.answer(Answer::Applied);
    assert!(matches!(
        acp.submit_owed_change(&delegated).await,
        Submission::AwaitingAuthority { .. }
    ));
    let stolen = change(HELD, delegation(REMOTE), OwedOp::Add);
    provider.expired.store(false, Ordering::Relaxed);
    provider.expires_in_flight.store(false, Ordering::Relaxed);
    provider.key_held.store(false, Ordering::Relaxed);
    assert!(matches!(
        acp.submit_owed_change(&stolen).await,
        Submission::AwaitingAuthority { .. }
    ));
    assert!(matches!(
        acp.submit_owed_change(&change(HELD, OwedAuthority::NodeKey, OwedOp::Add))
            .await,
        Submission::AwaitingAuthority { reason } if reason.contains("no longer holds")
    ));
    assert!(provider.sent().is_empty());
}

#[nuclei::test]
async fn a_managing_relation_is_asked_as_a_named_permission() {
    let acp = VeraDocumentACP::without_access_cache(OwedProvider::new(true));
    let held = |relation: &'static str| {
        let acp = &acp;
        async move {
            acp.check_named_permission(&did(HELD), relation, "policy-1", "users", "doc-1")
                .await
                .unwrap()
        }
    };
    assert!(held("manager").await);
    assert!(!held("reader").await);
    assert_eq!(
        acp.canonical_actor(&did(&format!("{REMOTE}#alias"))),
        did(REMOTE)
    );
}
