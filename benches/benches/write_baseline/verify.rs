use std::collections::{BTreeMap, BTreeSet};

use document::NormalValue;
use query::planner::index_selection::{IndexScanParams, IndexScanType};
use query::{DocFetcher, QueryRequest};

use super::{Fixture, Scenario, COLLECTION, SHARED_DOCUMENTS};

pub(super) async fn check(fixture: &Fixture, scenario: Scenario, successes: &[(usize, usize)]) {
    let response = fixture
        .state
        .executor
        .execute(QueryRequest::new(
            "query { Sample { _docID count label left right } }",
        ))
        .await;
    assert!(
        response.errors.is_empty(),
        "verification: {:?}",
        response.errors
    );
    let data = response.data.unwrap();
    let docs = data[COLLECTION].as_array().unwrap();
    if matches!(scenario, Scenario::Creates) {
        let expected: BTreeSet<_> = successes
            .iter()
            .map(|&(writer, operation)| token(writer, operation))
            .collect();
        let labels: BTreeSet<_> = docs
            .iter()
            .map(|doc| doc["label"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(docs.len(), successes.len());
        assert_eq!(labels, expected);
        return;
    }
    assert_eq!(docs.len(), SHARED_DOCUMENTS);
    for (index, id) in fixture.ids.iter().enumerate() {
        let doc = docs.iter().find(|doc| doc["_docID"] == *id).unwrap();
        let updates: Vec<_> = successes
            .iter()
            .copied()
            .filter(|(_, op)| op % SHARED_DOCUMENTS == index)
            .collect();
        if matches!(scenario, Scenario::SharedUpdates) {
            for (parity, field) in [(0, "left"), (1, "right")] {
                assert_last_write(
                    updates
                        .iter()
                        .copied()
                        .filter(|(writer, _)| writer % 2 == parity),
                    doc[field].as_str().unwrap(),
                    "initial",
                    &format!("{id}.{field}"),
                );
            }
        } else {
            assert_eq!(doc["count"].as_u64().unwrap(), updates.len() as u64);
            assert_last_write(
                updates,
                doc["label"].as_str().unwrap(),
                &format!("seed-{index}"),
                &format!("{id}.label"),
            );
        }
    }
    if matches!(scenario, Scenario::CountersIndexes) {
        let fetcher = db::LensedAutoCommitFetcher::new(fixture.db.clone());
        let scan = |scan_type| IndexScanParams {
            index_name: "by_label".into(),
            scan_type,
            limit: None,
            offset: 0,
            value_filter: None,
            cursor_seek: None,
        };
        let all = fetcher
            .get_by_index_scan(
                COLLECTION,
                &scan(IndexScanType::PrefixScan {
                    prefix_values: Vec::new(),
                    reverse: false,
                }),
            )
            .await
            .unwrap();
        assert_eq!(
            all.raw_fetches(),
            docs.len() as u64,
            "no obsolete label entries remain"
        );
        for doc in docs {
            let exact = fetcher
                .get_by_index_scan(
                    COLLECTION,
                    &scan(IndexScanType::ExactMatch {
                        values: vec![NormalValue::String(
                            doc["label"].as_str().unwrap().to_owned(),
                        )],
                    }),
                )
                .await
                .unwrap();
            assert_eq!(exact.raw_fetches(), 1);
            assert_eq!(
                exact.doc_ids(),
                &[doc["_docID"].as_str().unwrap().to_owned()]
            );
        }
    }
}

fn assert_last_write(
    updates: impl IntoIterator<Item = (usize, usize)>,
    actual: &str,
    initial: &str,
    field: &str,
) {
    let latest: BTreeMap<_, _> = updates.into_iter().collect();
    let permitted: BTreeSet<_> = latest
        .into_iter()
        .map(|(writer, op)| token(writer, op))
        .collect();
    if permitted.is_empty() {
        assert_eq!(actual, initial);
    } else {
        assert!(
            permitted.contains(actual),
            "{field}: {actual} is not a final successful write: {permitted:?}"
        );
    }
}

fn token(writer: usize, operation: usize) -> String {
    format!("writer-{writer}-operation-{operation}")
}
