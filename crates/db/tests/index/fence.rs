//! The backfill fence against writers that race an index creation.
//!
//! The backfill indexes documents whose short ID is below a cutoff allocated
//! after the definition commits. Every interleaving of one writer with the
//! creation ends with the writer maintaining the index or conflicting; none
//! commits a document the index misses.

use db::{AutoCommitMutator, DbDocMutator, LensedAutoCommitFetcher};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::RapidHashSet;

use super::create::{by_name, create_user, entries_for, index_entries, open_with_collection};

fn named(name: &str) -> Document {
    let mut doc = Document::new(super::add::users_schema("Users", "v1", "col-users"));
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    doc
}

/// A create staged before the definition commits and committed after it
/// read the old definition in its transaction, so it conflicts.
#[test]
fn an_explicit_create_spanning_index_creation_conflicts() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("SpanCreate", "span-create-v1", "col-span-c").await;
        let writer = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        writer
            .create("SpanCreate", named("spanning"))
            .await
            .unwrap();

        let (fields, kind) = by_name();
        let index = db
            .create_index("SpanCreate", Some("by_name"), fields, kind)
            .await
            .unwrap();

        let error =
            writer.take_txn().await.unwrap().commit().await.expect_err(
                "a create on the old definition must not commit after the index exists",
            );
        assert!(error.is_txn_conflict(), "{error}");
        assert_eq!(index_entries(&db, "SpanCreate", index.id).await, 0);

        // The re-run sees the index and maintains it.
        create_user(
            &AutoCommitMutator::new(db.clone()),
            "SpanCreate",
            "spanning",
        )
        .await;
        assert_eq!(entries_for(&db, "SpanCreate", "spanning").await, 1);
    })
    .unwrap()
}

/// An update staged before the definition commits conflicts the same way,
/// and the index keeps the committed value.
#[test]
fn an_explicit_update_spanning_index_creation_conflicts() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("SpanUpdate", "span-update-v1", "col-span-u").await;
        let doc_id = create_user(&AutoCommitMutator::new(db.clone()), "SpanUpdate", "before").await;

        let writer = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut doc = writer
            .get_for_update("SpanUpdate", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("name", NormalValue::String("after".to_owned()))
            .unwrap();
        writer
            .update(
                "SpanUpdate",
                doc,
                RapidHashSet::from_iter(["name".to_owned()]),
            )
            .await
            .unwrap();

        let (fields, kind) = by_name();
        let index = db
            .create_index("SpanUpdate", Some("by_name"), fields, kind)
            .await
            .unwrap();

        let error =
            writer.take_txn().await.unwrap().commit().await.expect_err(
                "an update on the old definition must not commit after the index exists",
            );
        assert!(error.is_txn_conflict(), "{error}");
        assert_eq!(entries_for(&db, "SpanUpdate", "before").await, 1);
        assert_eq!(entries_for(&db, "SpanUpdate", "after").await, 0);
        assert_eq!(index_entries(&db, "SpanUpdate", index.id).await, 1);
    })
    .unwrap()
}

/// A transaction whose snapshot predates the definition, creating a document
/// whose short ID is allocated after the fence: the backfill does not cover
/// it, so the writer must never commit it unindexed.
#[test]
fn a_snapshot_before_the_definition_never_commits_above_the_fence_unindexed() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("SpanLate", "span-late-v1", "col-span-l").await;
        // A transaction pins its snapshot when it begins.
        let writer = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());

        let (fields, kind) = by_name();
        let index = db
            .create_index("SpanLate", Some("by_name"), fields, kind)
            .await
            .unwrap();

        let committed = match writer.create("SpanLate", named("late")).await {
            Ok(_) => writer.take_txn().await.unwrap().commit().await.is_ok(),
            Err(_) => false,
        };
        let docs = LensedAutoCommitFetcher::new(db.clone())
            .get_all("SpanLate")
            .await
            .unwrap();
        assert_eq!(docs.len(), usize::from(committed));
        assert_eq!(index_entries(&db, "SpanLate", index.id).await, docs.len());
        assert!(
            !committed,
            "a writer that read the old definition cannot commit after the new one"
        );
    })
    .unwrap()
}
