use crate::common::schema::parsed;
use std::sync::Arc;

use acp::DocumentACP;
use db::AutoCommitMutator;
use document::Document;
use query::{mutator::DocMutator, QueryExecutor, QueryRequest};

#[test]
fn prepared_upsert_keeps_hidden_matches_out_of_its_target_set() {
    workers::block_on(async {
    for has_visible_match in [false, true] {
        let fixture = super::kms::fixture().await;
        let protected_sdl = r#"type Protected @policy(id: "policy", resource: "protected") { left: String right: String }"#;
        let protected = parsed(protected_sdl);
        fixture
            .db
            .add_collection(&defra_core::RequestContext::ambient(), protected_sdl)
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
                    Document::from_json_str(
                        &format!(r#"{{"left":"match","right":"{value}"}}"#),
                        protected.clone(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            acp.register_doc_object(owner, "policy", "protected", &doc.doc_id.to_string(), &[])
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
})
    .unwrap()
}
