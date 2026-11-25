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
/// DID's delegation and `request:<did>|<command>` ([`bound_to`]) as that
/// DID's delegation bound to one command, each until it is expired, and
/// spells an actor without its `#alias` suffix.
struct OwedProvider {
    answer: std::sync::atomic::AtomicU8,
    takes_delegations: bool,
    /// Whether a failed transaction is a verified rejection.
    classifies: AtomicBool,
    key_held: AtomicBool,
    /// The key is gone the moment after the node was told it holds it.
    key_vanishes: AtomicBool,
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
            classifies: AtomicBool::new(true),
            key_held: AtomicBool::new(true),
            key_vanishes: AtomicBool::new(false),
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
        let held = did == HELD && self.key_held.load(Ordering::Relaxed);
        if held && self.key_vanishes.load(Ordering::Relaxed) {
            self.key_held.store(false, Ordering::Relaxed);
        }
        held
    }

    fn write_refused(&self, error: &ProviderError) -> bool {
        self.classifies.load(Ordering::Relaxed) && matches!(error, ProviderError::Transaction(_))
    }

    fn takes_subject_commands(&self) -> bool {
        self.takes_delegations
    }

    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        self.takes_delegations.then(|| acp::DelegationAudience {
            worker: "did:key:worker".into(),
            deployment: 1,
            genesis_id: None,
        })
    }

    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<std::result::Result<acp::AuthenticatedCaller, ProviderError>> {
        let did = token.strip_prefix("delegation:")?;
        Some(if self.expired.load(Ordering::Relaxed) {
            Err(ProviderError::Config("caller delegation: expired".into()))
        } else {
            Ok(acp::AuthenticatedCaller {
                did: identity::Did::new(did).expect("fixture DID"),
                expires_at: u64::MAX,
            })
        })
    }

    fn caller_of_request_delegation(
        &self,
        token: &str,
        command: &crate::PolicyCommand<'_>,
    ) -> Option<std::result::Result<String, ProviderError>> {
        let (did, names) = token.strip_prefix("request:")?.split_once('|')?;
        Some(if self.expired.load(Ordering::Relaxed) {
            Err(ProviderError::Config("request delegation: expired".into()))
        } else if names != named(command) {
            Err(ProviderError::Config(
                "request delegation: bound to another request".into(),
            ))
        } else {
            Ok(did.to_string())
        })
    }

    async fn create_policy(
        &self,
        _policy_yaml: &str,
        _nonce: Option<acp::PolicyNonce>,
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

    async fn set_relationship_subject_as(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _kind: u8,
        _subject_resource: &str,
        _subject_object_id: &str,
        _subject_relation: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.write(bearer_token)
    }

    async fn delete_relationship_subject_as(
        &self,
        bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _kind: u8,
        _subject_resource: &str,
        _subject_object_id: &str,
        _subject_relation: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.write(bearer_token)
    }

    async fn set_relationship_subject(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _kind: u8,
        _subject_resource: &str,
        _subject_object_id: &str,
        _subject_relation: &str,
    ) -> std::result::Result<bool, ProviderError> {
        self.write("the providers own actor")
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

/// The words a delegation bound to one request names `command` by.
fn named(command: &crate::PolicyCommand<'_>) -> String {
    use crate::{PolicyAction, PolicySubject};

    let subject = |subject: &PolicySubject<'_>| match subject {
        PolicySubject::Actor(SubjectRef::Actor(did)) => did.clone(),
        PolicySubject::Actor(_) => "*".to_owned(),
        PolicySubject::Structured {
            resource,
            object_id,
            relation,
            ..
        } => format!("{resource}:{object_id}#{relation}"),
    };
    let object = command.object_id;
    match &command.action {
        PolicyAction::RegisterObject => format!("register {object}"),
        PolicyAction::RegisterUnderParent {
            resource,
            object_id,
        } => format!("register {object} under {resource}:{object_id}"),
        PolicyAction::SetRelationship {
            relation,
            subject: on,
        } => format!("set {object} {relation} {}", subject(on)),
        PolicyAction::DeleteRelationship {
            relation,
            subject: on,
        } => format!("delete {object} {relation} {}", subject(on)),
        PolicyAction::ArchiveObject => format!("archive {object}"),
    }
}

/// `actor`'s delegation bound to the one command `command` names.
fn bound_to(actor: &str, command: &str) -> String {
    format!("request:{actor}|{command}")
}

/// `acp` serving a request that presents `token` as `actor`'s delegation
/// bound to the one command the request sends.
fn presenting(acp: &VeraDocumentACP, actor: &str, token: &str) -> Arc<dyn DocumentACP> {
    acp.bind_request(
        &RequestContext::default().with_request_delegation(actor.to_string(), token.to_string()),
    )
    .expect("a Vera ACP serves the request it is bound to")
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
        authority_checked: true,
        epoch: None,
        in_flight: None,
        alone: false,
        with_registration: false,
        deleted_by_actor: false,
        outside_admission_counts: false,
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
        carrying(&acp, REMOTE, "a.plain.token")
            .owed_authority(&did(REMOTE))
            .unwrap(),
        OwedAuthority::Delegation {
            token: "a.plain.token".into()
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
    // A chain that was not reached and one that answered with an error it
    // does not classify are told apart: only the second counts toward
    // taking the change as refused.
    provider.answer(Answer::Unreachable);
    assert!(matches!(
        bound.submit_owed_change(&keyed).await,
        Submission::Unavailable { .. }
    ));
    // A failed transaction the provider does not classify is its answer
    // all the same. An error before anything was sent is the node's own.
    provider.classifies.store(false, Ordering::Relaxed);
    provider.answer(Answer::Rejected);
    assert!(matches!(
        bound.submit_owed_change(&keyed).await,
        Submission::Failed { reason } if reason.contains("rejected")
    ));
    provider.classifies.store(true, Ordering::Relaxed);
    provider.answer(Answer::Unclear);
    assert!(matches!(
        bound.submit_owed_change(&keyed).await,
        Submission::NotSent { reason } if reason.contains("no answer")
    ));

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
        Submission::Unsupported { .. }
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

/// A key that is gone between the node being told it holds it and the token
/// being signed leaves the change waiting for authority. The token the
/// process holds for the DID is not sent in its place.
#[nuclei::test]
async fn a_key_that_vanishes_before_signing_is_not_replaced_by_the_process_token() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    defra_core::signing::set_request_bearer_token(HELD, format!("delegation:{HELD}"));
    provider.key_vanishes.store(true, Ordering::Relaxed);
    let submitted = acp
        .submit_owed_change(&change(HELD, OwedAuthority::NodeKey, OwedOp::Add))
        .await;
    defra_core::signing::clear_request_bearer_token(HELD);
    assert!(
        matches!(submitted, Submission::AwaitingAuthority { .. }),
        "{submitted:?}"
    );
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

/// A userset or cross-object subject is submitted under the authority
/// recorded with the change, as an actor subject is, and never under the
/// provider's own actor.
#[nuclei::test]
async fn a_change_of_a_userset_subject_is_sent_under_its_recorded_authority() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let members = OwedSubject::EntitySet {
        resource: "groups".into(),
        object: "team".into(),
        relation: "member".into(),
    };
    let on_members = |actor: &str, authority: OwedAuthority, op: OwedOp| {
        let mut change = change(actor, authority, op);
        change.tuple.subject = members.clone();
        change
    };

    for op in [OwedOp::Add, OwedOp::Delete] {
        assert_eq!(
            acp.submit_owed_change(&on_members(REMOTE, delegation(REMOTE), op))
                .await,
            Submission::Applied
        );
        assert_eq!(
            acp.submit_owed_change(&on_members(HELD, OwedAuthority::NodeKey, op))
                .await,
            Submission::Applied
        );
    }
    assert_eq!(
        provider.sent(),
        [
            format!("delegation:{REMOTE}"),
            format!("signed:{HELD}"),
            format!("delegation:{REMOTE}"),
            format!("signed:{HELD}"),
        ]
    );

    // The provider's refusal of the caller is the change's, whoever the
    // provider's own actor is.
    provider.answer(Answer::Rejected);
    assert!(matches!(
        acp.submit_owed_change(&on_members(REMOTE, delegation(REMOTE), OwedOp::Add))
            .await,
        Submission::Refused { .. }
    ));
    assert_eq!(provider.sent(), [format!("delegation:{REMOTE}")]);
}

#[test]
fn a_subject_is_staged_only_where_the_provider_can_be_sent_it() {
    let members = Subject::entity_set("groups", "team", "member");
    let typed = Subject::TypedWildcard {
        resource: "groups".into(),
    };
    for takes in [true, false] {
        let acp = VeraDocumentACP::without_access_cache(OwedProvider::new(takes));
        assert!(acp.submits_subject(&Subject::Entity(did(REMOTE))));
        assert!(acp.submits_subject(&Subject::Wildcard));
        assert_eq!(acp.submits_subject(&members), takes);
        assert!(!acp.submits_subject(&typed));
    }
}

#[test]
fn a_delegation_bound_to_one_request_counts_for_the_command_it_names_only() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let registration = |object| acp::ProviderCommand::RegisterObject {
        policy_id: "policy-1",
        resource: "users",
        object,
    };
    let unavailable = |authority: Result<OwedAuthority>| {
        matches!(
            authority,
            Err(acp::Error::OwedAuthorityUnavailable { actor, .. }) if actor == REMOTE
        )
    };
    let token = bound_to(REMOTE, "register doc-1");
    let bound = presenting(&acp, REMOTE, &token);

    assert_eq!(
        bound
            .command_authority(&did(REMOTE), registration("doc-1"))
            .unwrap(),
        OwedAuthority::Delegation {
            token: token.clone()
        }
    );
    // Another document, another kind of command, and no command at all.
    assert!(unavailable(
        bound.command_authority(&did(REMOTE), registration("doc-2"))
    ));
    let granted = change(REMOTE, OwedAuthority::NodeKey, OwedOp::Add);
    assert!(unavailable(bound.command_authority(
        &did(REMOTE),
        acp::ProviderCommand::Relationship {
            tuple: &granted.tuple,
            op: granted.op,
        },
    )));
    assert!(unavailable(bound.owed_authority(&did(REMOTE))));

    // Presented as a plain delegation it is none, and a plain delegation is
    // not taken where one bound to the request is presented.
    assert!(unavailable(
        carrying(&acp, REMOTE, &token).command_authority(&did(REMOTE), registration("doc-1"))
    ));
    assert!(unavailable(
        presenting(&acp, REMOTE, &format!("delegation:{REMOTE}"))
            .command_authority(&did(REMOTE), registration("doc-1"))
    ));
    // Signed by somebody else.
    assert!(unavailable(
        presenting(&acp, REMOTE, &bound_to(HELD, "register doc-1"))
            .command_authority(&did(REMOTE), registration("doc-1"))
    ));
    // A key the node holds needs no delegation, whatever is presented.
    assert_eq!(
        presenting(&acp, HELD, &bound_to(HELD, "register doc-2"))
            .command_authority(&did(HELD), registration("doc-1"))
            .unwrap(),
        OwedAuthority::NodeKey
    );

    provider.expired.store(true, Ordering::Relaxed);
    assert!(unavailable(
        bound.command_authority(&did(REMOTE), registration("doc-1"))
    ));
}

#[nuclei::test]
async fn a_command_is_not_sent_with_a_delegation_bound_to_another() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let granting = bound_to(REMOTE, &format!("set doc-1 reader {REMOTE}"));
    let bound = presenting(&acp, REMOTE, &granting);
    let grant = |object: &'static str, relation: &'static str| {
        let bound = bound.clone();
        async move {
            bound
                .add_actor_relationship(
                    &did(REMOTE),
                    &did(REMOTE),
                    "policy-1",
                    "users",
                    object,
                    relation,
                    &[],
                )
                .await
        }
    };

    for (object, relation) in [("doc-2", "reader"), ("doc-1", "writer")] {
        assert!(matches!(
            grant(object, relation).await,
            Err(acp::Error::OwedAuthorityUnavailable { .. })
        ));
    }
    assert!(matches!(
        bound
            .delete_actor_relationship(
                &did(REMOTE),
                &did(REMOTE),
                "policy-1",
                "users",
                "doc-1",
                "reader",
                &[],
            )
            .await,
        Err(acp::Error::OwedAuthorityUnavailable { .. })
    ));
    assert!(matches!(
        bound
            .register_doc_object(&did(REMOTE), "policy-1", "users", "doc-1")
            .await,
        Err(acp::Error::OwedAuthorityUnavailable { .. })
    ));
    assert!(provider.sent().is_empty(), "nothing reached the provider");

    assert!(grant("doc-1", "reader").await.unwrap());
    assert_eq!(provider.sent(), [granting]);
}

