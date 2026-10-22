//! Stores the previous release wrote, opened by this one.
//!
//! The fixtures (see `fixtures/README.md`) were written by main's code: one
//! store through its per-value encryption wrapper, one without encryption,
//! with the same collections and documents. Opening the wrapped one rewrites
//! it once into a store the engine seals; opening either gives every
//! collection part IDs and puts its documents in the field format. Every
//! query follows the current document visibility contract.

use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/outcome.rs"]
mod outcome;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use document::stored::StoredDocument;
use embedded::{EmbeddedNode, EmbeddedStore, NodeBuilder};
use outcome::Result;
use serde_json::Value;
use storage::corekv::{IterOptions, Reader, Store};
use storage::{RegolithStore, RegolithStoreOptions};

/// The key main's wrapper encrypted `legacy_wrapped` with.
const KEY: [u8; 32] = [0x5a; 32];

fn materialize(directory: &Path, fixture: &str) -> Result<PathBuf> {
    let encoded = match fixture {
        "wrapped" => include_str!("fixtures/legacy_wrapped.regolith.tar.zst.b64"),
        _ => include_str!("fixtures/legacy_plain.regolith.tar.zst.b64"),
    };
    let encoded: String = encoded.split_whitespace().collect();
    let compressed = BASE64_STANDARD.decode(encoded).expect("decode fixture");
    let archive = zstd::stream::decode_all(compressed.as_slice()).expect("decompress fixture");
    let path = directory.join(fixture);
    fs::create_dir(&path).expect("create fixture directory");
    tar::Archive::new(archive.as_slice())
        .unpack(&path)
        .expect("unpack fixture");
    Ok(path)
}

/// The original stores remain unchanged. Expected reads follow current
/// visibility: `showDeleted` includes Notes even though the previous writer's
/// live-only index query omitted that retained row.
fn expected() -> Result<Vec<(String, Value)>> {
    let answers: Value = serde_json::from_str(include_str!("fixtures/legacy_expected.json"))?;
    answers
        .as_array()
        .expect("expected answers")
        .iter()
        .map(|answer| {
            Ok((
                answer["query"].as_str().expect("query").to_owned(),
                answer["data"].clone(),
            ))
        })
        .collect()
}

async fn assert_expected_reads(node: &EmbeddedNode<EmbeddedStore>) -> Result<()> {
    for (query, data) in expected()? {
        let response = node.execute(&query).await;
        assert!(response.errors.is_empty(), "{query}: {:?}", response.errors);
        assert!(
            response.data.as_ref() == Some(&data),
            "{query} answered {:?}, expected {data}",
            response.data
        );
    }
    Ok(())
}

async fn open(path: &Path, key: Option<[u8; 32]>) -> Result<EmbeddedNode<EmbeddedStore>> {
    let mut builder = NodeBuilder::default().data_path(path);
    if let Some(key) = key {
        builder = builder.with_at_rest_encryption_key(key);
    }
    builder.build().await.map_err(Into::into)
}

/// Raw store contents, read with the store's own key if it has one.
async fn raw_entries(path: &Path, key: Option<[u8; 32]>) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let options = match key {
        Some(key) => RegolithStoreOptions::embedded().with_encryption_key(key),
        None => RegolithStoreOptions::embedded(),
    };
    let store = RegolithStore::open_with_options(path, options)?;
    let txn = store.new_txn(true).await?;
    let mut iter = txn.iterator(IterOptions::new()).await?;
    let mut entries = Vec::new();
    while let Some(pair) = iter.next().await? {
        entries.push((pair.key.to_vec(), pair.value.to_vec()));
    }
    iter.close().await?;
    drop(txn);
    store.close().await?;
    Ok(entries)
}

/// Every collection version has part IDs and every document is in the field
/// format, so updates merge field operands and reads can name the fields.
fn assert_field_format(entries: &[(Vec<u8>, Vec<u8>)]) -> Result<()> {
    let documents: Vec<_> = entries
        .iter()
        .filter(|(key, _)| key.starts_with(b"d/d/"))
        .collect();
    assert!(documents.len() == 6, "found {} documents", documents.len());
    assert!(
        documents
            .iter()
            .all(|(_, value)| StoredDocument::is_field_format(value)),
        "a document is still a whole CBOR value"
    );
    let versions = entries
        .iter()
        .filter(|(key, _)| key.starts_with(b"s/collection/id/"))
        .count();
    let parts = entries
        .iter()
        .filter(|(key, _)| key.starts_with(b"s/parts/"))
        .count();
    assert!(
        versions == 2 && parts == versions,
        "{parts} part maps for {versions} versions"
    );
    Ok(())
}

