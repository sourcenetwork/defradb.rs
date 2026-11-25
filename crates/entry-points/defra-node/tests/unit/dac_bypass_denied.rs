//! A request whose caller denied it the DAC bypass
//! (`RequestContext::with_dac_bypass_denied`) is decided by document access
//! control on every path, even when node access control grants its identity
//! `bypass-dac`, as it does a node administrator. A request without the denial
//! keeps the bypass, and an identity without `bypass-dac` is unaffected. The
//! denial belongs to the request: a transaction begun with it does not deny
//! a later request in that transaction.
//!
//! In-process, the identity-based bypass reaches only a query and a commits
//! query. An administrator without the denial already counts 0, reads nothing
//! from a subscription's scoped query and changes nothing by a mutation, so
//! there the denied 0 is what document access control gives either way, not
//! a sign of the denial.

use defra_core::RequestContext;
use query::QueryRequest;
use serde_json::Value;

use super::Node;

const NAC_OWNER: &str = "did:key:z6MkiMqcDKMWsVCTvyyAuMKBpBERre9BJ4SAYdwpYb39rB2i";
const ADMIN: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "name: Rows\nresources:\n  - name: rows\n    relations:\n      - name: reader\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n";
const SUBSCRIPTION: &str = "subscription { Record { _docID pending } }";

fn actor(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_owned()))
}

fn denied(did: &str) -> RequestContext {
    actor(did).with_dac_bypass_denied()
}

