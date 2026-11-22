use async_trait::async_trait;
use document::{DocID, Document, NormalValue};
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexDescription,
    IndexedFieldDescription,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::Result;
use crate::fetcher::{DocFetcher, FetchByIdsResult, IndexScanResult};
use crate::mapper::{Field, Filter, OrderBy, OrderCondition, OrderDirection, Select};
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
            FieldDescription::new(
                "5",
                "publisher",
                FieldKind::relation("Publisher", false),
                CType::LwwRegister,
            )
            .with_relation_name("publisher_books")
            .as_primary(),
            FieldDescription::new("6", "_publisherID", FieldKind::doc_id(), CType::LwwRegister)
                .with_relation_name("publisher_books")
                .as_primary(),
        ],
    )
    .with_index(IndexDescription {
        id: 2,
        name: "Book_title_ASC".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "title".to_string(),
            descending: false,
        }],
    })
}

fn books_with_parent_fk_index() -> CollectionVersion {
    books().with_index(IndexDescription {
        id: 1,
        name: "Book__authorID_ASC".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "_authorID".to_string(),
            descending: false,
        }],
    })
}

fn publishers() -> CollectionVersion {
    CollectionVersion::new(
        "Publisher",
        "v1",
        "coll-publisher",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "name", FieldKind::string(), CType::LwwRegister),
            FieldDescription::new(
                "3",
                "published",
                FieldKind::relation("Book", true),
                CType::LwwRegister,
            )
            .with_relation_name("publisher_books"),
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

fn publisher(name: &str) -> Document {
    Document::from_json_str(
        &json!({"_docID": doc_id(name), "name": name}).to_string(),
        publishers(),
    )
    .unwrap()
}

fn book(title: &str, author: &str) -> Document {
    Document::from_json_str(
        &json!({
            "_docID": doc_id(title),
            "title": title,
            "_authorID": doc_id(author),
            "_publisherID": doc_id("Penguin"),
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
    publishers: Vec<Document>,
    /// Every Book document this fetcher has handed out, by any route.
    book_reads: AtomicUsize,
}

impl IndexedSideFetcher {
    fn new() -> Self {
        Self {
            people: vec![person("David"), person("Other")],
            books: vec![book("David", "David"), book("Other", "Other")],
            publishers: vec![publisher("Penguin")],
            book_reads: AtomicUsize::new(0),
        }
    }

    fn all(&self, collection_name: &str) -> Vec<Document> {
        match collection_name {
            "Person" => self.people.clone(),
            "Book" => {
                self.book_reads
                    .fetch_add(self.books.len(), Ordering::SeqCst);
                self.books.clone()
            }
            "Publisher" => self.publishers.clone(),
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
        let docs: Vec<_> = self
            .all(collection_name)
            .into_iter()
            .filter(|doc| doc.id().is_some_and(|id| doc_ids.contains(&id.to_string())))
            .collect();
        if collection_name == "Book" {
            // Only the Books returned are read, not every one `all` counted.
            self.book_reads
                .fetch_sub(self.books.len() - docs.len(), Ordering::SeqCst);
        }
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
        collection_name: &str,
        params: &IndexScanParams,
    ) -> Result<IndexScanResult> {
        if collection_name == "Book" {
            if params.index_name == "Book_title_ASC" {
                // The title index, read whole for an order it provides.
                let mut books: Vec<_> = self.books.iter().collect();
                books.sort_by_key(|book| {
                    book.get("title")
                        .and_then(|value| value.as_str().map(String::from))
                });
                return Ok(IndexScanResult::new(
                    books
                        .into_iter()
                        .filter_map(|book| book.id().map(|id| id.to_string()))
                        .collect(),
                ));
            }
            if params.index_name == "Book__authorID_ASC" {
                let IndexScanType::ExactMatch { values } = &params.scan_type else {
                    panic!("unexpected author index scan type: {:?}", params.scan_type);
                };
                let author_ids: Vec<&str> = values
                    .iter()
                    .filter_map(|value| match value {
                        NormalValue::String(value) => Some(value.as_str()),
                        _ => None,
                    })
                    .collect();
                return Ok(IndexScanResult::new(
                    self.books
                        .iter()
                        .filter(|book| {
                            book.get("_authorID")
                                .and_then(|value| value.as_str())
                                .is_some_and(|author_id| author_ids.contains(&author_id))
                        })
                        .filter_map(|book| book.id().map(|id| id.to_string()))
                        .collect(),
                ));
            }
        }
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

/// The direction of the join on `relation` in `explain`.
fn join_direction(explain: &serde_json::Value, relation: &str) -> Option<String> {
    match explain {
        serde_json::Value::Object(map) => {
            if let Some(join) = map.get("typeIndexJoin") {
                if join.get("subTypeName").and_then(serde_json::Value::as_str) == Some(relation) {
                    return join
                        .get("direction")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from);
                }
            }
            map.values()
                .find_map(|value| join_direction(value, relation))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|value| join_direction(value, relation)),
        _ => None,
    }
}

fn author_named(name: &str) -> Filter {
    Filter::from_conditions(
        json!({"author": {"name": {"_eq": name}}})
            .as_object()
            .cloned()
            .unwrap(),
    )
}

/// Every row of `plan`, rendered, and the plan's execute explain.
async fn run(
    mut plan: Box<dyn crate::planner::PlanNode>,
) -> (Vec<serde_json::Value>, serde_json::Value) {
    plan.init().await.unwrap();
    plan.start().await.unwrap();
    let mut rows = Vec::new();
    while plan.next().await.unwrap() {
        rows.push(plan.document_map().render_doc_to_json(plan.value()));
    }
    plan.close().await.unwrap();
    (rows, plan.explain_execute())
}

fn planner() -> Planner {
    planner_over(Arc::new(IndexedSideFetcher::new()))
}

fn planner_over(fetcher: Arc<IndexedSideFetcher>) -> Planner {
    let fetcher: Arc<dyn DocFetcher> = fetcher;
    Planner::new(vec![people(), books(), publishers()]).with_fetcher(fetcher)
}

/// With no index on the parent FK, the Books are read once, in their own
/// order, and each is kept when the indexed author matches.
#[test]
fn filter_on_an_indexed_child_reads_the_parents_once() {
    workers::block_on(async {
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_filter(author_named("David"));
        let plan = planner().plan(&select).unwrap();
        assert_eq!(
            join_direction(&plan.explain(), "author").as_deref(),
            Some("primary"),
            "{}",
            plan.explain()
        );

        let (rows, _) = run(plan).await;
        let titles: Vec<_> = rows.iter().map(|row| row["title"].clone()).collect();
        assert_eq!(titles, vec![json!("David")]);
    })
    .unwrap()
}

/// A parent-side FK index switches the join to parent-index lookups. A relation
/// already joined into the parent plan must still survive those raw lookups.
#[test]
fn indexed_side_filter_keeps_relation_when_parent_fk_is_indexed() {
    workers::block_on(async {
        let fetcher = Arc::new(IndexedSideFetcher::new());
        let planner = Planner::new(vec![people(), books_with_parent_fk_index(), publishers()])
            .with_fetcher(fetcher.clone());
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_select(Select::new("publisher").with_field(Field::new("name")))
            .with_filter(Filter::from_conditions(
                json!({"author": {"name": {"_eq": "David"}}})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ));

        let mut plan = planner.plan(&select).unwrap();
        assert_eq!(
            join_direction(&plan.explain(), "author").as_deref(),
            Some("secondary"),
            "{}",
            plan.explain()
        );
        plan.init().await.unwrap();
        plan.start().await.unwrap();

        let mut rows = Vec::new();
        while plan.next().await.unwrap() {
            rows.push(plan.document_map().render_doc_to_json(plan.value()));
        }
        plan.close().await.unwrap();

        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["title"], json!("David"), "{rows:?}");
        assert_eq!(rows[0]["publisher"]["name"], json!("Penguin"), "{rows:?}");
        assert_eq!(fetcher.book_reads.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

/// The ordered primary path fetches raw parents by ID too. Its already-planned
/// relation is reapplied directly, without rescanning all parent documents.
#[test]
fn ordered_parent_lookup_keeps_relation_when_parent_fk_is_indexed() {
    workers::block_on(async {
        let fetcher = Arc::new(IndexedSideFetcher::new());
        let planner = Planner::new(vec![people(), books_with_parent_fk_index(), publishers()])
            .with_fetcher(fetcher.clone());
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_select(Select::new("publisher").with_field(Field::new("name")))
            .with_filter(author_named("David"))
            .with_order(OrderBy::new().with_condition(OrderCondition::with_path(
                vec!["author".to_string(), "name".to_string()],
                OrderDirection::Asc,
            )));

        let mut plan = planner.plan(&select).unwrap();
        assert_eq!(
            join_direction(&plan.explain(), "author").as_deref(),
            Some("secondary"),
            "{}",
            plan.explain()
        );
        plan.init().await.unwrap();
        plan.start().await.unwrap();

        let mut rows = Vec::new();
        while plan.next().await.unwrap() {
            rows.push(plan.document_map().render_doc_to_json(plan.value()));
        }
        let explain = plan.explain_execute();
        plan.close().await.unwrap();

        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["title"], json!("David"), "{rows:?}");
        assert_eq!(rows[0]["publisher"]["name"], json!("Penguin"), "{rows:?}");
        assert_eq!(fetcher.book_reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            explain["selectNode"]["typeIndexJoin"]["typeJoinOne"]["root"]["scanNode"]["docFetches"],
            json!(2),
            "{explain}"
        );
    })
    .unwrap()
}

/// A relation already joined into the parent plan, here the selected
/// publisher, is kept on a Book the author filter keeps.
#[test]
fn filter_on_an_indexed_child_keeps_relations_joined_into_the_parent() {
    workers::block_on(async {
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_select(Select::new("publisher").with_field(Field::new("name")))
            .with_filter(author_named("David"));
        let (rows, _) = run(planner().plan(&select).unwrap()).await;
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["title"], json!("David"), "{rows:?}");
        assert_eq!(rows[0]["publisher"]["name"], json!("Penguin"), "{rows:?}");
    })
    .unwrap()
}

/// When no author matches, no Book can pass the filter, and none is read.
/// Explain reports the parent scan under the root and the child lookups under
/// the child branch.
#[test]
fn no_matching_child_reads_no_parent() {
    workers::block_on(async {
        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_filter(author_named("David"));
        let (_, explain) = run(planner().plan(&select).unwrap()).await;
        let root = &explain["selectNode"]["typeIndexJoin"]["typeJoinOne"]["root"];
        assert_eq!(root["scanNode"]["docFetches"], json!(2), "{explain}");
        let child = &explain["selectNode"]["typeIndexJoin"]["typeJoinOne"]["subType"]
            ["selectTopNode"]["selectNode"];
        assert_eq!(child["filterMatches"], json!(1), "{explain}");
        assert_eq!(child["scanNode"]["docFetches"], json!(1), "{explain}");
        assert_eq!(child["scanNode"]["indexFetches"], json!(1), "{explain}");

        let select = Select::new("Book")
            .with_field(Field::new("title"))
            .with_filter(author_named("Nobody"));
        let (rows, explain) = run(planner().plan(&select).unwrap()).await;
        assert!(rows.is_empty(), "{rows:?}");
        let root = &explain["selectNode"]["typeIndexJoin"]["typeJoinOne"]["root"];
        assert_eq!(root["scanNode"]["docFetches"], json!(0), "{explain}");
        let child = &explain["selectNode"]["typeIndexJoin"]["typeJoinOne"]["subType"]
            ["selectTopNode"]["selectNode"];
        assert_eq!(child["filterMatches"], json!(0), "{explain}");
        assert_eq!(child["scanNode"]["docFetches"], json!(0), "{explain}");
    })
    .unwrap()
}

/// When no author matches and a parent index provides the order, the
/// index-ordered Book scan reads no Book either: a scan that loads its
/// documents when it starts is not started.
#[test]
fn no_matching_child_reads_no_parent_of_an_index_ordered_scan() {
    workers::block_on(async {
        let ordered = |name: &str| {
            Select::new("Book")
                .with_field(Field::new("title"))
                .with_filter(author_named(name))
                .with_order(
                    OrderBy::new()
                        .with_condition(OrderCondition::new("title", OrderDirection::Asc)),
                )
        };

        let fetcher = Arc::new(IndexedSideFetcher::new());
        let (rows, _) = run(planner_over(fetcher.clone())
            .plan(&ordered("David"))
            .unwrap())
        .await;
        let titles: Vec<_> = rows.iter().map(|row| row["title"].clone()).collect();
        assert_eq!(titles, vec![json!("David")]);
        assert!(fetcher.book_reads.load(Ordering::SeqCst) > 0);

        let fetcher = Arc::new(IndexedSideFetcher::new());
        let (rows, _) = run(planner_over(fetcher.clone())
            .plan(&ordered("Nobody"))
            .unwrap())
        .await;
        assert!(rows.is_empty(), "{rows:?}");
        assert_eq!(fetcher.book_reads.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}
