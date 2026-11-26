//! A collection whose policy restricts who may add documents to it (the
//! `create` of its resource's `collection` section), on a node whose document
//! ACP is a Vera node in this process.
//!
//! A book names its author through `author: Author @immutable
//! @policy(relation: "author")`: the node derives `book:<id>#author@author:<authorID>`
//! and registers the book with it; Vera checks `create: author->publish` over
//! it and stores it with the owner. The stored relationship is observed
//! through `book.read: author->read`, which lets a reader of the author read
//! the book.

use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, DocumentPermission, Identity, OwedState, RemotePolicies as _};
use defra_core::RequestContext;
use defra_node::{Node, QueryRequest, QueryResponse};
use serde_json::Value;
use vera::vera_rs::{caller_delegation, DelegationSigner, Ed25519Signer};

use super::embedded::{self, Embedded};

const LIFETIME: Duration = Duration::from_secs(600);

const POLICY: &str = "\
name: library
resources:
  - name: author
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
      - name: publish
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
  - name: book
    permissions:
      - name: read
        expr: author->read
      - name: update
        expr: editor
      - name: delete
        expr: editor
    relations:
      - name: author
        types:
          - author
      - name: editor
        types:
          - actor
    collection:
      create: author->publish
  - name: artist
    permissions:
      - name: read
        expr: reader
      - name: update
        expr: reader
      - name: delete
        expr: reader
    relations:
      - name: reader
        types:
          - actor
";

fn schema(policy: &str) -> String {
    format!(
        r#"
        type Author @policy(id: "{policy}", resource: "author") {{
            name: String
        }}
        type Book @governed(root: "library") @policy(id: "{policy}", resource: "book") {{
            title: String
            author: Author @immutable @policy(relation: "author")
        }}
        type Artist @policy(id: "{policy}", resource: "artist") {{
            name: String
        }}
        type Note {{
            text: String
        }}
        "#
    )
}

/// A client that keeps its key and signs its requests with a delegation to
/// the node's Vera deployment.
struct User {
    did: identity::Did,
    token: String,
}

impl User {
    async fn new(seed: u8, audience: &acp::DelegationAudience) -> Self {
        let signer = Ed25519Signer::from_slice(&[seed; 32]).expect("key");
        let token = caller_delegation(&signer, audience, LIFETIME)
            .await
            .expect("delegation");
        Self {
            did: identity::Did::new(signer.did()).expect("DID"),
            token,
        }
    }

    fn request(&self, query: &str) -> QueryRequest {
        QueryRequest::new(query)
            .with_identity(Some(self.did.clone()))
            .with_context(
                RequestContext::default()
                    .with_identity(Some(self.did.to_string()))
                    .with_bearer(Some(self.did.to_string()), Some(self.token.clone())),
            )
    }
}

fn add(collection: &str, input: &str) -> String {
    format!("mutation {{ add_{collection}(input: {{{input}}}) {{ _docID }} }}")
}

fn created(response: QueryResponse, collection: &str) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .expect("document ID")
        .to_owned()
}

