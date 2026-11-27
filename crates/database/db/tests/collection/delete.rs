//! Tests for the multi-collection delete orchestrator (Go #4688 parity).

use db::database::DB;
use storage::RegolithStore;

const USERS: &str = "type Users { name: String }";
const USERS_AND_BOOKS: &str = "type Users { name: String } type Books { title: String }";
const RELATED: &str = r#"
    type Users {
        books: [Books] @relation(name: "user_books")
    }
    type Books {
        owner: Users @relation(name: "user_books") @primary
    }
"#;

#[test]
fn delete_collections_removes_single_name() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), USERS)
            .await
            .unwrap();

        db.delete_collections(vec!["Users".into()], true)
            .await
            .unwrap();

        assert!(
            db.get_collection("Users").unwrap().is_none(),
            "Users collection should be gone after delete_collections"
        );
    })
    .unwrap()
}

#[test]
fn delete_collections_removes_multiple_names_atomically() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_AND_BOOKS)
            .await
            .unwrap();

        db.delete_collections(vec!["Users".into(), "Books".into()], true)
            .await
            .unwrap();

        assert!(
            db.get_collection("Users").unwrap().is_none(),
            "Users collection should be gone"
        );
        assert!(
            db.get_collection("Books").unwrap().is_none(),
            "Books collection should be gone"
        );
    })
    .unwrap()
}

#[test]
fn delete_collections_errors_on_empty_names() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let err = db
            .delete_collections(vec![], true)
            .await
            .expect_err("expected error for empty names");

        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("name") || msg.contains("empty") || msg.contains("required"),
            "unexpected error message: {err}"
        );
    })
    .unwrap()
}

#[test]
fn delete_collections_rejects_empty_name_without_deleting_valid_names() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), USERS)
            .await
            .unwrap();

        let err = db
            .delete_collections(vec!["Users".into(), String::new()], true)
            .await
            .expect_err("an empty name must reject the entire request");

        assert!(err.to_string().contains("collection name can't be empty"));
        assert!(db.get_collection("Users").unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn delete_collections_errors_on_unknown_name() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), USERS)
            .await
            .unwrap();

        let err = db
            .delete_collections(vec!["Users".into(), "Ghost".into()], true)
            .await
            .expect_err("expected error for unknown collection name");

        let msg = err.to_string();
        assert!(
            msg.contains("Ghost") || msg.to_lowercase().contains("not found"),
            "unexpected error message: {err}"
        );

        // Atomicity: because Ghost failed before any state changed, Users must still exist.
        assert!(
            db.get_collection("Users").unwrap().is_some(),
            "Users should not have been deleted when batch validation failed"
        );
    })
    .unwrap()
}

#[test]
fn delete_collections_dedupes_repeated_names() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), USERS)
            .await
            .unwrap();

        db.delete_collections(vec!["Users".into(), "Users".into()], true)
            .await
            .expect("duplicate names should be deduplicated, not error");

        assert!(
            db.get_collection("Users").unwrap().is_none(),
            "Users collection should be gone"
        );
    })
    .unwrap()
}

#[test]
fn delete_collections_rejects_dangling_relation_targets() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), RELATED)
            .await
            .unwrap();

        let err = db
            .delete_collections(vec!["Users".into()], true)
            .await
            .expect_err("a referenced collection must not be deleted alone");

        assert!(err
            .to_string()
            .contains("cannot remove a collection while another field references it"));
        assert!(db.get_collection("Users").unwrap().is_some());
        assert!(db.get_collection("Books").unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn delete_collections_allows_deleting_both_sides_of_relation() {
    workers::block_on(async {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        db.add_collection(&defra_core::RequestContext::ambient(), RELATED)
            .await
            .unwrap();

        db.delete_collections(vec!["Users".into(), "Books".into()], true)
            .await
            .unwrap();

        assert!(db.get_collection("Users").unwrap().is_none());
        assert!(db.get_collection("Books").unwrap().is_none());
    })
    .unwrap()
}
