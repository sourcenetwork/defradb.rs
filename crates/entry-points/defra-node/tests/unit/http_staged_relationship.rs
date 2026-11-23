//! Relationship changes a client makes over HTTP in its own transaction on a
//! remote document ACP provider: staged in that transaction under the
//! delegation its verified credential is, checked before the commit, and
//! sent to the provider after it (#3080).

use std::sync::Arc;
use std::time::Duration;

use identity::Identity as _;
use serde_json::{json, Value};

use crate::staged_relationship_tests::{
    eventually, node_identity, provision, Remote, OWNER, READER,
};
use crate::{DocumentAcpProvider, Node};

/// A caller whose key the node does not hold.
const CALLER: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
/// An actor with no authority over the caller's documents.
const STRANGER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// A node on `remote` serving HTTP on loopback, idle transactions ended
/// after `idle`.
struct Served {
    node: Node,
    base: String,
    client: net::http::Client,
}

impl Served {
    async fn start(remote: &Arc<Remote>, idle: Option<Duration>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut builder = Node::builder()
            .with_node_identity_did(node_identity())
            .with_document_acp_provider(DocumentAcpProvider::new(remote.clone()))
            .with_http(crate::HttpConfig::with_listener(listener).unwrap());
        if let Some(idle) = idle {
            builder = builder.with_transaction_idle_timeout(idle, Duration::from_millis(100));
        }
        let node = builder.build().await.unwrap();
        provision(&node).await;
        let client = net::http::Client::builder(edge::testing::edge())
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        Self {
            node,
            base: format!("http://{address}/api/v0"),
            client,
        }
    }

    /// The stub provider's delegation of `did`, as a bearer.
    fn delegation(did: &str) -> String {
        format!("Bearer delegation:{did}")
    }

    async fn begin(&self, auth: &str) -> String {
        let begun = self
            .client
            .post(format!("{}/tx", self.base))
            .header("Authorization", auth)
            .send()
            .await
            .unwrap();
        assert!(begun.status().is_success(), "{}", begun.status());
        let body: Value = begun.json().await.unwrap();
        body["id"].as_str().unwrap().to_string()
    }