fn errors(response: &QueryResponse) -> String {
    response
        .errors
        .iter()
        .map(|error| format!("{error:?}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The `_docID`s of `collection` `response` lists.
fn ids(response: QueryResponse, collection: &str) -> Vec<String> {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")[collection]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["_docID"].as_str().expect("_docID").to_owned())
        .collect()
}

async fn reads(node: &Node, who: &identity::Did, policy: &str, resource: &str, doc: &str) -> bool {
    node.document_acp()
        .check_doc_access(
            &Identity::Authenticated(who.clone()),
            DocumentPermission::Read,
            policy,
            resource,
            doc,
        )
        .await
        .expect("access decision")
}

/// Wait until `who` may read `doc`, as the registration settles after the
/// commit; false when it never may within the deadline.
async fn eventually_reads(
    node: &Node,
    who: &identity::Did,
    policy: &str,
    resource: &str,
    doc: &str,
) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if reads(node, who, policy, resource, doc).await {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        nuclei::time::sleep(Duration::from_millis(200)).await;
    }
}

/// What the node still owes Vera for `actor`.
async fn owed(node: &Node, actor: &identity::Did) -> Vec<acp::OwedEntry> {
    node.owed_acp_as(
        &RequestContext::default().with_identity(Some(actor.to_string())),
        actor.as_str(),
    )
    .await
    .expect("owed entries")
}

#[nuclei::test]
#[serial_test::serial]
#[mantra_macros::req_test("defra.acp.create_gate.vera")]
async fn a_gated_collection_admits_creators_its_policy_lets_create() {
    edge::compat(async {
        let hosted = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider: Arc<dyn vera::VeraProvider> = Arc::new(
            hosted
                .in_process(embedded::worker(root.path()))
                .await
                .expect("provider"),
        );
        let node = Node::builder()
            .with_document_acp_provider(defra_node::DocumentAcpProvider::new(provider))
            .build()
            .await
            .expect("node");
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&RequestContext::ambient(), &schema(&policy))
            .await
            .expect("collections");
        let audience = vera
            .light_client_status()
            .expect("status")
            .delegation
            .expect("a vera.rs node names its delegation audience");
        let alice = User::new(1, &audience).await;
        let bob = User::new(2, &audience).await;
        let dave = User::new(3, &audience).await;
        let as_owner = |query: &str| QueryRequest::new(query).with_identity(Some(owner.clone()));

        // The author belongs to the node's actor; alice writes for it, dave
        // reads it.
        let author = created(
            node.execute_request(as_owner(&add("Author", r#"name: "Mike""#)))
                .await,
            "Author",
        );
        assert!(eventually_reads(&node, &owner, &policy, "author", &author).await);
        for (relation, who) in [("writer", &alice.did), ("reader", &dave.did)] {
            node.document_acp()
                .add_actor_relationship(&owner, who, &policy, "author", &author, relation, &[])
                .await
                .expect("relationship");
        }

        // Anyone adds an artist: its resource has no `create`.
        let artist = created(
            node.execute_request(bob.request(&add("Artist", r#"name: "Ana""#)))
                .await,
            "Artist",
        );
        assert!(eventually_reads(&node, &bob.did, &policy, "artist", &artist).await);

        // A collection without a policy is untouched, even anonymously.
        let note = node
            .execute_request(QueryRequest::new(add("Note", r#"text: "hi""#)))
            .await;
        created(note, "Note");

        // alice may publish for Mike: her book is registered with
        // `book:<id>#author@author:<Mike>`, which lets Mike's reader read it.
        let book_input = format!(r#"title: "Dune", author: "{author}""#);
        let book = created(
            node.execute_request(alice.request(&add("Book", &book_input)))
                .await,
            "Book",
        );
        assert!(
            eventually_reads(&node, &dave.did, &policy, "book", &book).await,
            "a reader of the author reads the book through the derived relationship: {:?}",
            owed(&node, &alice.did).await
        );
        assert!(!reads(&node, &bob.did, &policy, "book", &book).await);
        assert_eq!(
            vera.get_doc_owner(&policy, "book", &book)
                .await
                .expect("owner")
                .map(|owner| owner.to_string()),
            Some(alice.did.to_string())
        );
        assert_eq!(
            ids(
                node.execute_request(dave.request("query { Book { _docID } }"))
                    .await,
                "Book"
            ),
            vec![book.clone()]
        );

        // bob may not publish for Mike: the create commits and its
        // registration is refused after the commit (a refused hook does not
        // undo a durable commit); the book stays unregistered and hidden from
        // everyone, bob too, and the refusal is in what the node owes.
        let bobs_input = format!(r#"title: "Not Dune", author: "{author}""#);
        let refused = node
            .execute_request(bob.request(&add("Book", &bobs_input)))
            .await;
        let bobs_book = refused
            .data
            .as_ref()
            .and_then(|data| data["add_Book"][0]["_docID"].as_str())
            .map(str::to_owned);
        let refusal = owed(&node, &bob.did).await;
        let entry = refusal
            .iter()
            .find(|entry| entry.resource == "book")
            .unwrap_or_else(|| panic!("bob's book is still owed: {refusal:?}"));
        assert!(
            matches!(&entry.state, OwedState::Refused { reason } if reason.starts_with("not authorized to create document ")),
            "{entry:?}"
        );
        if let Some(bobs_book) = &bobs_book {
            assert_eq!(&entry.object, bobs_book);
        }
        let bobs_book = entry.object.clone();
        assert!(!vera
            .is_doc_registered(&policy, "book", &bobs_book)
            .await
            .expect("registration"));
        for who in [&bob, &alice, &dave] {
            assert!(!reads(&node, &who.did, &policy, "book", &bobs_book).await);
            let listed = ids(
                node.execute_request(who.request("query { Book { _docID } }"))
                    .await,
                "Book",
            );
            assert!(!listed.contains(&bobs_book), "{listed:?}");
        }
        // The refusal is final: a redrive sends nothing and keeps it.
        node.document_acp()
            .redrive_registration(&policy, "book", &bobs_book)
            .await
            .expect_err("a refused create stays refused");
        assert!(!vera
            .is_doc_registered(&policy, "book", &bobs_book)
            .await
            .expect("registration"));

        // Nobody can be checked without an identity: refused before
        // anything is written.
        let anonymous = node
            .execute_request(QueryRequest::new(add(
                "Book",
                &format!(r#"title: "Anonymous", author: "{author}""#),
            )))
            .await;
        assert!(
            errors(&anonymous).contains("without an identity"),
            "{:?} {}",
            anonymous.data,
            errors(&anonymous)
        );
        let listed: Value = node
            .execute_request(as_owner("query { Book { title } }"))
            .await
            .data
            .expect("data");
        assert!(!listed.to_string().contains("Anonymous"), "{listed}");

        node.shutdown().await.expect("shutdown");
        drop(node);
        hosted.stop().await;
    })
    .await;
}
