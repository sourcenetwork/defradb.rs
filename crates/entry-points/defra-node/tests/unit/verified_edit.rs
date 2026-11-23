//! Verified updates and deletes on a remote document ACP provider: admitted
//! on the signer's permission as the provider proves it, under any
//! credential form, with what this node still owes the provider deciding
//! first. Nothing is owed for the write itself.

use std::sync::atomic::Ordering;

use db::block::builder::{
    ClientDeleteSignatures, ClientUpdateSignatures, ClientUpdateSnapshot, VerifiedClientMutation,
};
use defra_core::signing::SigningConfig;
use identity::Did;
use query::executor::WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE;
use serde_json::json;

use super::staged_relationship_tests::{actor, request_token};
use super::verified_remote_tests::{user, Verified};
use super::{RelationshipChange, SignerCredential};

/// Somebody who signs verified writes with a key the node never holds.
struct Signer {
    did: String,
    signing: SigningConfig,
}

impl Signer {
    fn new() -> Self {
        let (did, signing) = user();
        Self { did, signing }
    }

    fn delegation(&self) -> SignerCredential {
        SignerCredential::Delegation(format!("delegation:{}", self.did))
    }
}

/// What a verified write does to the document.
#[derive(Clone, Copy)]
enum Edit {
    Update(&'static str),
    Delete,
}

/// One verified update or delete handed to the node, not yet committed.
struct Staged {
    txn: query::TransactionHandle,
    response: query::QueryResponse,
}

impl Verified {
    fn owner(&self) -> Signer {
        Signer {
            did: self.user.clone(),
            signing: self.signing.clone(),
        }
    }

    /// The document's history as its owner reads it now.
    async fn snapshot(&self, doc: &str) -> ClientUpdateSnapshot {
        let history = self
            .node
            .execute_request(
                query::QueryRequest::new("query($docID:[String!]!) { _commits(docID:$docID) { cid docID fieldName height delta collectionVersionId links { cid fieldName } heads { cid } signature { type identity value } } }")
                    .with_variables(json!({ "docID": [doc] }))
                    .with_identity(Some(Did::new(&self.user).unwrap())),
            )
            .await;
        assert!(!history.has_errors(), "{:?}", history.errors);
        ClientUpdateSnapshot::from_commits(
            doc,
            history.data.unwrap()["_commits"].as_array().unwrap(),
        )
        .unwrap()
    }

    /// `signer`'s verified `edit` of `doc`, signed on its current history,
    /// with `credential` beside it.
    async fn edit(
        &self,
        signer: &Signer,
        doc: &str,
        edit: Edit,
        credential: SignerCredential,
    ) -> Staged {
        let snapshot = self.snapshot(doc).await;
        self.edit_on(signer, doc, &snapshot, edit, credential).await
    }

    /// [`Self::edit`], signed on the history `snapshot`.
    async fn edit_on(
        &self,
        signer: &Signer,
        doc: &str,
        snapshot: &ClientUpdateSnapshot,
        edit: Edit,
        credential: SignerCredential,
    ) -> Staged {
        let schema = self.node.get_collection("Record").unwrap().unwrap();
        let crdts = storage::Crdts::default();
        let identity = Some(Did::new(&signer.did).unwrap());
        let (graph, request) = match edit {
            Edit::Update(name) => {
                let patch = json!({ "name": name });
                let signed = ClientUpdateSignatures::from_input(
                    &crdts,
                    &schema,
                    doc,
                    &patch,
                    snapshot,
                    &signer.signing,
                )
                .unwrap();
                let graph = VerifiedClientMutation::verify_update(
                    &crdts,
                    &schema,
                    doc,
                    &patch,
                    snapshot,
                    signed.evidence(),
                )
                .unwrap();
                let request = query::QueryRequest::new("mutation($id:ID!, $input:RecordMutationInputArg!) { update_Record(docID:$id,input:$input) {_docID} }")
                    .with_variables(json!({ "id": doc, "input": patch }));
                (graph, request)
            }
            Edit::Delete => {
                let signed =
                    ClientDeleteSignatures::from_snapshot(&schema, doc, snapshot, &signer.signing)
                        .unwrap();
                let graph = VerifiedClientMutation::verify_delete(
                    &schema,
                    doc,
                    snapshot,
                    signed.evidence(),
                )
                .unwrap();
                let request = query::QueryRequest::new(
                    "mutation($id:ID!) { delete_Record(docIDs:[$id]) {_docID} }",
                )
                .with_variables(json!({ "id": doc }));
                (graph, request)
            }
        };
        let txn = self.node.begin_transaction(false).await.unwrap();
        let response = self
            .node
            .execute_verified_mutation_in_txn(
                request.with_identity(identity),
                &txn,
                &graph,
                credential,
            )
            .await;
        Staged { txn, response }
    }

