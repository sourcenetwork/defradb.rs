use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use cid::Cid;
use embedded::{EmbeddedNode, EmbeddedStore, NodeBuilder};
use outcome::Result;
use serde_json::{json, Value};

const OLD_TASK_1_ID: &str = "bae-7513a70e-b156-5803-b492-017180bd20b8";
const LAST_LEGACY_TASK_ID: &str = "bae-aba20f79-3f40-5f9e-a2e3-849786094ac9";
const TASK_COLLECTION_ID: &str = "bafyreia5e7ttg5pzmgkokb7fafom5nqb5tajlc443p5fmbzsb4646lw334";
const LAST_LEGACY_COMPOSITE_HEAD: &str =
    "bafyreidz66r3ozihn5gum475v5s7evufcjy4fewvywv6n24eh3x5vncwae";
const MIGRATION_MARKER: &[u8] = b"/migration/doc-short-id/v1";

/// Unpack the v0.15 store into a disposable directory.
///
/// A regolith store is a directory, so the fixture is a tar rather than the
/// single file the Redb-era one was.
fn materialize_fixture(directory: &Path) -> Result<PathBuf> {
    let encoded: String = include_str!("fixtures/v015_populated.regolith.tar.zst.b64")
        .split_whitespace()
        .collect();
    let compressed = BASE64_STANDARD
        .decode(encoded)
        .expect("decode v0.15 fixture base64");
    let archive =
        zstd::stream::decode_all(compressed.as_slice()).expect("decompress v0.15 fixture");
    let path = directory.join("v015_store");
    fs::create_dir(&path).expect("create disposable v0.15 store directory");
    tar::Archive::new(archive.as_slice())
        .unpack(&path)
        .expect("unpack disposable v0.15 store");
    Ok(path)
}

async fn execute_ok(node: &EmbeddedNode<EmbeddedStore>, query: &str) -> Result<Value> {
    let response = node.execute(query).await;
    assert!(
        response.errors.is_empty(),
        "query failed: {:?}",
        response.errors
    );
    Ok(response.data.expect("successful query returned no data"))
}

#[test]
fn migrates_populated_v015_store_and_reopens_idempotently() -> Result<()> {
    workers::block_on(async {
    let directory = tempfile::tempdir()?;
    let path = materialize_fixture(directory.path())?;

    let node = NodeBuilder::default().data_path(&path).build().await?;
    let documents = execute_ok(
        &node,
        r#"query { Task(order: {task_id: ASC}) { _docID task_id status note } }"#,
    )
    .await?;
    assert_eq!(
        documents,
        json!({
            "Task": [
                {
                    "_docID": "bae-cfe4febb-cee3-541d-afe7-86d76da576b5",
                    "task_id": "task-1",
                    "status": "running",
                    "note": "updated"
                },
                {
                    "_docID": "bae-7f630507-5c0f-511d-a448-df2927da12e0",
                    "task_id": "task-2",
                    "status": "complete",
                    "note": "second"
                },
                {
                    "_docID": "bae-6f2358f8-d095-55a1-b572-35fe6864ac4a",
                    "task_id": "task-3",
                    "status": "failed",
                    "note": "third"
                }
            ]
        })
    );

    let indexed = execute_ok(
        &node,
        r#"query { Task(filter: {task_id: {_eq: "task-1"}}) { _docID task_id status note } }"#,
    )
    .await?;
    assert_eq!(indexed["Task"].as_array().map(Vec::len), Some(1));
    assert_eq!(indexed["Task"][0]["note"], "updated");

    let commits = execute_ok(
        &node,
        &format!(
            r#"query {{ _commits(docID: "{OLD_TASK_1_ID}") {{ cid docID height fieldName }} }}"#
        ),
    )
    .await?;
    assert_eq!(commits["_commits"].as_array().map(Vec::len), Some(7));
    assert!(
        commits["_commits"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|commit| commit["docID"] == "bae-cfe4febb-cee3-541d-afe7-86d76da576b5"),
        "old DocID alias did not resolve to the canonical commit history"
    );

    let duplicate = node
        .execute(
            r#"mutation { create_Task(input: {task_id: "task-1", status: "duplicate", note: "must fail"}) { _docID } }"#,
        )
        .await;
    assert!(
        duplicate.has_errors(),
        "unique-index duplicate was accepted"
    );
    assert!(
        duplicate
            .errors
            .iter()
            .any(|error| error.message.to_ascii_lowercase().contains("unique")),
        "duplicate failed for an unexpected reason: {:?}",
        duplicate.errors
    );

    let created = execute_ok(
        &node,
        r#"mutation { create_Task(input: {task_id: "task-4", status: "pending", note: "post-migration"}) { task_id status note } }"#,
    )
    .await?;
    assert_eq!(
        created["add_Task"][0]["task_id"], "task-4",
        "unexpected create response: {created}"
    );

    let updated = execute_ok(
        &node,
        r#"mutation { update_Task(filter: {task_id: {_eq: "task-2"}}, input: {status: "verified"}) { task_id status } }"#,
    )
    .await?;
    assert_eq!(updated["update_Task"][0]["status"], "verified");
    node.shutdown().await;
    drop(node);

    let reopened = NodeBuilder::default().data_path(&path).build().await?;
    let after_reopen = execute_ok(
        &reopened,
        r#"query { Task(order: {task_id: ASC}) { task_id status note } }"#,
    )
    .await?;
    assert_eq!(after_reopen["Task"].as_array().map(Vec::len), Some(4));
    assert_eq!(after_reopen["Task"][1]["task_id"], "task-2");
    assert_eq!(after_reopen["Task"][1]["status"], "verified");
    assert_eq!(after_reopen["Task"][3]["task_id"], "task-4");
    reopened.shutdown().await;

    Ok(())
})
    .unwrap()
}

#[test]
fn malformed_v015_store_rolls_back_the_entire_migration() -> Result<()> {
    workers::block_on(async {
        let directory = tempfile::tempdir()?;
        let path = materialize_fixture(directory.path())?;

        let store = Arc::new(storage::RegolithStore::open(
            path.to_str().expect("fixture path contains non-UTF-8"),
        )?);
        {
            let database = db::DB::from_arc(Arc::clone(&store))?;
            let txn = database.new_txn(false).await?;
            let cid = LAST_LEGACY_COMPOSITE_HEAD.parse::<Cid>()?;
            txn.blockstore()?.delete(&cid.to_bytes()).await?;
            txn.commit().await?;
        }

        let error = match db::DB::open_from_arc(Arc::clone(&store)).await {
            Ok(database) => {
                database.close().await?;
                panic!("migration unexpectedly accepted a missing legacy block")
            }
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("references missing block"),
            "migration failed for an unexpected reason: {error:#}"
        );

        let database = db::DB::from_arc(store)?;
        let txn = database.new_txn(true).await?;
        {
            let systemstore = txn.systemstore()?;
            assert!(
                !systemstore.has(MIGRATION_MARKER).await?,
                "failed migration published its completion marker"
            );
            assert!(
                !systemstore.has(b"/d/s/\x01").await?,
                "failed migration leaked a short-ID mapping"
            );
            assert!(
                !systemstore.has(b"/seq/doc").await?,
                "failed migration advanced the short-ID sequence"
            );
            let legacy_key = format!("/d/{TASK_COLLECTION_ID}/{LAST_LEGACY_TASK_ID}");
            assert!(
                txn.datastore()?.has(legacy_key.as_bytes()).await?,
                "failed migration deleted a legacy document"
            );
        }
        txn.discard()?;
        database.close().await?;

        Ok(())
    })
    .unwrap()
}
