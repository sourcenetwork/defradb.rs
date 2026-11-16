//! Verified writes on a remote document ACP provider: admitted only when the
//! node may act for the write's signer there, with a key it holds, the
//! signer's delegation, or the signer's delegation bound to the one command
//! the write sends.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Did, Identity as _};
use query::executor::WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE;
use serde_json::json;

use super::staged_relationship_tests::{
    actor, node_identity, provision, request_token, start, Remote, READER,
};
use super::{Node, RelationshipChange, SignerCredential};

const STRANGER: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";

/// A user whose key the node does not hold, and what signs its blocks.
fn user() -> (String, SigningConfig) {
    let private = crypto::generate_ed25519().unwrap();
    let public = private.public_key();
    let config = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: private.raw_owned(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    };
    let did = identity::RawIdentity::from_private_key(private)
        .unwrap()
        .did()
        .unwrap()
        .to_string();
    (did, config)
}

/// A node on the stub remote provider and one user writing through it.
struct Verified {
    remote: Arc<Remote>,
    node: Node,
    user: String,
    signing: SigningConfig,
    policy: String,
}

/// One verified create handed to the node.
struct Attempt {
    doc: String,
    txn: query::TransactionHandle,
    response: query::QueryResponse,
}

impl Verified {
    async fn new() -> Self {
        let remote = Remote::new();
        let node = start(&node_identity(), Some(&remote), None).await;
        provision(&node).await;
        let (user, signing) = user();
        let schema = node.get_collection("Record").unwrap().unwrap();
        let policy = schema.policy.as_ref().unwrap().id.clone();
        Self {
            remote,
            node,
            user,
            signing,
            policy,
        }
    }

    /// The user's verified create of a record called `name`, with the
    /// credential `credential` makes for the document it creates.
    async fn create(
        &self,
        name: &str,
        credential: impl FnOnce(&str) -> SignerCredential,
    ) -> Attempt {
        let schema = self.node.get_collection("Record").unwrap().unwrap();
        let crdts = storage::Crdts::default();
        let input = json!({ "name": name });
        let signed = db::block::builder::ClientCreateSignatures::from_input(
            &crdts,
            &schema,
            &input,
            &self.signing,
        )
        .unwrap();
        let graph = db::block::builder::VerifiedClientCreate::verify(
            &crdts,
            &schema,
            &input,
            signed.evidence(),
        )
        .unwrap();
        let request = query::QueryRequest::new(
            "mutation($input:RecordMutationInputArg!) { create_Record(input:$input) {_docID} }",
        )
        .with_variables(json!({ "input": input }))
        .with_identity(Some(Did::new(&self.user).unwrap()));
        let doc = graph.doc_id().to_owned();
        let txn = self.node.begin_transaction(false).await.unwrap();
        let response = self
            .node
            .execute_verified_create_in_txn(request, &txn, &graph, credential(&doc))
            .await;
        Attempt { doc, txn, response }
    }

    fn delegation(&self) -> SignerCredential {
        SignerCredential::Delegation(format!("delegation:{}", self.user))
    }

    fn registration(&self, doc: &str) -> SignerCredential {
        SignerCredential::RequestBound(request_token(&self.user, &format!("register {doc}")))
    }

    /// The attempt was refused for want of authority and left nothing: no
    /// transaction to commit, no row, nothing owed and nothing sent.
    async fn refused(&self, attempt: Attempt) {
        let Attempt { doc, txn, response } = attempt;
        assert_eq!(
            response.errors.first().and_then(|error| error.code()),
            Some(WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE),
            "{:?}",
            response.errors
        );
        assert!(self.node.commit_transaction(&txn).await.is_err());
        assert!(self.records().await.is_empty());
        assert!(self.owed().await.is_empty());
        assert!(!self
            .node
            .document_acp()
            .is_doc_registered(&self.policy, "rows", &doc)
            .await
            .unwrap());
        assert!(self.remote.owners.get(&doc).is_none());
        assert!(self.remote.take_writes().is_empty());
    }

    /// The attempt was staged; its commit registers the user as the owner.
    async fn committed(&self, attempt: Attempt) -> String {
        let Attempt { doc, txn, response } = attempt;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert!(
            self.remote.owners.get(&doc).is_none(),
            "nothing is sent before the commit"
        );
        self.node.commit_transaction(&txn).await.unwrap();
        assert_eq!(
            self.remote.owners.get(&doc).as_deref(),
            Some(self.user.as_str())
        );
        doc
    }

