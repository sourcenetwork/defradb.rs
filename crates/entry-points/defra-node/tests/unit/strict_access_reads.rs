//! Reads on a remote document ACP provider whose access check for one row
//! cannot be answered: by default the row is dropped and the read goes on;
//! a strict read fails instead. Modeled by proofs/tla/StrictAccessRead.tla.

use std::sync::Arc;

use defra_core::RequestContext;

use identity::Did;

use super::staged_relationship_tests::{node_identity, provision, start, Remote, OWNER, READER};
use super::Node;

async fn node(remote: &Arc<Remote>) -> Node {
    let node = start(&node_identity(), Some(remote), None).await;
    provision(&node).await;
    node
}

fn created(response: query::QueryResponse) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn create(name: &str) -> query::QueryRequest {
    query::QueryRequest::new(format!(
        "mutation {{ add_Record(input: {{name: \"{name}\"}}) {{ _docID }} }}"
    ))
    .with_identity(Some(Did::new(OWNER).unwrap()))
}

fn read(did: &str) -> query::QueryRequest {
    query::QueryRequest::new("{ Record { name } }").with_identity(Some(Did::new(did).unwrap()))
}

fn names(response: &query::QueryResponse) -> Vec<String> {
    let mut names: Vec<String> = response.data.as_ref().unwrap()["Record"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_row_whose_check_cannot_be_answered_is_dropped_without_an_error() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        created(node.execute_request(create("seen")).await);
        let hidden = created(node.execute_request(create("unchecked")).await);
        remote.unchecked.insert(hidden, ());
        let response = node.execute_request(read(OWNER)).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(names(&response), ["seen"], "the owner's own row is missing");
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// `query` as `did`, strict.
fn strict(query: &str, did: &str) -> query::QueryRequest {
    query::QueryRequest::new(query)
        .with_identity(Some(Did::new(did).unwrap()))
        .with_context(
            RequestContext::default()
                .with_identity(Some(did.to_owned()))
                .with_strict_access_reads(),
        )
}

/// Whether `response` failed as a strict read whose check had no answer,
/// returning no rows and naming nothing.
fn unavailable(response: &query::QueryResponse, unchecked: &str) -> bool {
    response.data.is_none()
        && response.errors.len() == 1
        && response.errors[0].code() == Some(query::executor::ACCESS_CHECK_UNAVAILABLE_ERROR_CODE)
        && !response.errors[0].message.contains(unchecked)
}

#[test]
fn a_strict_read_fails_where_the_default_drops_and_only_filters_a_denial() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        created(node.execute_request(create("seen")).await);
        let hidden = created(node.execute_request(create("unchecked")).await);

        let denied = node
            .execute_request(strict("{ Record { name } }", READER))
            .await;
        assert!(
            !denied.has_errors(),
            "a denial is no error: {:?}",
            denied.errors
        );
        assert!(names(&denied).is_empty());

        remote.unchecked.insert(hidden.clone(), ());
        for query in [
            "{ Record { name } }",
            "{ Record(showDeleted: true) { name } }",
            "{ Record(filter: {name: {_eq: \"unchecked\"}}) { name } }",
        ] {
            let response = node.execute_request(strict(query, OWNER)).await;
            assert!(
                unavailable(&response, &hidden),
                "{query}: {:?}",
                response.errors
            );
        }
        let by_id = node
            .execute_request(strict(
                &format!("{{ Record(docID: \"{hidden}\") {{ name }} }}"),
                OWNER,
            ))
            .await;
        assert!(unavailable(&by_id, &hidden), "{:?}", by_id.errors);

        remote.unchecked.remove(&hidden);
        let answered = node
            .execute_request(strict("{ Record { name } }", OWNER))
            .await;
        assert!(!answered.has_errors(), "{:?}", answered.errors);
        assert_eq!(names(&answered), ["seen", "unchecked"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_failed_strict_read_in_a_transaction_leaves_it_usable() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let hidden = created(node.execute_request(create("unchecked")).await);
        remote.unchecked.insert(hidden.clone(), ());
        let txn = node.begin_transaction(false).await.unwrap();
        let refused = node
            .execute_request_in_txn(strict("{ Record { name } }", OWNER), &txn)
            .await;
        assert!(unavailable(&refused, &hidden), "{:?}", refused.errors);
        created(node.execute_request_in_txn(create("after"), &txn).await);
        node.commit_transaction(&txn).await.unwrap();
        remote.unchecked.remove(&hidden);
        let response = node.execute_request(read(OWNER)).await;
        assert_eq!(names(&response), ["after", "unchecked"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
