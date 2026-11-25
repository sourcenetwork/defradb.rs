//! A request's headless decision goes stale when a write lands between the
//! request's KMS preparation and its build: the attempt decides on its own
//! heads, and a document key the preparation did not mint is prepared at the
//! edge before the next attempt.

use cid::Cid;
use db::block::builder::DocHeadsSnapshot;
use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use defra_core::block::Block;
use defra_core::encryption::{set_encryption_config, EncryptionConfig};
use document::{DocID, Document};
use query::mutator::DocMutator;
use query::prepared::PreparedInputKind;
use query::{QueryExecutor, QueryRequest, QueryResponse, QueryRunner};
use std::sync::Arc;

type Runner = QueryRunner<
    LensedAutoCommitFetcher<storage::RegolithStore>,
    DbTransactionRegistry<storage::RegolithStore>,
>;

fn runner(db: &Arc<db::DB<storage::RegolithStore>>) -> Runner {
    QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(db)),
        DbCollectionProvider::new_arc(Arc::clone(db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(db))))
}

fn data(response: QueryResponse) -> serde_json::Value {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("a response without errors has data")
}

/// Whether a KMS input asks for a fresh key rather than an existing one.
fn generates(input: &query::prepared::PreparedInputRequest) -> bool {
    let request: serde_json::Value = serde_json::from_slice(input.exact_request()).unwrap();
    request["operation"] == "generate"
}

/// Whether each head of `field` links an `Encryption` block, read from a
/// fresh transaction.
async fn heads_encrypted(
    db: &db::DB<storage::RegolithStore>,
    doc_id: &str,
    field: &str,
) -> Vec<bool> {
    let collection = db.require_collection("Secrets").unwrap().resolved_root_id();
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let short_id = db::docid::map::get_doc_short_id(&systemstore, collection, doc_id)
        .await
        .unwrap()
        .expect("the document is mapped");
    let headstore = txn.headstore().unwrap();
    let blockstore = txn.blockstore().unwrap();
    let snapshot = DocHeadsSnapshot::load(&headstore, short_id).await.unwrap();
    let heads: Vec<Cid> = snapshot.field_heads(field);
    let mut encrypted = Vec::new();
    for head in heads {
        let bytes = blockstore
            .get(&head.to_bytes())
            .await
            .unwrap()
            .expect("the head block is stored");
        encrypted.push(Block::from_dag_cbor(&bytes).unwrap().encryption.is_some());
    }
    encrypted
}

/// Write A of a request prepares `left` as plaintext, and write B prepares the
/// headless `right` as plaintext under rule (c), since A leaves `left`
/// plaintext. A write then encrypts `left` before the request is built. A's
/// build follows `left`'s heads and writes it encrypted; B, decided again on
/// the attempt's heads, finds every field encrypted and takes a document key
/// the preparation never minted, so `right` is encrypted too.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn a_stale_headless_decision_is_prepared_again() {
    workers::block_on(async {
        let fixture = super::kms::fixture().await;
        let runner = runner(&fixture.db);
        let created = data(
            runner
                .execute(QueryRequest::new(
                    r#"mutation { add_Secrets(input: {left: "l"}) { _docID } }"#,
                ))
                .await,
        );
        let doc_id = created["add_Secrets"][0]["_docID"]
            .as_str()
            .expect("the create returns its document ID")
            .to_string();
        assert_eq!(heads_encrypted(&fixture.db, &doc_id, "left").await, [false]);
        assert!(heads_encrypted(&fixture.db, &doc_id, "right")
            .await
            .is_empty());

        let request = QueryRequest::new(format!(
            r#"mutation {{
                a: update_Secrets(docID: "{doc_id}", input: {{left: "l2"}}) {{ _docID }}
                b: update_Secrets(docID: "{doc_id}", input: {{right: "r"}}) {{ _docID }}
            }}"#
        ));
        let mut prepared = runner.prepare_request(&request).await.unwrap();

        // The write that lands between preparation and build: `left` gets an
        // encrypted head.
        set_encryption_config(Some(EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["left".to_string()],
        }));
        let mut doc = Document::new(super::kms::secrets_collection());
        doc.set("left", "encrypted").unwrap();
        doc.set_id(DocID::from_string(&doc_id).unwrap());
        let encrypted = AutoCommitMutator::new(Arc::clone(&fixture.db))
            .update("Secrets", doc, ["left".to_string()].into_iter().collect())
            .await;
        set_encryption_config(None);
        encrypted.unwrap();
        assert_eq!(heads_encrypted(&fixture.db, &doc_id, "left").await, [true]);

        let mut key_inputs = 0;
        let response = loop {
            let response = runner
                .execute_prepared(request.clone(), prepared.clone())
                .await;
            let Some(input) = response.retry_input().cloned() else {
                break response;
            };
            if input.kind() == PreparedInputKind::KmsKey && generates(&input) {
                key_inputs += 1;
            }
            prepared = runner
                .resolve_retry_input(&request, prepared.clone(), &input)
                .await
                .unwrap();
        };
        data(response);
        assert_eq!(
            key_inputs, 1,
            "the stale decision ends one attempt for the document key"
        );

        assert_eq!(heads_encrypted(&fixture.db, &doc_id, "left").await, [true]);
        assert_eq!(
            heads_encrypted(&fixture.db, &doc_id, "right").await,
            [true],
            "the headless field of a wholly encrypted document is encrypted"
        );
        let read = data(
            runner
                .execute(QueryRequest::new("{ Secrets { left right } }"))
                .await,
        );
        assert_eq!(read["Secrets"][0]["left"], "l2");
        assert_eq!(read["Secrets"][0]["right"], "r");
    })
    .unwrap()
}