    /// The names of the records the user reads.
    async fn records(&self) -> Vec<String> {
        let response = self
            .node
            .execute_request(
                query::QueryRequest::new("{ Record { name } }")
                    .with_identity(Some(Did::new(&self.user).unwrap())),
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        response.data.unwrap()["Record"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["name"].as_str().unwrap().to_owned())
            .collect()
    }

    /// The relationship changes the node owes the provider, as committed.
    async fn owed(&self) -> Vec<acp::OwedChange> {
        let database = self.node.database();
        let txn = database.new_txn(true).await.unwrap();
        let store: Arc<dyn storage::corekv::Store> = database.store().clone();
        let changes =
            acp::OwedChanges::new(&txn.acpstore().unwrap(), store, acp::StagedCount::new());
        let queue = changes.page(None).await.unwrap();
        drop(changes);
        txn.discard().unwrap();
        queue
    }

    /// The user acting with `token` as its delegation bound to one request.
    fn bound(&self, token: String) -> defra_core::RequestContext {
        actor(&self.user).with_request_delegation(self.user.clone(), token)
    }

    async fn grant(
        &self,
        txn: &query::TransactionHandle,
        context: &defra_core::RequestContext,
        doc: &str,
    ) -> crate::Result<RelationshipChange> {
        self.node
            .add_dac_actor_relationship_change_for_in_txn(
                txn, context, "Record", doc, "reader", READER,
            )
            .await
    }
}

#[test]
fn a_verified_create_is_staged_only_with_authority_for_its_signer() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;

        // No key and no credential.
        on.refused(on.create("unowned", |_| SignerCredential::None).await)
            .await;
        // A delegation somebody else signed.
        on.refused(
            on.create("foreign", |_| {
                SignerCredential::Delegation(format!("delegation:{STRANGER}"))
            })
            .await,
        )
        .await;
        // A token that is no delegation to this node.
        on.refused(
            on.create("identity", |_| {
                SignerCredential::Delegation("an.identity.token".into())
            })
            .await,
        )
        .await;

        on.committed(on.create("delegated", |_| on.delegation()).await)
            .await;
        assert_eq!(on.records().await, ["delegated"]);

        // A key the node holds for the signer needs no credential.
        on.remote.keys.insert(on.user.clone(), ());
        on.committed(on.create("held", |_| SignerCredential::None).await)
            .await;
        let mut records = on.records().await;
        records.sort();
        assert_eq!(records, ["delegated", "held"]);
        assert!(on.owed().await.is_empty());
        // The graph's signatures were made before the write: no access
        // decision is recorded to sign under.
        assert_eq!(on.remote.decisions.load(Ordering::SeqCst), 0);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_request_bound_delegation_covers_the_registration_it_names_and_nothing_else() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;

        // Bound to the registration of another document.
        on.refused(
            on.create("other", |_| on.registration("bae-another-document"))
                .await,
        )
        .await;
        // Bound to another command on this document.
        on.refused(
            on.create("grant", |doc| {
                SignerCredential::RequestBound(request_token(
                    &on.user,
                    &format!("set {doc} reader {READER}"),
                ))
            })
            .await,
        )
        .await;
        // Signed by somebody else.
        on.refused(
            on.create("foreign", |doc| {
                SignerCredential::RequestBound(request_token(STRANGER, &format!("register {doc}")))
            })
            .await,
        )
        .await;
        // A plain delegation is not taken where one bound to the request is
        // presented, and one bound to the request is not a plain delegation.
        on.refused(
            on.create("plain", |_| {
                SignerCredential::RequestBound(format!("delegation:{}", on.user))
            })
            .await,
        )
        .await;
        on.refused(
            on.create("misfiled", |doc| {
                SignerCredential::Delegation(request_token(&on.user, &format!("register {doc}")))
            })
            .await,
        )
        .await;
        // Expired before the write.
        on.remote.lapsed.store(true, Ordering::SeqCst);
        on.refused(on.create("late", |doc| on.registration(doc)).await)
            .await;
        on.remote.lapsed.store(false, Ordering::SeqCst);