    async fn finish(&self, auth: &str, tx: &str, commit: bool) -> u16 {
        let url = format!("{}/tx/{tx}", self.base);
        let request = if commit {
            self.client.post(url)
        } else {
            self.client.delete(url)
        };
        request
            .header("Authorization", auth)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    /// A document `auth`'s caller creates in `tx`.
    async fn create(&self, auth: &str, tx: &str) -> String {
        let response = self
            .client
            .post(format!("{}/graphql", self.base))
            .header("Authorization", auth)
            .header("x-defradb-tx", tx)
            .json(&json!({
                "query": "mutation { add_Record(input: {name: \"shared\"}) { _docID } }"
            }))
            .send()
            .await
            .unwrap();
        let body: Value = response.json().await.unwrap();
        body["data"]["add_Record"][0]["_docID"]
            .as_str()
            .unwrap_or_else(|| panic!("no document: {body}"))
            .to_owned()
    }

    /// A grant (or, with `delete`, a revocation) of `reader` on `doc`, in
    /// `tx` when there is one: its status and body.
    async fn relationship(
        &self,
        auth: &str,
        tx: Option<&str>,
        doc: &str,
        delete: bool,
    ) -> (u16, Value) {
        let url = format!("{}/acp/document/relationship", self.base);
        let mut request = if delete {
            self.client.delete(url)
        } else {
            self.client.post(url)
        };
        request = request.header("Authorization", auth);
        if let Some(tx) = tx {
            request = request.header("x-defradb-tx", tx);
        }
        let response = request
            .json(&json!({
                "collection": "Record",
                "doc_id": doc,
                "relation": "reader",
                "target_actor": READER,
            }))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    /// An identity token `caller` signs for this node, as a bearer: a sign-in,
    /// not a delegation.
    fn identity_token(&self, caller: &identity::RawIdentity) -> String {
        let address = self
            .base
            .trim_start_matches("http://")
            .trim_end_matches("/api/v0");
        let token = identity::new_token(
            caller,
            Duration::from_secs(60),
            Some(address.to_owned()),
            None,
        )
        .unwrap();
        format!("Bearer {}", String::from_utf8(token).unwrap())
    }

    /// Everything `did` still owes the provider, as the node reports it.
    async fn owed_by(&self, did: &str) -> usize {
        self.node
            .owed_acp_as(&crate::staged_relationship_tests::actor(did), did)
            .await
            .unwrap()
            .len()
    }
}

/// A client creates a document and grants a reader in one transaction over
/// HTTP. The grant is staged in it, under the caller's delegation and
/// checked as the creator's, the answer says so, and after the commit the
/// provider is sent the registration and then the grant.
#[test]
fn a_client_transaction_creates_and_grants_and_the_grant_lands_after_the_commit() {
    workers::block_on(async {
        let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let served = Served::start(&remote, None).await;
        let auth = Served::delegation(CALLER);

        let tx = served.begin(&auth).await;
        let doc = served.create(&auth, &tx).await;
        let (status, granted) = served.relationship(&auth, Some(&tx), &doc, false).await;
        assert_eq!(status, 200, "{granted}");
        assert_eq!(granted["ExistedAlready"], false, "{granted}");
        assert_eq!(granted["Staged"]["AuthorityChecked"], true, "{granted}");
        assert!(granted["Staged"]["Seq"].is_u64(), "{granted}");
        assert!(
            !remote.holds(&doc, READER),
            "nothing is sent before the commit"
        );

        assert_eq!(served.finish(&auth, &tx, true).await, 200);
        eventually("the grant did not reach the provider", || async {
            remote.holds(&doc, READER)
        })
        .await;
        assert_eq!(
            remote.owners.get(&doc).as_deref(),
            Some(CALLER),
            "registered to its creator first"
        );
        assert_eq!(served.owed_by(CALLER).await, 0);

        // Outside a transaction the route answers as Go's does.
        let (status, revoked) = served.relationship(&auth, None, &doc, true).await;
        assert_eq!(status, 200, "{revoked}");
        assert_eq!(revoked, json!({"RecordFound": true}));
        served.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A change its caller has no authority for is refused before the commit,
/// and so is one by a caller whose credential is no delegation and whose key
/// the node does not hold: nothing is staged, nothing is owed and nothing
/// reaches the provider.
#[test]
fn a_change_without_authority_is_refused_before_the_commit_and_owes_nothing() {
    workers::block_on(async {
        let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let served = Served::start(&remote, None).await;
        let owner = Served::delegation(CALLER);
        let tx = served.begin(&owner).await;
        let doc = served.create(&owner, &tx).await;
        assert_eq!(served.finish(&owner, &tx, true).await, 200);
        eventually("the registration did not reach the provider", || async {
            remote.owners.get(&doc).as_deref() == Some(CALLER)
        })
        .await;

        let stranger = Served::delegation(STRANGER);
        for delete in [false, true] {
            let tx = served.begin(&stranger).await;
            let (status, refused) = served
                .relationship(&stranger, Some(&tx), &doc, delete)
                .await;
            assert_eq!(status, 401, "{refused}");
            assert_eq!(served.finish(&stranger, &tx, true).await, 200);
        }
        assert_eq!(served.owed_by(STRANGER).await, 0);

        // Signed in with an identity token for a DID whose key the node does
        // not hold: the node could not send the change once the request is
        // gone, so it is not staged.
        let caller =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let signed_in = served.identity_token(&caller);
        let tx = served.begin(&signed_in).await;
        let (status, refused) = served
            .relationship(&signed_in, Some(&tx), &doc, false)
            .await;
        assert_eq!(status, 401, "{refused}");
        assert!(
            refused
                .to_string()
                .contains("no authority to submit ACP changes"),
            "{refused}"
        );
        assert_eq!(served.finish(&signed_in, &tx, false).await, 200);
        assert_eq!(served.owed_by(caller.did().unwrap().as_str()).await, 0);

        assert!(remote.take_writes().is_empty());
        served.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A caller signed in with an identity token for a DID whose key the node
/// holds has its change staged under the node's key, as the route without a
/// transaction sends as that DID, and checked at staging all the same: the
/// owner's grant is staged and lands after the commit, and another such
/// caller, who owns nothing, is refused and owes nothing.
#[test]
fn an_identity_token_for_a_did_whose_key_the_node_holds_stages_under_the_node_key() {
    workers::block_on(async {
        let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let served = Served::start(&remote, None).await;
        let held = |remote: &Remote| {
            let caller =
                identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap())
                    .unwrap();
            let did = caller.did().unwrap().to_string();
            remote.keys.insert(did.clone(), ());
            (caller, did)
        };
        let (owner, owner_did) = held(&remote);
        let auth = served.identity_token(&owner);
        let tx = served.begin(&auth).await;
        let doc = served.create(&auth, &tx).await;
        assert_eq!(served.finish(&auth, &tx, true).await, 200);
        eventually("the registration did not reach the provider", || async {
            remote.owners.get(&doc).as_deref() == Some(owner_did.as_str())
        })
        .await;
        remote.take_writes();

        let (stranger, stranger_did) = held(&remote);
        let refused_auth = served.identity_token(&stranger);
        let tx = served.begin(&refused_auth).await;
        let (status, refused) = served
            .relationship(&refused_auth, Some(&tx), &doc, false)
            .await;
        assert_eq!(status, 401, "{refused}");
        assert_eq!(served.finish(&refused_auth, &tx, true).await, 200);
        assert_eq!(served.owed_by(&stranger_did).await, 0);

        // Cut off, the provider cannot take the grant at the commit, so the
        // node reports it owed with what it will be sent under.
        remote.cut_off(&doc, true);
        let tx = served.begin(&auth).await;
        let (status, granted) = served.relationship(&auth, Some(&tx), &doc, false).await;
        assert_eq!(status, 200, "{granted}");
        assert_eq!(granted["Staged"]["AuthorityChecked"], true, "{granted}");
        assert_eq!(served.finish(&auth, &tx, true).await, 200);
        let owed = served
            .node
            .owed_acp_as(
                &crate::staged_relationship_tests::actor(&owner_did),
                &owner_did,
            )
            .await
            .unwrap();
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert_eq!(owed[0].authority, Some(acp::OwedAuthorityKind::NodeKey));
        remote.cut_off(&doc, false);
        eventually("the grant did not reach the provider", || async {
            remote.holds(&doc, READER)
        })
        .await;
        assert_eq!(served.owed_by(&owner_did).await, 0);
        served.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A grant staged in a transaction the client discards, or abandons until
/// the node's idle sweep ends it, is gone with it: nothing is owed and
/// nothing reaches the provider.
#[test]
fn a_discarded_or_abandoned_transaction_leaves_nothing_owed() {
    workers::block_on(async {
        let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let served = Served::start(&remote, Some(Duration::from_secs(1))).await;
        let auth = Served::delegation(CALLER);
        let tx = served.begin(&auth).await;
        let doc = served.create(&auth, &tx).await;
        assert_eq!(served.finish(&auth, &tx, true).await, 200);
        eventually("the registration did not reach the provider", || async {
            remote.owners.get(&doc).as_deref() == Some(CALLER)
        })
        .await;
        remote.take_writes();

        let discarded = served.begin(&auth).await;
        let (status, staged) = served
            .relationship(&auth, Some(&discarded), &doc, false)
            .await;
        assert_eq!(status, 200, "{staged}");
        assert!(staged["Staged"].is_object(), "{staged}");
        assert_eq!(served.finish(&auth, &discarded, false).await, 200);

        let abandoned = served.begin(&auth).await;
        let (status, staged) = served
            .relationship(&auth, Some(&abandoned), &doc, true)
            .await;
        assert_eq!(status, 200, "{staged}");
        assert_eq!(staged["RecordFound"], true, "{staged}");
        // Untouched for longer than the idle limit, the sweep ends it; a
        // commit asked for afterwards finds no transaction.
        edge::testing::edge().sleep(Duration::from_secs(3)).await;
        let late = served.finish(&auth, &abandoned, true).await;
        assert_eq!(late, 404, "the idle sweep ended the transaction");

        assert_eq!(served.owed_by(CALLER).await, 0);
        let writes = remote.take_writes();
        assert!(writes.is_empty(), "{writes:?}");
        assert!(!remote.holds(&doc, READER));
        served.node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// The acting DID is the one the credential names: a stranger who also
/// names the owner in a header of the request changes nothing on the
/// owner's document, stages nothing and owes nothing.
#[test]
fn the_acting_identity_comes_from_the_credential_alone() {
    workers::block_on(async {
        let _serial = crate::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let served = Served::start(&remote, None).await;
        let doc = {
            let response = served
                .node
                .execute_request(
                    query::QueryRequest::new(
                        "mutation { add_Record(input: {name: \"owned\"}) { _docID } }",
                    )
                    .with_identity(Some(identity::Did::new(OWNER).unwrap())),
                )
                .await;
            assert!(!response.has_errors(), "{:?}", response.errors);
            response.data.unwrap()["add_Record"][0]["_docID"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let stranger = Served::delegation(STRANGER);
        let tx = served.begin(&stranger).await;
        let response = served
            .client
            .post(format!("{}/acp/document/relationship", served.base))
            .header("Authorization", stranger.as_str())
            .header("x-defradb-tx", tx.as_str())
            .header("x-defradb-identity", OWNER)
            .json(&json!({
                "collection": "Record",
                "doc_id": doc,
                "relation": "reader",
                "target_actor": READER,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 401);
        assert_eq!(served.finish(&stranger, &tx, true).await, 200);
        assert_eq!(served.owed_by(STRANGER).await, 0);
        assert_eq!(served.owed_by(OWNER).await, 0);
        assert!(remote.take_writes().is_empty());
        served.node.shutdown().await.unwrap();
    })
    .unwrap();
}
