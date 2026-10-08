use std::sync::Arc;

use acp::DocumentACP;
use db::AutoCommitMutator;
use document::Document;
use query::{mutator::DocMutator, QueryExecutor, QueryRequest};
use schema::{CollectionVersion, FieldDescription, FieldKind, PolicyDescription};

#[tokio::test]
async fn prepared_upsert_keeps_hidden_matches_out_of_its_target_set() {
    for has_visible_match in [false, true] {
        let fixture = super::kms::fixture().await;
        fixture
            .db
            .create_collection(
                CollectionVersion::new(
                    "Protected",
                    "protected-v1",
                    "protected",
                    vec![
                        FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                        FieldDescription::new("2", "left", FieldKind::string()),
                        FieldDescription::new("3", "right", FieldKind::string()),
                    ],
                )
                .with_policy(PolicyDescription::new("policy", "protected")),
            )
            .await
            .unwrap();
        let acp = Arc::new(acp::LocalDocumentACP::new(Arc::new(
            acp::MemoryAcpStore::new(),
        )));
        let alice = identity::Did::new("did:key:z6Mkalice").unwrap();
        let bob = identity::Did::new("did:key:z6Mkbob").unwrap();
        let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
        for (owner, value) in [(&bob, "hidden"), (&alice, "visible")] {
            if value == "visible" && !has_visible_match {
                continue;
            }
            let doc = mutator
                .create(
                    "Protected",
                    Document::from_json_str(&format!(r#"{{"left":"match","right":"{value}"}}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            acp.register_doc_object(owner, "policy", "protected", &doc.doc_id.to_string())
                .await
                .unwrap();
        }
        let runner = query::QueryRunner::with_provider(
            db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
            db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
        )
        .with_mutator(Arc::new(mutator))
        .with_acp(acp);
        let mut request = QueryRequest::new(
            r#"mutation {
            upsert_Protected(filter: {left: {_eq: "match"}}, add: {left: "created"}, update: {right: "updated"}, encryptFields: [right]) { left right }
        }"#,
        );
        request.identity = Some(alice);
        let prepared = runner.prepare_request(&request).await.unwrap();
        let result = runner.execute_prepared(request, prepared).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        let data = result.data.unwrap();
        if has_visible_match {
            assert_eq!(data["upsert_Protected"][0]["right"], "updated");
        } else {
            assert_eq!(data["upsert_Protected"][0]["left"], "created");
        }
    }
}
