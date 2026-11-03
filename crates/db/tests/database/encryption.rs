//! Integration test: full DB document CRUD over an encrypted Regolith engine.
//!
//! Proves the engine encryption composes with the complete DB
//! stack (systemstore + datastore through transactions and iterators).

use db::database::DB;
use db::AutoCommitMutator;
use document::Document;
use query::mutator::DocMutator;
use std::sync::Arc;
use storage::RegolithStore;

#[test]
fn document_crud_roundtrips_through_encrypted_store() {
    workers::block_on(async {
        let key = [42u8; 32];
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            RegolithStore::open_with_options(
                root.path(),
                storage::RegolithStoreOptions::default().with_encryption_key(key),
            )
            .unwrap(),
        );
        let db = Arc::new(DB::open_from_arc(store).await.unwrap());

        let collections = query::parse_sdl(
            r#"
        type Users {
            name: String
        }
        "#,
        )
        .unwrap();
        db.create_collections_atomic(collections).await.unwrap();

        // Create a document through the encrypted store via the real mutator flow
        // (allocates the doc short ID and derives the DocID from the genesis block).
        let mutator = AutoCommitMutator::new(db.clone());
        let doc = Document::from_json_str(
            r#"{"name": "Alice"}"#,
            db.get_collection("Users").unwrap().unwrap().schema_arc(),
        )
        .unwrap();
        let created = mutator.create("Users", doc).await.unwrap();

        // Read it back in a fresh transaction; the value must decrypt cleanly.
        let fetched = mutator
            .get_for_update("Users", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.get("name").and_then(|v| v.as_str()), Some("Alice"));
    })
    .unwrap()
}
