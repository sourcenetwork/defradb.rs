//! A parent delete reaches a peer whose pending-DAG map is full
//! (`defra.chunk.replicate.saturated`, #2307).

use integration_test::actions::{Actions, AdmissionSpec, DeleteDoc, Request};
use integration_test::scenario::{Common, Eventually, Text};
use serde_json::json;

use super::support::{
    add_article_on, body, chunks_are_listed, on, read_chunks, replicated_pair_into,
    without_default_node, CHUNKS,
};

/// Long enough for every waiting chunk to give the entry up in turn (about
/// 14 seconds each), far short of a sweep tick (60 seconds) per chunk.
fn within(seconds: u64, mut action: Actions) -> Actions {
    if let Actions::Request(request) = &mut action {
        request.common.eventually = Some(Eventually {
            timeout_millis: Some(seconds * 1_000),
            interval_millis: Some(500),
        });
    }
    action
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.saturated")]
fn a_parent_delete_reaches_a_peer_whose_pending_map_is_full() {
    const CHUNK_COUNT: usize = 8;
    let mut actions = replicated_pair_into(
        &["Article", CHUNKS],
        Some(AdmissionSpec {
            // One quarter of this is the sender's quota: a single entry.
            max_pending_dags: 4,
            rate_limit_burst: 500,
            rate_limit_rate: 50.0,
            deferral_peer_bytes: None,
        }),
    );
    actions.extend([
        add_article_on(&body(CHUNK_COUNT, "glaciers"), "article", Some("db0")),
        read_chunks("created", CHUNK_COUNT, Some("db0")),
        within(150, chunks_are_listed("created", CHUNK_COUNT, Some("db1"))),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        within(
            150,
            Request {
                request: format!("query {{ Article {{ _docID }} {CHUNKS} {{ hash }} }}").into(),
                expected_data: json!({ "Article": [], CHUNKS: [] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
    ]);
    without_default_node(actions).execute();
}
