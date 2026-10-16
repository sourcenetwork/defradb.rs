use async_trait::async_trait;
use document::Document;
use schema::{FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription};
use serde_json::json;

use super::*;
use crate::doc_stream::DocStream;
use crate::fetcher::FetchByIdsResult;
use crate::planner::index_selection::{select_best_index_with_estimates, IndexScanParams};

struct Counts {
    scope: u64,
    key: u64,
    unknown_at: Option<usize>,
    calls: kovan::Atom<Vec<(String, u64, Option<u64>)>>,
}

impl Counts {
    fn new(scope: u64, key: u64) -> Self {
        Self {
            scope,
            key,
            unknown_at: None,
            calls: kovan::Atom::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DocFetcher for Counts {
    async fn stream_all_with_deleted(&self, _: &str, _: bool) -> Result<Box<dyn DocStream>> {
        unreachable!()
    }
    async fn get_all(&self, _: &str) -> Result<Vec<Document>> {
        unreachable!()
    }
    async fn get_by_ids(&self, _: &str, _: &[String]) -> Result<FetchByIdsResult> {
        unreachable!()
    }
    async fn get_by_field_value(&self, _: &str, _: &str, _: &str) -> Result<Vec<Document>> {
        unreachable!()
    }
    async fn stream_by_doc_short_ids(
        &self,
        _: &str,
        _: &[u64],
        _: bool,
    ) -> Result<Box<dyn DocStream>> {
        unreachable!()
    }
    fn supports_index_queries(&self) -> bool {
        true
    }
    async fn estimate_index_scan(
        &self,
        _: &str,
        params: &IndexScanParams,
        cap: u64,
    ) -> Result<Option<u64>> {
        let count = if self.unknown_at == Some(self.calls.load().len()) {
            None
        } else {
            Some(match params.index_name.as_str() {
                "scope" => self.scope.min(cap),
                "key" => self.key.min(cap),
                other => panic!("unexpected index {other}"),
            })
        };
        self.calls.rcu(|calls| {
            let mut next = calls.clone();
            next.push((params.index_name.clone(), cap, count));
            next
        });
        Ok(count)
    }
}

fn fixture() -> (CollectionVersion, Filter) {
    let mut collection = CollectionVersion::new(
        "headers",
        "v1",
        "headers",
        vec![
            FieldDescription::new("1", "node", FieldKind::string()),
            FieldDescription::new("2", "session", FieldKind::string()),
            FieldDescription::new("3", "sequence", FieldKind::int()),
            FieldDescription::new("4", "key", FieldKind::string()),
        ],
    );
    let index = |id, name: &str, fields: &[&str]| IndexDescription {
        id,
        name: name.into(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: fields
            .iter()
            .map(|name| IndexedFieldDescription {
                name: (*name).into(),
                descending: false,
            })
            .collect(),
    };
    collection.indexes = vec![
        index(1, "scope", &["node", "session", "sequence"]),
        index(2, "key", &["key"]),
    ];
    let filter = Filter::from_conditions(
        serde_json::from_value(json!({
            "node": {"_eq": "node"}, "session": {"_eq": "session"}, "key": {"_eq": "message"}
        }))
        .unwrap(),
    );
    (collection, filter)
}

#[tokio::test]
async fn small_candidate_avoids_a_full_probe_of_the_shape_winner() {
    let (collection, filter) = fixture();
    let fetcher = Counts::new(5_000, 1);
    let estimates = estimate_filter_indexes(&fetcher, &collection, &filter)
        .await
        .unwrap();
    assert_eq!(
        select_best_index_with_estimates(&filter, &collection.indexes, &estimates)
            .unwrap()
            .name,
        "key"
    );
    let calls = fetcher.calls.load();
    assert_eq!(
        calls.iter().filter_map(|(_, _, count)| *count).sum::<u64>(),
        33
    );
    assert!(calls.iter().all(|(_, cap, _)| *cap <= 32));
}

#[tokio::test]
async fn probes_preserve_capped_selection_and_shape_ties() {
    let (collection, filter) = fixture();
    for scope in [
        0, 1, 2, 31, 32, 33, 63, 64, 65, 255, 256, 257, 1023, 1024, 1025, 5_000,
    ] {
        for key in [
            0, 1, 2, 31, 32, 33, 63, 64, 65, 255, 256, 257, 1023, 1024, 1025, 5_000,
        ] {
            let fetcher = Counts::new(scope, key);
            let observed = estimate_filter_indexes(&fetcher, &collection, &filter)
                .await
                .unwrap();
            let reference = [
                ("scope".to_owned(), scope.min(ESTIMATE_CAP)),
                ("key".to_owned(), key.min(ESTIMATE_CAP)),
            ]
            .into_iter()
            .collect();
            assert_eq!(
                select_best_index_with_estimates(&filter, &collection.indexes, &observed)
                    .unwrap()
                    .name,
                select_best_index_with_estimates(&filter, &collection.indexes, &reference)
                    .unwrap()
                    .name,
                "scope={scope}, key={key}"
            );
            assert!(
                fetcher
                    .calls
                    .load()
                    .iter()
                    .filter_map(|(_, _, count)| *count)
                    .sum::<u64>()
                    <= 2 * (ESTIMATE_CAP + 32)
            );
        }
    }
}

#[tokio::test]
async fn unknown_counts_discard_partial_probes_in_either_phase() {
    let (collection, filter) = fixture();
    for unknown_at in 0..4 {
        let mut fetcher = Counts::new(5_000, 5_000);
        fetcher.unknown_at = Some(unknown_at);
        assert!(estimate_filter_indexes(&fetcher, &collection, &filter)
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn a_single_usable_index_needs_no_estimates() {
    let (mut collection, filter) = fixture();
    collection.indexes.pop();
    let fetcher = Counts::new(5_000, 1);
    assert!(estimate_filter_indexes(&fetcher, &collection, &filter)
        .await
        .unwrap()
        .is_empty());
    assert!(fetcher.calls.load().is_empty());
}
