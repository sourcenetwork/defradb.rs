//! `_jsonConflicts` answers only for a document the app's read validator lets
//! the caller read: a denied document yields no rows and is never fetched.

use std::sync::Arc;

use async_trait::async_trait;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};

use crate::access_hooks::{ReadRequest, ReadValidator};
use crate::executor::{QueryExecutor, QueryRequest};
use crate::runner::QueryRunner;
use crate::test_utils::MockFetcher;

struct DenyAll;

#[async_trait]
impl ReadValidator for DenyAll {
    fn governs(&self, _: &CollectionVersion) -> bool {
        true
    }

    async fn may_read(&self, _: &ReadRequest<'_>) -> std::result::Result<bool, String> {
        Ok(false)
    }
}

fn notes() -> CollectionVersion {
    CollectionVersion::new(
        "Note",
        "v1",
        "coll-note",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "meta", FieldKind::json(), CType::Json),
        ],
    )
}

const QUERY: &str =
    r#"query { _jsonConflicts(collection: "Note", docID: "bae-note", field: "meta") { path } }"#;

#[test]
fn a_document_the_read_validator_denies_has_no_conflicts() {
    workers::block_on(async {
        // The mock fetcher cannot read conflicts, so a fetch answers with an error.
        let open = QueryRunner::new(MockFetcher::new(), vec![notes()]);
        assert!(open.execute(QueryRequest::new(QUERY)).await.has_errors());

        let denied = QueryRunner::new(MockFetcher::new(), vec![notes()])
            .with_read_validator(Arc::new(DenyAll));
        let response = denied.execute(QueryRequest::new(QUERY)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(
            response.data,
            Some(serde_json::json!({ "_jsonConflicts": [] }))
        );
    })
    .unwrap()
}
