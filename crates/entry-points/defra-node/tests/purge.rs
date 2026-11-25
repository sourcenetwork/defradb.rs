//! `Node::shutdown_and_wipe` wipes nothing when it cannot know the store is
//! released: another handle still holds the node, so the closed store's
//! directory stays locked (`WipeWhileHeld`), or another caller ran the
//! shutdown, whose outcome it never saw (`WipeAfterShutdown`). Modeled by
//! proofs/tla/StorePurge.tla's LastHandleChecked and ShutdownOnce guards.

#[path = "support/outcome.rs"]
mod outcome;

use std::path::Path;
use std::sync::Arc;

use defra_node::{Error, Node, NodeBuilder};
use outcome::Result;
use serde_json::{json, Value as JsonValue};

/// A node over `path` holding one `Users` document.
async fn node_with_a_user(path: &Path) -> Result<Arc<Node>> {
    let node = NodeBuilder::default().data_path(path).build().await?;
    node.add_collection(
        &defra_core::RequestContext::ambient(),
        "type Users { name: String }",
    )
    .await?;
    let created = node
        .execute(r#"mutation { create_Users(input: {name: "John"}) { name } }"#)
        .await;
    assert!(
        created.errors.is_empty(),
        "create failed: {:?}",
        created.errors
    );
    Ok(Arc::new(node))
}

/// The `Users` documents a node opened again over `path` reads.
async fn users_after_reopen(path: &Path) -> Result<JsonValue> {
    let reopened = NodeBuilder::default().data_path(path).build().await?;
    let users = reopened.execute("query { Users { name } }").await;
    reopened.shutdown().await?;
    Ok(users
        .data
        .and_then(|data| data.get("Users").cloned())
        .unwrap_or(JsonValue::Null))
}

#[test]
fn a_node_another_handle_holds_is_shut_down_but_not_wiped() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let node = node_with_a_user(dir.path()).await?;
            let other = Arc::clone(&node);

            let wiped = node.shutdown_and_wipe().await;
            assert!(
                matches!(wiped, Err(Error::WipeWhileHeld)),
                "a wipe under another handle must be refused, got {wiped:?}"
            );
            assert!(other.is_shutdown(), "the teardown still ran");
            drop(other);

            assert_eq!(
                users_after_reopen(dir.path()).await?,
                json!([{ "name": "John" }]),
                "a refused wipe removed the store"
            );
            Ok(())
        })
        .unwrap()
}

#[test]
fn a_node_another_caller_shut_down_is_not_wiped() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let node = node_with_a_user(dir.path()).await?;
            node.shutdown().await?;

            let wiped = node.shutdown_and_wipe().await;
            assert!(
                matches!(wiped, Err(Error::WipeAfterShutdown)),
                "a wipe after another caller's shutdown must be refused, got {wiped:?}"
            );

            assert_eq!(
                users_after_reopen(dir.path()).await?,
                json!([{ "name": "John" }]),
                "a refused wipe removed the store"
            );
            Ok(())
        })
        .unwrap()
}

#[test]
fn the_last_handle_wipes_the_store() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let node = node_with_a_user(dir.path()).await?;

            node.shutdown_and_wipe().await?;

            assert_eq!(
                users_after_reopen(dir.path()).await?,
                JsonValue::Null,
                "the wiped store still holds the Users collection"
            );
            Ok(())
        })
        .unwrap()
}