    /// The edit was admitted, and its commit sent the provider nothing and
    /// left nothing owed.
    async fn admitted(&self, staged: Staged) {
        let Staged { txn, response } = staged;
        assert!(!response.has_errors(), "{:?}", response.errors);
        self.node.commit_transaction(&txn).await.unwrap();
        assert!(self.remote.take_writes().is_empty());
        assert!(self.owed().await.is_empty());
    }

    /// The edit was refused before it wrote anything: its transaction is
    /// gone, the user still reads `records`, and nothing is owed or sent.
    /// `code` is the error code the refusal must carry, when it carries one.
    async fn edit_refused(&self, staged: Staged, code: Option<&str>, records: &[&str]) {
        self.edit_refused_unread(staged, code).await;
        assert_eq!(self.records().await, records);
    }

    /// [`Self::edit_refused`], without reading the records back.
    async fn edit_refused_unread(&self, staged: Staged, code: Option<&str>) {
        let Staged { txn, response } = staged;
        assert!(response.has_errors(), "an edit was admitted");
        if let Some(code) = code {
            assert_eq!(
                response.errors.first().and_then(|error| error.code()),
                Some(code),
                "{:?}",
                response.errors
            );
        }
        assert!(self.node.commit_transaction(&txn).await.is_err());
        assert!(self.remote.take_writes().is_empty());
        assert!(self.owed().await.is_empty());
    }

    /// A document the user created and the provider registered.
    async fn registered(&self, name: &str) -> String {
        let attempt = self.create(name, |_| self.delegation()).await;
        self.committed(attempt).await
    }
}

/// With no credential the signer's verified permission alone decides: the
/// owner and a writer update and delete, a reader (who may read but not
/// update) and a stranger are refused, and the node never holds a key.
#[test]
fn a_verified_update_or_delete_is_admitted_on_the_signers_permission() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let owner = on.owner();
        let (writer, reader, stranger) = (Signer::new(), Signer::new(), Signer::new());
        let doc = on.registered("first").await;
        on.remote.finalize(&doc, "writer", &writer.did, true);
        on.remote.finalize(&doc, "reader", &reader.did, true);

        on.admitted(
            on.edit(
                &owner,
                &doc,
                Edit::Update("by owner"),
                SignerCredential::None,
            )
            .await,
        )
        .await;
        assert_eq!(on.records().await, ["by owner"]);
        on.admitted(
            on.edit(
                &writer,
                &doc,
                Edit::Update("by writer"),
                SignerCredential::None,
            )
            .await,
        )
        .await;
        assert_eq!(on.records().await, ["by writer"]);

        on.edit_refused(
            on.edit(
                &reader,
                &doc,
                Edit::Update("by reader"),
                SignerCredential::None,
            )
            .await,
            None,
            &["by writer"],
        )
        .await;
        on.edit_refused(
            on.edit(
                &stranger,
                &doc,
                Edit::Update("by stranger"),
                SignerCredential::None,
            )
            .await,
            None,
            &["by writer"],
        )
        .await;
        on.edit_refused(
            on.edit(&reader, &doc, Edit::Delete, SignerCredential::None)
                .await,
            None,
            &["by writer"],
        )
        .await;

