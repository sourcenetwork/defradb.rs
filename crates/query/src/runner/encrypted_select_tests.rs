//! `encrypted_<Collection>` exists only with searchable encryption on (a wired
//! SE transport), matching Go, which generates the field only when an SE key is set.

use std::sync::Arc;

use async_trait::async_trait;
use schema::{CType, CollectionVersion, EncryptedIndexDescription, FieldDescription, FieldKind};
use serde_json::Value as JsonValue;

use crate::executor::{QueryExecutor, QueryRequest};
use crate::runner::{QueryRunner, SeQueryTransport};
use crate::test_utils::MockFetcher;

struct OneDoc;

#[async_trait]
impl SeQueryTransport for OneDoc {
    async fn query_doc_ids(
        &self,
        _: &str,
        _: Vec<(String, JsonValue)>,
    ) -> std::result::Result<Vec<String>, String> {
        Ok(vec!["bae-user".into()])
    }
}

fn users() -> CollectionVersion {
    let mut users = CollectionVersion::new(
        "User",
        "v1",
        "coll-user",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "age", FieldKind::int(), CType::LwwRegister),
        ],
    );
    users.encrypted_indexes = vec![EncryptedIndexDescription::new("age")];
    users
}

const QUERY: &str = r#"query { encrypted_User(filter: {age: {_eq: 21}}) { docIDs } }"#;

#[test]
fn encrypted_query_without_searchable_encryption_is_unknown_field() {
    workers::block_on(async {
        let runner = QueryRunner::new(MockFetcher::new(), vec![users()]);
        let response = runner.execute(QueryRequest::new(QUERY)).await;
        let messages: Vec<&str> = response.errors.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            ["Cannot query field \"encrypted_User\" on type \"Query\"."]
        );
    })
    .unwrap()
}

#[test]
fn encrypted_query_with_searchable_encryption_asks_the_transport() {
    workers::block_on(async {
        let runner =
            QueryRunner::new(MockFetcher::new(), vec![users()]).with_se_transport(Arc::new(OneDoc));
        let response = runner.execute(QueryRequest::new(QUERY)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(
            response.data,
            Some(serde_json::json!({ "encrypted_User": [{ "docIDs": ["bae-user"] }] }))
        );
    })
    .unwrap()
}
