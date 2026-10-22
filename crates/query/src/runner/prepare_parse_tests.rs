//! A request is parsed once: a prepared mutation runs from the parse its
//! preparation made, and a request that cannot be a mutation is parsed only
//! by its execution, with the same answer when it does not parse.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;
use document::{DocID, Document};
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind};

use crate::error::Result;
use crate::executor::{QueryExecutor, QueryRequest};
use crate::limits::QueryLimits;
use crate::mutator::{CreateResult, DeleteResult, DocMutator, UpdateResult};
use crate::query_parse::{parse_request_with_limits, ParsedOperation};
use crate::runner::QueryRunner;
use crate::test_utils::MockFetcher;

/// Creates every document it is given under a fresh ID.
#[derive(Default)]
struct Creates;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocMutator for Creates {
    async fn create(&self, collection_name: &str, doc: Document) -> Result<CreateResult> {
        self.create_as(&RequestContext::ambient(), collection_name, doc)
            .await
    }

    async fn create_as(
        &self,
        _context: &RequestContext,
        _collection_name: &str,
        mut doc: Document,
    ) -> Result<CreateResult> {
        let id = DocID::new_v0_from_seed("prepare-parse-doc");
        doc.set_id(id.clone());
        Ok(CreateResult::new(id, doc))
    }

    async fn update(
        &self,
        _collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> Result<UpdateResult> {
        Ok(UpdateResult::new(doc, modified_fields.len()))
    }

    async fn delete(&self, _collection_name: &str, doc_id: &DocID) -> Result<DeleteResult> {
        Ok(DeleteResult::new(doc_id.clone(), true))
    }

    async fn exists(&self, _collection_name: &str, _doc_id: &DocID) -> Result<bool> {
        Ok(false)
    }

    async fn get_for_update(
        &self,
        _collection_name: &str,
        _doc_id: &DocID,
    ) -> Result<Option<Document>> {
        Ok(None)
    }
}

fn users() -> CollectionVersion {
    CollectionVersion::new(
        "User",
        "v1",
        "coll-user",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

fn runner() -> QueryRunner<MockFetcher> {
    QueryRunner::new(MockFetcher::new(), vec![users()]).with_mutator(Arc::new(Creates))
}

#[test]
fn a_prepared_mutation_runs_from_the_parse_its_preparation_made() {
    workers::block_on(async {
        let runner = runner();
        let request =
            QueryRequest::new(r#"mutation { add_User(input: { name: "Alice" }) { name } }"#);

        let prepared = runner.prepare_request(&request).await.unwrap();
        let parsed = prepared
            .as_ref()
            .and_then(|prepared| prepared.parsed.as_deref())
            .expect("the capsule to carry the request's parse");
        let ParsedOperation::Mutation { mutations, .. } = parsed else {
            panic!("the parse of a mutation is a mutation: {parsed:?}");
        };
        assert_eq!(mutations.len(), 1);

        let response = runner.execute_prepared(request, prepared).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(
            response.data,
            Some(serde_json::json!({ "add_User": [{ "name": "Alice" }] }))
        );
    })
    .unwrap()
}

#[test]
fn a_request_without_a_mutation_is_left_to_its_execution_to_parse() {
    workers::block_on(async {
        let runner = runner();
        let query =
            QueryRequest::new(r#"query { User(filter: { name: { _eq: "Bob" } }) { name } }"#);
        assert!(runner.prepare_request(&query).await.unwrap().is_none());
        let response = runner.execute(query).await;
        assert!(!response.has_errors(), "{:?}", response.errors);

        // The word inside a query's argument does not make it a mutation.
        let worded =
            QueryRequest::new(r#"query { User(filter: { name: { _eq: "mutation" } }) { name } }"#);
        assert!(runner.prepare_request(&worded).await.unwrap().is_none());
        let response = runner.execute(worded).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
    })
    .unwrap()
}

#[test]
fn a_request_that_does_not_parse_answers_with_its_parse_error() {
    workers::block_on(async {
        for text in [
            "query { User { name ",
            "{ User(",
            "mutation { add_User(input: ",
        ] {
            let expected = parse_request_with_limits(text, None, None, QueryLimits::default())
                .expect_err("the request does not parse")
                .to_string();
            let response = runner().execute(QueryRequest::new(text)).await;
            assert_eq!(response.data, None);
            assert_eq!(
                response
                    .errors
                    .iter()
                    .map(|error| error.message.as_str())
                    .collect::<Vec<_>>(),
                vec![expected.as_str()],
                "{text}"
            );
        }
    })
    .unwrap()
}