fn files_contain(path: &Path, needle: &[u8]) -> Result<bool> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            if files_contain(&path, needle)? {
                return Ok(true);
            }
        } else if fs::read(&path)?
            .windows(needle.len())
            .any(|window| window == needle)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[test]
fn a_wrapped_store_of_the_previous_release_is_rewritten_and_reads_the_same() -> Result<()> {
    workers::block_on(async {
    let directory = tempfile::tempdir()?;
    let path = materialize(directory.path(), "wrapped")?;

    let node = open(&path, Some(KEY)).await?;
    assert_expected_reads(&node).await?;
    let created = node
        .execute(r#"mutation { add_Book(input: {title: "Rust", pages: 560}) { title } }"#)
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let updated = node
        .execute(r#"mutation { update_Author(filter: {name: {_eq: "Grace"}}, input: {age: 86, points: 4}) { name age points } }"#)
        .await;
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    assert!(
        updated
            .data
            .as_ref()
            .map(|data| &data["update_Author"][0]["points"])
            == Some(&Value::from(26)),
        "{:?}",
        updated.data
    );
    node.shutdown().await;
    drop(node);

    // Nothing of the old store is left beside it, and its files are sealed.
    let names: Vec<_> = fs::read_dir(directory.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<_>>()?;
    assert!(names.len() == 1, "left beside the store: {names:?}");
    assert!(!files_contain(&path, b"Grace")?, "plaintext on disk");

    // A second open rewrites nothing and still reads the same.
    let node = open(&path, Some(KEY)).await?;
    let book = node
        .execute(r#"query { Book(filter: {title: {_eq: "Rust"}}) { pages } }"#)
        .await;
    assert!(
        book.data.as_ref().map(|data| &data["Book"][0]["pages"]) == Some(&Value::from(560)),
        "{:?}",
        book.data
    );
    node.shutdown().await;
    drop(node);

    let entries = raw_entries(&path, Some(KEY)).await?;
    assert!(
        entries
            .iter()
            .any(|(key, _)| key == b"s/system/at_rest_rewrite"),
        "the rewrite marker is missing"
    );
    let documents = entries
        .iter()
        .filter(|(key, _)| key.starts_with(b"d/d/"))
        .count();
    assert!(documents == 7, "found {documents} documents");
    Ok(())
})
    .unwrap()
}

#[test]
fn previous_release_documents_get_part_ids_and_field_operands() -> Result<()> {
    workers::block_on(async {
        let directory = tempfile::tempdir()?;
        for (fixture, key) in [("plain", None), ("wrapped", Some(KEY))] {
            let path = materialize(directory.path(), fixture)?;
            let node = open(&path, key).await?;
            assert_expected_reads(&node).await?;
            node.shutdown().await;
            drop(node);
            let entries = raw_entries(&path, key).await?;
            assert_field_format(&entries)?;
            assert!(
                entries
                    .iter()
                    .any(|(key, _)| key == b"s/system/document_format"),
                "{fixture}: the document format stamp is missing"
            );
            // Opening again changes nothing and reads the same.
            let node = open(&path, key).await?;
            assert_expected_reads(&node).await?;
            node.shutdown().await;
            drop(node);
            assert!(
                raw_entries(&path, key).await? == entries,
                "{fixture}: a second open changed the store"
            );
        }
        Ok(())
    })
    .unwrap()
}

#[test]
fn a_wrapped_store_opened_without_its_key_is_refused_not_served() -> Result<()> {
    workers::block_on(async {
        let directory = tempfile::tempdir()?;
        let path = materialize(directory.path(), "wrapped")?;
        for (key, message) in [
            (None, "open it with at-rest encryption on"),
            (Some([0x11; 32]), "none of the configured at-rest keys"),
        ] {
            let error = match open(&path, key).await {
                Ok(node) => {
                    node.shutdown().await;
                    panic!("a wrapped store opened with {key:?}")
                }
                Err(error) => format!("{error:#}"),
            };
            assert!(error.contains(message), "unexpected refusal: {error}");
        }
        // The refusals left the store as it was: its own key still opens it.
        let node = open(&path, Some(KEY)).await?;
        assert_expected_reads(&node).await?;
        node.shutdown().await;
        Ok(())
    })
    .unwrap()
}
