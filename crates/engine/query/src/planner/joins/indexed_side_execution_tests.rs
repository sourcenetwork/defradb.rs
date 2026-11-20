use async_trait::async_trait;
use document::{DocID, Document, NormalValue};
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexDescription,
    IndexedFieldDescription,
};
use serde_json::json;
use std::sync::Arc;

use crate::error::Result;
use crate::fetcher::{DocFetcher, FetchByIdsResult, IndexScanResult};
use crate::mapper::{Field, Filter, Select};
use crate::planner::builder::Planner;
use crate::planner::{IndexScanParams, IndexScanType};

fn people() -> CollectionVersion {
    CollectionVersion::new(
        "Person",
        "v1",
        "coll-person",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "name", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "authoredBooks",
                FieldKind::relation("Book", true),
                CType::LwwRegister,
            )
            .with_relation_name("author_books"),
        ],
    )
    .with_index(IndexDescription {
        id: 1,
        name: "Person_name_ASC".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "name".to_string(),
            descending: false,
        }],
    })
}

fn books() -> CollectionVersion {
    CollectionVersion::new(
        "Book",
        "v1",
        "coll-book",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "title", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "author",
                FieldKind::relation("Person", false),
                CType::LwwRegister,
            )
            .with_relation_name("author_books")
            .as_primary(),
            FieldDescription::new("4", "_authorID", FieldKind::doc_id(), CType::LwwRegister)
                .with_relation_name("author_books")
                .as_primary(),
        ],
    )
}

fn doc_id(seed: &str) -> String {
    DocID::new_v0_from_seed(seed).to_string()
}

fn person(name: &str) -> Document {
    Document::from_json_str(
        &json!({"_docID": doc_id(name), "name": name}).to_string(),
        people(),
    )
    .unwrap()
}

fn book(title: &str, author: &str) -> Document {
    Document::from_json_str(
        &json!({
            "_docID": doc_id(title),
            "title": title,
            "_authorID": doc_id(author),
        })
        .to_string(),
        books(),
    )
    .unwrap()
}

#[derive(Debug)]
struct IndexedSideFetcher {
    people: Vec<Document>,
    books: Vec<Document>,
}

impl IndexedSideFetcher {
    fn new() -> Self {
        Self {
            people: vec![person("David"), person("Other")],
            books: vec![book("David", "David"), book("Other", "Other")],
        }
    }

    fn all(&self, collection_name: &str) -> Vec<Document> {
        match collection_name {
            "Person" => self.people.clone(),
            "Book" => self.books.clone(),
            _ => Vec::new(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocFetcher for IndexedSideFetcher {
    async fn get_all(&self, collection_name: &str) -> Result<Vec<Document>> {
        Ok(self.all(collection_name))
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        _show_deleted: bool,
    ) -> Result<Box<dyn crate::doc_stream::DocStream>> {
        let docs = self.all(collection_name);
        Ok(Box::new(crate::doc_stream::VecStream::new(
            docs.into_iter().map(|doc| (doc, false)).collect(),
        )))
    }

    async fn stream_by_doc_short_ids(
        &self,
        _collection_name: &str,
        _doc_short_ids: &[u64],
        _show_deleted: bool,
    ) -> Result<Box<dyn crate::doc_stream::DocStream>> {
        Ok(Box::new(crate::doc_stream::VecStream::new(Vec::new())))
    }

    async fn get_by_ids(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> Result<FetchByIdsResult> {
        let docs = self
            .all(collection_name)
            .into_iter()
            .filter(|doc| doc.id().is_some_and(|id| doc_ids.contains(&id.to_string())))
            .collect();
        Ok(FetchByIdsResult::all_found(docs))
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> Result<Vec<Document>> {
        Ok(self
            .all(collection_name)
            .into_iter()
            .filter(|doc| doc.get(field_name).and_then(|value| value.as_str()) == Some(value))
            .collect())
    }

    async fn get_by_index_scan(
        &self,
        _collection_name: &str,
        params: &IndexScanParams,
    ) -> Result<IndexScanResult> {
        let IndexScanType::ExactMatch { values } = &params.scan_type else {
            panic!("unexpected index scan type in test: {:?}", params.scan_type);
        };
        let names: Vec<&str> = values
            .iter()
            .filter_map(|value| match value {
                NormalValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .collect();
        let doc_ids = self
            .people
            .iter()
            .filter(|person| {
                person
                    .get("name")
                    .and_then(|value| value.as_str())
                    .is_some_and(|name| names.contains(&name))
            })
            .filter_map(|person| person.id().map(|id| id.to_string()))
            .collect();
        Ok(IndexScanResult::new(doc_ids))
    }

    fn supports_index_queries(&self) -> bool {
        true
    }
}

#[test]
fn indexed_side_filter_returns_only_matching_books() {
    workers::block_on(async {
        let fetcher: Arc<dyn DocFetcher> = Arc::new(IndexedSideFetcher::new());
        let planner = Planner::new(vec![people(), books()]).with_fetcher(fetcher);
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_filter(Filter::from_conditions(
                json!({"author": {"name": {"_eq": "David"}}})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ));

        let mut plan = planner.plan(&select).unwrap();
        plan.init().await.unwrap();
        plan.start().await.unwrap();

        let mut titles = Vec::new();
        while plan.next().await.unwrap() {
            let rendered = plan.document_map().render_doc_to_json(plan.value());
            titles.push(rendered["title"].as_str().unwrap().to_string());
        }
        plan.close().await.unwrap();

        assert_eq!(titles, vec!["David".to_string()]);
    })
    .unwrap()
}