        on.committed(on.create("bound", |doc| on.registration(doc)).await)
            .await;
        assert_eq!(on.records().await, ["bound"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_grant_in_a_verified_transaction_takes_its_own_request_bound_delegation() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("shared", |doc| on.registration(doc)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let granting = request_token(&on.user, &format!("set {doc} reader {READER}"));

        let unavailable = |staged: crate::Result<RelationshipChange>| {
            matches!(
                staged,
                Err(crate::Error::Acp(
                    acp::Error::OwedAuthorityUnavailable { .. }
                ))
            )
        };
        // The registration's delegation does not cover the grant.
        let registering = request_token(&on.user, &format!("register {doc}"));
        assert!(unavailable(
            on.grant(&txn, &on.bound(registering), &doc).await
        ));
        // Nor does one bound to a grant of another relation or subject.
        for other in [
            format!("set {doc} writer {READER}"),
            format!("set {doc} reader {STRANGER}"),
            format!("delete {doc} reader {READER}"),
        ] {
            let token = request_token(&on.user, &other);
            assert!(unavailable(on.grant(&txn, &on.bound(token), &doc).await));
        }
        // Presented as a plain delegation it is not taken at all.
        let plain = actor(&on.user).with_bearer(Some(on.user.clone()), Some(granting.clone()));
        assert!(unavailable(on.grant(&txn, &plain, &doc).await));

        let staged = on.grant(&txn, &on.bound(granting), &doc).await;
        assert!(
            matches!(
                staged,
                Ok(RelationshipChange::Staged {
                    authority_checked: true,
                    ..
                })
            ),
            "{staged:?}"
        );
        assert!(on.remote.take_writes().is_empty());
        on.node.commit_transaction(&txn).await.unwrap();

        // The stub takes a grant from an object's owner only, so the grant
        // landed after the registration.
        assert_eq!(
            on.remote.owners.get(&doc).as_deref(),
            Some(on.user.as_str())
        );
        assert_eq!(
            on.remote.take_writes(),
            [(true, doc.clone(), READER.to_string())]
        );
        assert!(on.owed().await.is_empty());
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_request_bound_delegation_that_expires_before_submission_parks_its_change() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let Attempt { doc, txn, response } = on.create("parked", |_| on.delegation()).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let granting = request_token(&on.user, &format!("set {doc} reader {READER}"));
        assert!(matches!(
            on.grant(&txn, &on.bound(granting), &doc).await,
            Ok(RelationshipChange::Staged { .. })
        ));

        on.remote.lapsed.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();

        assert_eq!(
            on.remote.owners.get(&doc).as_deref(),
            Some(on.user.as_str())
        );
        assert!(!on.remote.holds(&doc, READER));
        assert!(on.remote.take_writes().is_empty());
        let owed = on.owed().await;
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].state, acp::OwedState::AwaitingAuthority);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_verified_update_needs_the_signers_general_authority() {
    use db::block::builder::{
        ClientUpdateSignatures, ClientUpdateSnapshot, VerifiedClientMutation,
    };

    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let doc = on
            .committed(on.create("before", |_| on.delegation()).await)
            .await;
        let user = Did::new(&on.user).unwrap();
        let history = on
            .node
            .execute_request(
                query::QueryRequest::new("query($docID:[String!]!) { _commits(docID:$docID) { cid docID fieldName height delta collectionVersionId links { cid fieldName } heads { cid } signature { type identity value } } }")
                    .with_variables(json!({ "docID": [doc] }))
                    .with_identity(Some(user.clone())),
            )
            .await;
        assert!(!history.has_errors(), "{:?}", history.errors);
        let snapshot = ClientUpdateSnapshot::from_commits(
            &doc,
            history.data.unwrap()["_commits"].as_array().unwrap(),
        )
        .unwrap();
        let schema = on.node.get_collection("Record").unwrap().unwrap();
        let crdts = storage::Crdts::default();
        let patch = json!({ "name": "after" });
        let update = |credential: SignerCredential| {
            let (on, schema, crdts, patch, snapshot, doc, user) =
                (&on, &schema, &crdts, &patch, &snapshot, &doc, &user);
            async move {
                let signed = ClientUpdateSignatures::from_input(
                    crdts, schema, doc, patch, snapshot, &on.signing,
                )
                .unwrap();
                let graph = VerifiedClientMutation::verify_update(
                    crdts,
                    schema,
                    doc,
                    patch,
                    snapshot,
                    signed.evidence(),
                )
                .unwrap();
                let request = query::QueryRequest::new("mutation($id:ID!, $input:RecordMutationInputArg!) { update_Record(docID:$id,input:$input) {_docID} }")
                    .with_variables(json!({ "id": doc, "input": patch }))
                    .with_identity(Some(user.clone()));
                let txn = on.node.begin_transaction(false).await.unwrap();
                let response = on
                    .node
                    .execute_verified_mutation_in_txn(request, &txn, &graph, credential)
                    .await;
                (txn, response)
            }
        };

        // An update sends the provider no command for a delegation bound to
        // one request to name, and nothing lets the node act without one.
        for credential in [SignerCredential::None, on.registration(&doc)] {
            let (txn, response) = update(credential).await;
            assert_eq!(
                response.errors.first().and_then(|error| error.code()),
                Some(WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE),
                "{:?}",
                response.errors
            );
            assert!(on.node.commit_transaction(&txn).await.is_err());
            assert_eq!(on.records().await, ["before"]);
        }

        let (txn, response) = update(on.delegation()).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        on.node.commit_transaction(&txn).await.unwrap();
        assert_eq!(on.records().await, ["after"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}