#[nuclei::test]
async fn an_owed_change_under_a_delegation_bound_to_it_is_sent_until_that_lapses() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let token = bound_to(REMOTE, &format!("set doc-1 reader {REMOTE}"));
    let owed = change(
        REMOTE,
        OwedAuthority::Delegation {
            token: token.clone(),
        },
        OwedOp::Add,
    );

    assert_eq!(acp.submit_owed_change(&owed).await, Submission::Applied);
    assert_eq!(provider.sent(), std::slice::from_ref(&token));

    // The same token recorded with the revocation of that tuple is bound to
    // another request: the change is not sent.
    let revocation = change(
        REMOTE,
        OwedAuthority::Delegation {
            token: token.clone(),
        },
        OwedOp::Delete,
    );
    assert!(matches!(
        acp.submit_owed_change(&revocation).await,
        Submission::AwaitingAuthority { reason } if reason.contains("another request")
    ));

    provider.expired.store(true, Ordering::Relaxed);
    assert!(matches!(
        acp.submit_owed_change(&owed).await,
        Submission::AwaitingAuthority { reason } if reason.contains("expired")
    ));
    assert!(provider.sent().is_empty());
}

#[nuclei::test]
async fn a_plain_delegation_is_checked_again_as_its_command_is_sent() {
    let provider = OwedProvider::new(true);
    let acp = VeraDocumentACP::without_access_cache(provider.clone());
    let token = format!("delegation:{REMOTE}");
    let grant = |acp: Arc<dyn DocumentACP>| async move {
        acp.add_actor_relationship(
            &did(REMOTE),
            &did(REMOTE),
            "policy-1",
            "users",
            "doc-1",
            "reader",
            &[],
        )
        .await
    };
    let refused = |result: Result<bool>| {
        matches!(
            result,
            Err(acp::Error::OwedAuthorityUnavailable { actor, .. }) if actor == REMOTE
        )
    };

    // A token that is no delegation, another signer's, and a plain one
    // where the request presents one bound to a request.
    for other in ["an.identity.token", &format!("delegation:{HELD}")] {
        assert!(refused(grant(carrying(&acp, REMOTE, other)).await));
    }
    assert!(refused(grant(presenting(&acp, REMOTE, &token)).await));
    assert!(provider.sent().is_empty());

    assert!(grant(carrying(&acp, REMOTE, &token)).await.unwrap());
    assert_eq!(provider.sent(), std::slice::from_ref(&token));

    // Expired between the write and the commit: neither the registration
    // nor a relationship goes out under it.
    provider.expired.store(true, Ordering::Relaxed);
    let stale = carrying(&acp, REMOTE, &token);
    assert!(refused(grant(stale.clone()).await));
    assert!(matches!(
        stale
            .register_doc_object(&did(REMOTE), "policy-1", "users", "doc-1")
            .await,
        Err(acp::Error::OwedAuthorityUnavailable { .. })
    ));
    assert!(provider.sent().is_empty());

    // The node's own key signs a token as the command is sent.
    assert!(carrying(&acp, HELD, "unused")
        .add_actor_relationship(
            &did(HELD),
            &did(REMOTE),
            "policy-1",
            "users",
            "doc-1",
            "reader",
            &[],
        )
        .await
        .unwrap());
    assert_eq!(provider.sent(), [format!("signed:{HELD}")]);
}

#[test]
fn a_verified_write_takes_no_delegation_the_node_cannot_verify() {
    let acp = VeraDocumentACP::without_access_cache(OwedProvider::new(false));
    let verified = acp
        .bind_request(
            &RequestContext::default()
                .with_transactional_authority_required()
                .with_bearer(Some(REMOTE.to_string()), Some("a.plain.token".into())),
        )
        .expect("a Vera ACP serves the request it is bound to");
    assert!(matches!(
        verified.owed_authority(&did(REMOTE)),
        Err(acp::Error::OwedAuthorityUnavailable { reason, .. }) if reason.contains("cannot verify")
    ));
    assert!(matches!(
        verified.command_authority(
            &did(REMOTE),
            acp::ProviderCommand::RegisterObject {
                policy_id: "policy-1",
                resource: "users",
                object: "doc-1",
            },
        ),
        Err(acp::Error::OwedAuthorityUnavailable { .. })
    ));
    // A key the node holds still admits it.
    assert_eq!(
        verified.owed_authority(&did(HELD)).unwrap(),
        OwedAuthority::NodeKey
    );
}
