use std::sync::Arc;
use std::time::{Duration, Instant};

use acp::{
    DocumentACP as _, OwedState, PendingRegistration, PendingRegistrations, RemotePolicies as _,
};
use defra_core::RequestContext;
use defra_node::{DocumentAcpProvider, Node, QueryResponse};
use identity::Did;
use query::QueryRequest;
use vera::vera_rs::{caller_delegation, DelegationSigner, Ed25519Signer};

#[allow(dead_code)]
#[path = "../../../../vera/embedded.rs"]
mod embedded;

const POLICY: &str = "
name: library
resources:
  - name: author
    permissions:
      - name: publish
        expr: publisher
      - name: read
        expr: publisher
      - name: update
        expr: publisher
      - name: delete
        expr: publisher
    relations:
      - name: publisher
        types:
          - actor
  - name: book
    permissions:
      - name: read
        expr: author->publish
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
";

fn created(response: QueryResponse, collection: &str) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn book(title: &str, edition: i32, author: &str) -> String {
    format!(
        r#"mutation {{
            add_Book(input: {{title: "{title}", edition: {edition}, author: "{author}"}}) {{ _docID }}
        }}"#
    )
}

fn publisher_request(publisher: &Did, token: Option<&str>, query: &str) -> QueryRequest {
    let context = RequestContext::default().with_identity(Some(publisher.to_string()));
    let context = match token {
        Some(token) => context.with_bearer(Some(publisher.to_string()), Some(token.to_owned())),
        None => context,
    };
    QueryRequest::new(query)
        .with_identity(Some(publisher.clone()))
        .with_context(context)
}

async fn stored(node: &Node, doc: &str) -> bool {
    let txn = node.database().new_txn(true).await.unwrap();
    db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), doc)
        .await
        .unwrap()
        .is_some()
}

async fn wait_for_purge(node: &Node, ids: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    for id in ids {
        while stored(node, id).await {
            assert!(
                Instant::now() < deadline,
                "refused create {id} still occupies its ID"
            );
            nuclei::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[nuclei::test]
#[serial_test::serial]
#[mantra_macros::req_test("defra.acp.create_gate.refused_cleanup")]
async fn a_final_refusal_releases_the_doc_id_and_unique_value() {
    edge::compat(async {
        let hosted = embedded::Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider: Arc<dyn vera::VeraProvider> = Arc::new(
            hosted
                .in_process(embedded::worker(root.path()))
                .await
                .unwrap(),
        );
        let builder = |provider| {
            Node::builder()
                .data_path(root.path().join("documents"))
                .with_document_acp_provider(DocumentAcpProvider::new(provider))
        };
        let node = builder(provider).build().await.unwrap();
        let vera = node.vera_acp().unwrap();
        let owner = Did::new(vera.self_did().unwrap()).unwrap();
        let policy = vera.add_policy(owner.as_str(), POLICY, None).await.unwrap();
        node.add_collection(&format!(
            r#"
            type Author @policy(id: "{policy}", resource: "author") {{ name: String }}
            type Book @governed(root: "library") @policy(id: "{policy}", resource: "book") {{
                title: String @index(unique: true)
                edition: Int
                author: Author @immutable @policy(relation: "author")
            }}
            "#
        ))
        .await
        .unwrap();

        let author = created(
            node.with_identity(owner.clone())
                .execute(r#"mutation { add_Author(input: {name: "Mike"}) { _docID } }"#)
                .await,
            "Author",
        );
        let author_gate = node.document_acp();
        author_gate
            .redrive_registration(&policy, "author", &author)
            .await
            .unwrap();
        author_gate
            .register_doc_object(&owner, &policy, "author", &author, &[])
            .await
            .unwrap();
        assert!(vera
            .is_doc_registered(&policy, "author", &author)
            .await
            .unwrap());
        let signer = Ed25519Signer::from_slice(&[17; 32]).unwrap();
        let publisher = Did::new(signer.did()).unwrap();
        let audience = vera.light_client_status().unwrap().delegation.unwrap();
        let token = caller_delegation(&signer, &audience, Duration::from_secs(600))
            .await
            .unwrap();
        author_gate
            .add_actor_relationship(
                &owner,
                &publisher,
                &policy,
                "author",
                &author,
                "publisher",
                &[],
            )
            .await
            .unwrap();

        let same = book("Same content", 1, &author);
        let unique = book("Same title", 1, &author);
        let txn = node
            .with_identity(publisher.clone())
            .with_bearer(token.clone())
            .new_txn(false)
            .await
            .unwrap();
        let same_id = created(txn.execute(&same).await, "Book");
        let unique_id = created(txn.execute(&unique).await, "Book");
        assert!(!stored(&node, &same_id).await);
        author_gate
            .delete_actor_relationship(
                &owner,
                &publisher,
                &policy,
                "author",
                &author,
                "publisher",
                &[],
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();

        wait_for_purge(&node, &[&same_id, &unique_id]).await;
        for id in [&same_id, &unique_id] {
            assert!(!vera.is_doc_registered(&policy, "book", id).await.unwrap());
        }
        author_gate
            .add_actor_relationship(
                &owner,
                &publisher,
                &policy,
                "author",
                &author,
                "publisher",
                &[],
            )
            .await
            .unwrap();
        // The publisher can pass the preflight without presenting the credential
        // needed to register the object. The committed row stays hidden and owed.
        let restart = book("After restart", 1, &author);
        let restart_id = created(
            node.execute_request(publisher_request(&publisher, None, &restart))
                .await,
            "Book",
        );
        assert!(stored(&node, &restart_id).await);
        drop(author_gate);
        node.shutdown().await.unwrap();
        drop(node);

        // Leave a final decision on disk with no running node to consume its wakeup.
        let refused: PendingRegistration = {
            let store = storage::RegolithStore::open(root.path().join("documents")).unwrap();
            let database = db::DB::open(store).await.unwrap();
            let txn = database.new_txn(false).await.unwrap();
            let view = txn.acpstore().unwrap();
            let registrations = PendingRegistrations::new(&view);
            let mut refused = registrations
                .get(&policy, "book", &restart_id)
                .await
                .unwrap()
                .unwrap();
            let reason = format!("not authorized to create document {restart_id}");
            refused.create_refused = true;
            refused.state = OwedState::Refused {
                reason: reason.clone(),
            };
            refused.last_error = Some(reason);
            assert!(refused.needs_create_cleanup());
            registrations.record(&refused).await.unwrap();
            drop(view);
            txn.commit().await.unwrap();
            database.close().await.unwrap();
            refused
        };

        let provider = hosted
            .in_process(embedded::worker(root.path()))
            .await
            .unwrap();
        let node = builder(Arc::new(provider)).build().await.unwrap();
        wait_for_purge(&node, &[&restart_id]).await;
        assert_eq!(
            created(
                node.execute_request(publisher_request(&publisher, Some(&token), &same))
                    .await,
                "Book",
            ),
            same_id
        );
        let replacement = created(
            node.execute_request(publisher_request(
                &publisher,
                Some(&token),
                &book("Same title", 2, &author),
            ))
            .await,
            "Book",
        );
        assert_ne!(replacement, unique_id);
        assert!(stored(&node, &same_id).await);
        assert!(stored(&node, &replacement).await);
        assert_eq!(
            created(
                node.execute_request(publisher_request(&publisher, Some(&token), &restart))
                    .await,
                "Book",
            ),
            restart_id
        );
        node.database().purge_refused_create(refused).await.unwrap();
        assert!(stored(&node, &restart_id).await);
        node.shutdown().await.unwrap();
        drop(node);
        hosted.stop().await;
    })
    .await;
}