        on.admitted(
            on.edit(&writer, &doc, Edit::Delete, SignerCredential::None)
                .await,
        )
        .await;
        assert!(on.records().await.is_empty());
        for signer in [&owner, &writer, &reader, &stranger] {
            assert!(!on.remote.keys.contains_key(&signer.did));
        }
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// Every credential form is asked the same question about the permission: a
/// delegation proves who asks and the permission still decides; a key the
/// node holds decides nothing either. A delegation bound to one request is
/// refused for an update or a delete, key or no key, and a delegation that
/// is not the signer's is refused as before.
#[test]
fn every_credential_form_is_decided_by_the_permission() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let (writer, stranger) = (Signer::new(), Signer::new());
        let doc = on.registered("first").await;
        on.remote.finalize(&doc, "writer", &writer.did, true);
        on.remote.finalize(&doc, "reader", &stranger.did, true);

        // The stranger, who may read the document, with its own delegation
        // and with a key the node holds for it: who asks is proven, the
        // permission to update is not there.
        on.edit_refused(
            on.edit(
                &stranger,
                &doc,
                Edit::Update("delegated"),
                stranger.delegation(),
            )
            .await,
            None,
            &["first"],
        )
        .await;
        on.remote.keys.insert(stranger.did.clone(), ());
        on.edit_refused(
            on.edit(
                &stranger,
                &doc,
                Edit::Update("keyed"),
                SignerCredential::None,
            )
            .await,
            None,
            &["first"],
        )
        .await;

        // A delegation somebody else signed, and a token that is no
        // delegation to this node: refused for want of authority, though
        // the signer may update.
        for credential in [
            stranger.delegation(),
            SignerCredential::Delegation("an.identity.token".into()),
        ] {
            on.edit_refused(
                on.edit(&writer, &doc, Edit::Update("foreign"), credential)
                    .await,
                Some(WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE),
                &["first"],
            )
            .await;
        }
        // Bound to one request: there is no command to name, even with a key.
        on.remote.keys.insert(writer.did.clone(), ());
        for named in [format!("register {doc}"), format!("update {doc}")] {
            on.edit_refused(
                on.edit(
                    &writer,
                    &doc,
                    Edit::Update("bound"),
                    SignerCredential::RequestBound(request_token(&writer.did, &named)),
                )
                .await,
                Some(WRITE_AUTHORITY_UNAVAILABLE_ERROR_CODE),
                &["first"],
            )
            .await;
        }
        on.remote.keys.remove(&writer.did);