/// A node whose access control the NAC owner holds, with `ADMIN` a node
/// administrator and `READER` granted only the document operations, and one
/// document only the NAC owner may read under document access control.
async fn fixture() -> (Node, String, String) {
    let node = Node::builder()
        .with_node_acp_owner(NAC_OWNER)
        .build()
        .await
        .unwrap();
    let owner = actor(NAC_OWNER);
    node.add_nac_actor_relationship_as(&owner, "admin", ADMIN)
        .await
        .unwrap();
    for permission in [
        acp::NodePermission::CollectionGet,
        acp::NodePermission::DocumentRead,
        acp::NodePermission::DocumentUpdate,
        acp::NodePermission::DocumentDelete,
    ] {
        node.add_nac_actor_relationship_as(&owner, permission.as_str(), READER)
            .await
            .unwrap();
    }
    let policy = node.add_dac_policy_as(&owner, POLICY).await.unwrap();
    node.add_collection(
        &owner,
        &format!(
            "type Record @policy(id: \"{policy}\", resource: \"rows\") {{ pending: Boolean }}"
        ),
    )
    .await
    .unwrap();
    let added = run(
        &node,
        &owner,
        "mutation { add_Record(input: {pending: true}) { _docID } }",
    )
    .await;
    let doc = added["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    let commits = run(&node, &owner, "{ _commits { cid } }").await;
    let cid = commits["_commits"][0]["cid"].as_str().unwrap().to_owned();
    (node, doc, cid)
}

async fn run(node: &Node, context: &RequestContext, query: &str) -> Value {
    let response = node
        .execute_request_as(context, QueryRequest::new(query))
        .await
        .unwrap();
    assert!(response.errors.is_empty(), "{query}: {:?}", response.errors);
    response.data.unwrap_or(Value::Null)
}

fn rows(data: &Value, field: &str) -> usize {
    data[field].as_array().map_or(0, Vec::len)
}

/// How many rows `context` reads on each read path: a query, a commits
/// query, an aggregate, and a subscription's update event, which the embedded
/// API answers as the query scoped to the document's new head.
async fn reads(node: &Node, context: &RequestContext, doc: &str, cid: &str) -> [usize; 4] {
    let query = rows(
        &run(node, context, "{ Record { pending } }").await,
        "Record",
    );
    let commits = rows(
        &run(node, context, "{ _commits { cid } }").await,
        "_commits",
    );
    let count = run(node, context, "{ COUNT(Record: {}) }").await["COUNT"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap();
    let scoped =
        query::subscription::subscription_to_scoped_query(SUBSCRIPTION, doc, cid, None).unwrap();
    let subscribed = rows(&run(node, context, &scoped).await, "Record");
    [query, commits, count, subscribed]
}

/// How many documents `context` changed by a filtered update and a delete.
/// Each mutation must succeed, so 0 means document access control hid the
/// document rather than that the mutation failed.
async fn writes(node: &Node, context: &RequestContext, doc: &str) -> (usize, usize) {
    let updated = rows(
        &run(
            node,
            context,
            "mutation { update_Record(filter: {}, input: {pending: false}) { _docID } }",
        )
        .await,
        "update_Record",
    );
    let deleted = rows(
        &run(
            node,
            context,
            &format!("mutation {{ delete_Record(docID: \"{doc}\") {{ _docID }} }}"),
        )
        .await,
        "delete_Record",
    );
    (updated, deleted)
}

#[test]
fn an_administrator_denied_the_bypass_reads_and_writes_nothing_dac_hides() {
    workers::block_on(async {
        let (node, doc, cid) = fixture().await;

        // Only the query and the commits query contrast with an administrator
        // without the denial; the other paths give 0 to both.
        assert_eq!(reads(&node, &denied(ADMIN), &doc, &cid).await, [0; 4]);
        assert_eq!(writes(&node, &denied(ADMIN), &doc).await, (0, 0));
        let kept = run(&node, &actor(NAC_OWNER), "{ Record { pending } }").await;
        assert_eq!(kept["Record"][0]["pending"], Value::Bool(true));

        let handle = node
            .runner()
            .begin_txn_as(&actor(ADMIN), true)
            .await
            .unwrap();
        let in_txn = denied(ADMIN).with_txn(Some(handle.as_str().parse().unwrap()));
        let read = run(&node, &in_txn, "{ Record { pending } }").await;
        assert_eq!(rows(&read, "Record"), 0);
        node.runner()
            .discard_txn_as(&actor(ADMIN), &handle)
            .await
            .unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn an_administrator_without_the_denial_keeps_the_bypass() {
    workers::block_on(async {
        let (node, doc, cid) = fixture().await;
        let [query, commits, count, subscribed] = reads(&node, &actor(ADMIN), &doc, &cid).await;
        assert_eq!(query, 1);
        assert!(commits >= 1);
        // The identity-based bypass does not reach an aggregate, a
        // subscription's scoped query or a mutation, so an administrator
        // without the denial gets there what document access control gives.
        assert_eq!((count, subscribed), (0, 0));
        assert_eq!(writes(&node, &actor(ADMIN), &doc).await, (0, 0));
        node.shutdown().await.unwrap();

        let (node, doc, _) = fixture().await;
        assert_eq!(writes(&node, &actor(NAC_OWNER), &doc).await, (1, 1));
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_transaction_begun_denied_leaves_a_later_request_undenied() {
    workers::block_on(async {
        let (node, _, _) = fixture().await;
        let handle = node
            .runner()
            .begin_txn_as(&denied(ADMIN), true)
            .await
            .unwrap();
        let txn = Some(handle.as_str().parse().unwrap());

        let undenied = run(&node, &actor(ADMIN).with_txn(txn), "{ Record { pending } }").await;
        assert_eq!(rows(&undenied, "Record"), 1);
        let still_denied = run(
            &node,
            &denied(ADMIN).with_txn(txn),
            "{ Record { pending } }",
        )
        .await;
        assert_eq!(rows(&still_denied, "Record"), 0);

        node.runner()
            .discard_txn_as(&denied(ADMIN), &handle)
            .await
            .unwrap();

        let guard = query::TransactionGuard::begin_as(node.runner().as_ref(), denied(ADMIN), true)
            .await
            .unwrap();
        let response = guard
            .execute(QueryRequest::new("{ Record { pending } }"))
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(rows(&response.data.unwrap(), "Record"), 0);
        guard.discard().await.unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn an_identity_without_bypass_dac_is_unaffected_by_the_denial() {
    workers::block_on(async {
        let (node, doc, cid) = fixture().await;
        assert_eq!(reads(&node, &actor(READER), &doc, &cid).await, [0; 4]);
        assert_eq!(reads(&node, &denied(READER), &doc, &cid).await, [0; 4]);

        node.add_dac_actor_relationship_as(&actor(NAC_OWNER), "Record", &doc, "reader", READER)
            .await
            .unwrap();
        let [query, commits, count, subscribed] = reads(&node, &denied(READER), &doc, &cid).await;
        assert_eq!((query, count, subscribed), (1, 1, 1));
        assert!(commits >= 1);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}