        on.admitted(
            on.edit(
                &writer,
                &doc,
                Edit::Update("delegated"),
                writer.delegation(),
            )
            .await,
        )
        .await;
        assert_eq!(on.records().await, ["delegated"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A revocation the provider finalized before admission refuses the next
/// update. One finalized between admission and commit does not undo the
/// admitted write: it commits.
#[test]
fn a_revocation_finalized_before_admission_refuses_the_update() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let writer = Signer::new();
        let doc = on.registered("first").await;
        // The writer keeps reading after the revocation: what refuses it is
        // the permission to update.
        on.remote.finalize(&doc, "reader", &writer.did, true);
        on.remote.finalize(&doc, "writer", &writer.did, true);

        let staged = on
            .edit(
                &writer,
                &doc,
                Edit::Update("admitted"),
                SignerCredential::None,
            )
            .await;
        assert!(
            !staged.response.has_errors(),
            "{:?}",
            staged.response.errors
        );
        on.remote.finalize(&doc, "writer", &writer.did, false);
        on.admitted(staged).await;
        assert_eq!(on.records().await, ["admitted"]);

        on.edit_refused(
            on.edit(
                &writer,
                &doc,
                Edit::Update("revoked"),
                SignerCredential::None,
            )
            .await,
            None,
            &["admitted"],
        )
        .await;
        on.edit_refused(
            on.edit(&writer, &doc, Edit::Delete, writer.delegation())
                .await,
            None,
            &["admitted"],
        )
        .await;
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A provider that cannot be reached, or whose newest verified state is past
/// the freshness limit, admits no update: the write fails closed.
#[test]
fn an_update_fails_closed_without_fresh_evidence() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let owner = on.owner();
        let doc = on.registered("first").await;

        for flag in [&on.remote.down, &on.remote.stale] {
            // The user signs on the history it read while the provider answered.
            let snapshot = on.snapshot(&doc).await;
            flag.store(true, Ordering::SeqCst);
            on.edit_refused_unread(
                on.edit_on(
                    &owner,
                    &doc,
                    &snapshot,
                    Edit::Update("blind"),
                    SignerCredential::None,
                )
                .await,
                None,
            )
            .await;
            flag.store(false, Ordering::SeqCst);
            assert_eq!(on.records().await, ["first"]);
        }
        on.admitted(
            on.edit(&owner, &doc, Edit::Update("seen"), SignerCredential::None)
                .await,
        )
        .await;
        assert_eq!(on.records().await, ["seen"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// What this node still owes the provider decides before the provider: a
/// registration still owed lets only the document's creator update it, even
/// while the provider cannot be reached, and a revocation still owed refuses
/// the subject it names whatever the provider holds.
#[test]
fn what_the_node_owes_decides_before_the_provider() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let on = Verified::new().await;
        let owner = on.owner();
        let writer = Signer::new();

        // The registration cannot be sent at the commit, and stays owed.
        let attempt = on.create("owed", |_| on.delegation()).await;
        assert!(
            !attempt.response.has_errors(),
            "{:?}",
            attempt.response.errors
        );
        let unregistered = attempt.doc.clone();
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&attempt.txn).await.unwrap();
        on.remote
            .finalize(&unregistered, "writer", &writer.did, true);
        let staged = on
            .edit(
                &writer,
                &unregistered,
                Edit::Update("not yours"),
                SignerCredential::None,
            )
            .await;
        assert!(staged.response.has_errors(), "an edit was admitted");
        assert!(on.node.commit_transaction(&staged.txn).await.is_err());
        let staged = on
            .edit(
                &owner,
                &unregistered,
                Edit::Update("still mine"),
                SignerCredential::None,
            )
            .await;
        assert!(
            !staged.response.has_errors(),
            "{:?}",
            staged.response.errors
        );
        on.node.commit_transaction(&staged.txn).await.unwrap();
        on.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(on.records().await, ["still mine"]);
        assert!(on.remote.owners.get(&unregistered).is_none());

        // A revocation of the writer committed while the provider was down
        // is owed, and denies the writer every permission on the document.
        let doc = on.registered("shared").await;
        on.remote.finalize(&doc, "writer", &writer.did, true);
        let txn = on.node.begin_transaction(false).await.unwrap();
        let revoking = actor(&on.user).with_bearer(
            Some(on.user.clone()),
            Some(format!("delegation:{}", on.user)),
        );
        let staged = on
            .node
            .delete_dac_actor_relationship_change_for_in_txn(
                &txn,
                &revoking,
                "Record",
                &doc,
                "reader",
                &writer.did,
            )
            .await;
        assert!(
            matches!(staged, Ok(RelationshipChange::Staged { .. })),
            "{staged:?}"
        );
        // The provider takes no write on the document from the commit on,
        // so the revocation stays owed while the node's drainer runs.
        on.remote.cut_off(&doc, true);
        on.remote.down.store(true, Ordering::SeqCst);
        on.node.commit_transaction(&txn).await.unwrap();
        on.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(on.owed().await.len(), 1);
        let staged = on
            .edit(
                &writer,
                &doc,
                Edit::Update("revoked"),
                SignerCredential::None,
            )
            .await;
        assert!(staged.response.has_errors(), "an edit was admitted");
        assert!(on.node.commit_transaction(&staged.txn).await.is_err());
        let mut records = on.records().await;
        records.sort();
        assert_eq!(records, ["shared", "still mine"]);
        on.node.shutdown().await.unwrap();
    })
    .unwrap();
}
