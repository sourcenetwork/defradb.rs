use cid::Cid;
use db::block::heads::document::{commit, iterator, load, publish};
use storage::corekv::{IterOptions, Key};
use storage::keys::headstore::HeadstoreDocKey;
use storage::merge::head_set::Write;
use storage::RegolithStore;

use db::block::heads::document::{APPLIED, PUBLISHED};
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta};
use storage::keys::headstore::HeadstorePriorityKey;

const DOC: u64 = 47; // The binary short ID contains a slash.
fn cid(value: &[u8]) -> Cid {
    defra_core::block::generate_cid_from_bytes(value).unwrap()
}
fn head(cid: Cid) -> HeadstoreDocKey {
    HeadstoreDocKey::new(DOC, "name", cid)
}
/// Publish `cid` over `parents` in `txn`, as a replicated block does.
async fn publish_head(txn: &db::DbTxn<RegolithStore>, cid: Cid, priority: u64, parents: &[Cid]) {
    let view = txn.headstore().unwrap();
    let set = load(&view, DOC).await.unwrap();
    let mut write = Write::default();
    publish(&view, &set, &mut write, &head(cid), priority, parents)
        .await
        .unwrap();
    commit(&view, DOC, &write).await.unwrap();
}
async fn live(view: &datastore::NamespaceView) -> Vec<Cid> {
    let mut iter = iterator(
        view,
        IterOptions::new().with_prefix(HeadstoreDocKey::document_prefix(DOC)),
    )
    .await
    .unwrap();
    let mut result = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        result.push(HeadstoreDocKey::parse(&pair.key).unwrap().cid);
    }
    iter.close().await.unwrap();
    result.sort_by_cached_key(Cid::to_string);
    result
}
async fn ahead(view: &datastore::NamespaceView) -> usize {
    load(view, DOC).await.unwrap().ahead().count()
}

/// Two writers that both saw the parent each publish a child: both commit, a
/// snapshot taken before keeps the parent, and the parent is no head after.
#[test]
fn overlapping_document_heads_commit_and_snapshots_are_preserved() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, left, right) = (cid(b"parent"), cid(b"left"), cid(b"right"));
        let seed = db.new_txn(false).await.unwrap();
        publish_head(&seed, parent, 1, &[]).await;
        seed.commit().await.unwrap();
        let old = db.new_txn(true).await.unwrap();
        let a = db.new_txn(false).await.unwrap();
        let b = db.new_txn(false).await.unwrap();
        for (txn, child) in [(&a, left), (&b, right)] {
            assert_eq!(live(&txn.headstore().unwrap()).await, vec![parent]);
            publish_head(txn, child, 2, &[parent]).await;
        }
        b.commit().await.unwrap();
        a.commit().await.unwrap();
        let mut expected = vec![left, right];
        expected.sort_by_cached_key(Cid::to_string);
        assert_eq!(live(&old.headstore().unwrap()).await, vec![parent]);
        let after = db.new_txn(true).await.unwrap();
        let view = after.headstore().unwrap();
        assert_eq!(live(&view).await, expected);
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

/// A child that arrives before its parent hides the parent when it arrives,
/// and leaves nothing behind.
#[test]
fn a_child_before_its_parent_hides_the_late_parent() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, child) = (cid(b"late parent"), cid(b"early child"));
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, child, 2, &[parent]).await;
        txn.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        assert_eq!(live(&txn.headstore().unwrap()).await, vec![child]);
        assert_eq!(ahead(&txn.headstore().unwrap()).await, 1);
        txn.discard().unwrap();
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, parent, 1, &[]).await;
        txn.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, vec![child]);
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

#[test]
fn replay_of_a_superseded_parent_cannot_resurrect_it() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, child) = (cid(b"published parent"), cid(b"published child"));
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, parent, 1, &[]).await;
        publish_head(&txn, child, 2, &[parent]).await;
        txn.commit().await.unwrap();
        // A replay may already own an older snapshot when the child commits.
        let replay = db.new_txn(false).await.unwrap();
        publish_head(&replay, parent, 1, &[]).await;
        replay.commit().await.unwrap();
        let replay = db.new_txn(false).await.unwrap();
        publish_head(&replay, parent, 1, &[]).await;
        replay.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        assert_eq!(live(&txn.headstore().unwrap()).await, vec![child]);
    })
    .unwrap()
}

/// A writer that supersedes a published parent concurrently with the write
/// that superseded it first: a fork never conflicts, and the head set never
/// holds the parent or a name ahead.
#[test]
fn a_late_sibling_of_a_superseded_parent_forks_without_conflict() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, left, right) = (
            cid(b"shared parent"),
            cid(b"left child"),
            cid(b"right child"),
        );
        let seed = db.new_txn(false).await.unwrap();
        publish_head(&seed, parent, 1, &[]).await;
        seed.commit().await.unwrap();

        let late = db.new_txn(false).await.unwrap();
        let early = db.new_txn(false).await.unwrap();
        publish_head(&early, left, 2, &[parent]).await;
        early.commit().await.unwrap();
        publish_head(&late, right, 2, &[parent]).await;
        late.commit().await.expect("a fork never conflicts");

        let mut expected = vec![left, right];
        expected.sort_by_cached_key(Cid::to_string);
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, expected);
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

/// The view over a head set answers the same live heads in either direction
/// and after a seek, for every pattern of superseded heads across fields.
#[test]
fn live_heads_agree_forward_reverse_and_after_seek() {
    workers::block_on(async {
        let fields = ["C", "name", "score"];
        let heads: Vec<HeadstoreDocKey> = fields
            .iter()
            .flat_map(|field| {
                (0..2u8)
                    .map(move |i| HeadstoreDocKey::new(DOC, *field, cid(&[i, field.len() as u8])))
            })
            .collect();
        for pattern in 0u32..(1 << heads.len()) {
            let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
            let txn = db.new_txn(false).await.unwrap();
            {
                let view = txn.headstore().unwrap();
                let mut write = Write::default();
                for (index, head) in heads.iter().enumerate() {
                    let name = db::block::heads::document::name(&head.field_id, &head.cid);
                    write.add(&name, 1);
                    if pattern & (1 << index) != 0 {
                        write.supersede(&name);
                    }
                }
                commit(&view, DOC, &write).await.unwrap();
            }
            txn.commit().await.unwrap();

            let mut expected: Vec<Vec<u8>> = heads
                .iter()
                .enumerate()
                .filter(|(index, _)| pattern & (1 << index) == 0)
                .map(|(_, head)| head.bytes())
                .collect();
            expected.sort();
            let txn = db.new_txn(true).await.unwrap();
            let view = txn.headstore().unwrap();
            for reverse in [false, true] {
                let options = IterOptions::new()
                    .with_prefix(HeadstoreDocKey::document_prefix(DOC))
                    .with_reverse(reverse);
                let mut iter = iterator(&view, options.clone()).await.unwrap();
                let mut walked = Vec::new();
                while let Some(pair) = iter.next().await.unwrap() {
                    walked.push(pair.key);
                }
                if reverse {
                    walked.reverse();
                }
                assert_eq!(walked, expected, "pattern {pattern:#08b} reverse {reverse}");

                // Seek to every stored head: the walk resumes on the first live
                // head at or past it, in the iteration's direction.
                for target in &heads {
                    let target = target.bytes();
                    let mut iter = iterator(&view, options.clone()).await.unwrap();
                    let found = iter.seek(&target).await.unwrap();
                    let got = iter.next().await.unwrap().map(|pair| pair.key);
                    let want = if reverse {
                        expected.iter().rev().find(|key| **key <= target).cloned()
                    } else {
                        expected.iter().find(|key| **key >= target).cloned()
                    };
                    assert_eq!(
                        found,
                        want.is_some(),
                        "pattern {pattern:#08b} reverse {reverse}"
                    );
                    assert_eq!(got, want, "pattern {pattern:#08b} reverse {reverse}");
                }
            }
        }
    })
    .unwrap()
}

/// The names stored in every document's head set, and the head keys of the
/// earlier layout still stored (none).
async fn stored(db: &db::DB<RegolithStore>) -> (usize, usize) {
    let txn = db.new_txn(true).await.unwrap();
    let view = txn.headstore().unwrap();
    let (mut names, mut legacy) = (0, 0);
    let mut sets = view
        .iterator(IterOptions::new().with_prefix(b"/hd/".to_vec()))
        .await
        .unwrap();
    while let Some(pair) = sets.next().await.unwrap() {
        let set = storage::merge::head_set::Set::read(&pair.value).unwrap();
        names += set.live().count() + set.ahead().count();
    }
    sets.close().await.unwrap();
    for prefix in [b"/d/".as_slice(), b"/ds/"] {
        let mut iter = view
            .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
            .await
            .unwrap();
        while iter.next().await.unwrap().is_some() {
            legacy += 1;
        }
        iter.close().await.unwrap();
    }
    (names, legacy)
}

/// Autocommit and explicit-transaction updates both keep a document's set to
/// its live heads, however many times it is written.
#[test]
fn every_write_path_keeps_a_documents_stored_heads_to_its_live_heads() {
    workers::block_on(async {
        use query::mutator::DocMutator;
        const WRITES: i64 = 24;
        let db = std::sync::Arc::new(db::DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(schema::CollectionVersion::new(
            "Rewritten",
            "rewritten-v1",
            "col-rewritten",
            vec![
                schema::FieldDescription::new("1", "_docID", schema::FieldKind::doc_id()),
                schema::FieldDescription::new("2", "value", schema::FieldKind::int()),
            ],
        ))
        .await
        .unwrap();
        let autocommit = db::AutoCommitMutator::new(db.clone());
        let doc_id = autocommit
            .create(
                "Rewritten",
                document::Document::from_json_str(r#"{"value": 0}"#).unwrap(),
            )
            .await
            .unwrap()
            .doc_id;
        let modified = || rapidhash::RapidHashSet::from_iter(["value".to_string()]);
        for value in 1..=WRITES {
            let mut doc = autocommit
                .get_for_update("Rewritten", &doc_id)
                .await
                .unwrap()
                .unwrap();
            doc.set("value", document::NormalValue::Int(value));
            autocommit
                .update("Rewritten", doc, modified())
                .await
                .unwrap();
        }
        // One composite head and one head for the field.
        assert_eq!(stored(&db).await, (2, 0));

        for value in WRITES + 1..=2 * WRITES {
            let explicit = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
            let mut doc = explicit
                .get_for_update("Rewritten", &doc_id)
                .await
                .unwrap()
                .unwrap();
            doc.set("value", document::NormalValue::Int(value));
            explicit.update("Rewritten", doc, modified()).await.unwrap();
            explicit.take_txn().await.unwrap().commit().await.unwrap();
        }
        assert_eq!(stored(&db).await, (2, 0));
    })
    .unwrap()
}

/// Two updates of one document, each of a different field, both commit: their
/// head set writes are blind merges, and the live set is both new composite
/// heads and both new field heads.
#[test]
fn concurrent_updates_of_one_document_both_commit() {
    workers::block_on(async {
        use query::mutator::DocMutator;
        let db = std::sync::Arc::new(db::DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(schema::CollectionVersion::new(
            "Pair",
            "pair-v1",
            "col-pair",
            vec![
                schema::FieldDescription::new("1", "_docID", schema::FieldKind::doc_id()),
                schema::FieldDescription::new("2", "left", schema::FieldKind::int()),
                schema::FieldDescription::new("3", "right", schema::FieldKind::int()),
            ],
        ))
        .await
        .unwrap();
        let doc_id = db::AutoCommitMutator::new(db.clone())
            .create(
                "Pair",
                document::Document::from_json_str(r#"{"left": 0, "right": 0}"#).unwrap(),
            )
            .await
            .unwrap()
            .doc_id;
        // Read outside the writers: `get_for_update` validates every field, which
        // would make the writers conflict on the document value, not on the heads.
        let reader = db::DbDocMutator::new(db.clone(), db.new_txn(true).await.unwrap());
        let current = reader
            .get_for_update("Pair", &doc_id)
            .await
            .unwrap()
            .unwrap();
        let mut writers = Vec::new();
        for (field, value) in [("left", 1), ("right", 2)] {
            let writer = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
            let mut doc = current.clone();
            doc.set(field, document::NormalValue::Int(value));
            writer
                .update(
                    "Pair",
                    doc,
                    rapidhash::RapidHashSet::from_iter([field.to_string()]),
                )
                .await
                .unwrap();
            writers.push(writer);
        }
        for writer in writers {
            writer.take_txn().await.unwrap().commit().await.unwrap();
        }
        // Two composite heads, and one new head for each updated field.
        assert_eq!(stored(&db).await, (4, 0));
    })
    .unwrap()
}

/// Holding a parent ahead is a decision that the parent is unpublished: it
/// reads the parent's guard validated, so it cannot commit once the parent was
/// published underneath it. The retry then supersedes the published parent, and
/// no name is left ahead.
#[test]
fn holding_a_parent_ahead_conflicts_with_its_publication() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, child) = (cid(b"racing parent"), cid(b"racing child"));
        let child_txn = db.new_txn(false).await.unwrap();
        publish_head(&child_txn, child, 2, &[parent]).await;
        let parent_txn = db.new_txn(false).await.unwrap();
        publish_head(&parent_txn, parent, 1, &[]).await;
        parent_txn.commit().await.unwrap();
        assert!(child_txn.commit().await.is_err());

        let retry = db.new_txn(false).await.unwrap();
        publish_head(&retry, child, 2, &[parent]).await;
        retry.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, vec![child]);
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

fn sorted(mut cids: Vec<Cid>) -> Vec<Cid> {
    cids.sort_by_cached_key(Cid::to_string);
    cids
}

/// A block at `priority`, stored as a replicated block is before it is
/// published.
async fn stored_block(db: &db::DB<RegolithStore>, seed: &str, priority: u64) -> Cid {
    let bytes = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: seed.to_string(),
            priority,
            status: 1,
        }),
        vec![],
        vec![],
    )
    .to_dag_cbor()
    .unwrap();
    let cid = defra_core::block::generate_cid_from_bytes(&bytes).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    txn.blockstore()
        .unwrap()
        .set(&cid.to_bytes(), &bytes)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    cid
}

/// Two publications of one head race a child that supersedes it. The head's
/// priority index entry is read validated, so the later publication cannot
/// commit once the first has, and its retry leaves the head superseded.
#[test]
fn a_replay_racing_its_childs_supersede_conflicts_and_never_resurrects() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (parent, child) = (cid(b"raced parent"), cid(b"raced child"));
        let first = db.new_txn(false).await.unwrap();
        let replay = db.new_txn(false).await.unwrap();
        publish_head(&first, parent, 1, &[]).await;
        publish_head(&replay, parent, 1, &[]).await;
        first.commit().await.unwrap();
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, child, 2, &[parent]).await;
        txn.commit().await.unwrap();
        assert!(
            replay.commit().await.is_err(),
            "a replay that found no record cannot commit after the head was published"
        );

        let retry = db.new_txn(false).await.unwrap();
        publish_head(&retry, parent, 1, &[]).await;
        retry.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, vec![child]);
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

/// A child holds its parent ahead while the parent is published and a sibling
/// supersedes it. The hold read the parent's part of the set, which both of
/// those operands touch, so it cannot commit; its retry finds the parent
/// published and supersedes it, and no name stays ahead.
#[test]
fn a_hold_on_a_parent_published_and_superseded_meanwhile_conflicts() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let parent = stored_block(&db, "held parent", 1).await;
        let (early, late) = (cid(b"early child"), cid(b"late sibling"));
        let held = db.new_txn(false).await.unwrap();
        publish_head(&held, early, 2, &[parent]).await;
        for (head, priority, parents) in [(parent, 1, &[][..]), (late, 2, &[parent][..])] {
            let txn = db.new_txn(false).await.unwrap();
            publish_head(&txn, head, priority, parents).await;
            txn.commit().await.unwrap();
        }
        assert!(
            held.commit().await.is_err(),
            "a hold on a parent published since cannot commit"
        );

        let retry = db.new_txn(false).await.unwrap();
        publish_head(&retry, early, 2, &[parent]).await;
        retry.commit().await.unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, sorted(vec![early, late]));
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

/// A child holding its parent ahead commits past an unrelated head published
/// meanwhile: the hold read only the parent's part of the set. The set holds
/// a value first, since a read that found no set decided on its absence,
/// which any merge into it ends.
#[test]
fn an_unrelated_publication_does_not_turn_away_a_hold() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let (seed, parent, child, other) = (
            cid(b"seed head"),
            cid(b"missing parent"),
            cid(b"holding child"),
            cid(b"unrelated head"),
        );
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, seed, 1, &[]).await;
        txn.commit().await.unwrap();
        let held = db.new_txn(false).await.unwrap();
        publish_head(&held, child, 2, &[parent]).await;
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, other, 1, &[]).await;
        txn.commit().await.unwrap();
        held.commit()
            .await
            .expect("an operand that names other heads does not touch the parent's part");

        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, sorted(vec![seed, child, other]));
        assert_eq!(ahead(&view).await, 1);
    })
    .unwrap()
}

/// A late sibling of a parent published and superseded before it began: the
/// parent's stored block and priority index entry say it was published, so
/// the sibling supersedes it and holds nothing ahead.
#[test]
fn a_late_sibling_of_a_superseded_parent_holds_nothing_ahead() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let parent = stored_block(&db, "published parent", 1).await;
        let (first, second) = (cid(b"first child"), cid(b"second child"));
        for (head, priority, parents) in [
            (parent, 1, &[][..]),
            (first, 2, &[parent][..]),
            (second, 2, &[parent][..]),
        ] {
            let txn = db.new_txn(false).await.unwrap();
            publish_head(&txn, head, priority, parents).await;
            txn.commit().await.unwrap();
        }
        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, sorted(vec![first, second]));
        assert_eq!(ahead(&view).await, 0);
    })
    .unwrap()
}

/// A counter merged on its own leaves its priority index entry saying the
/// delta was applied, not that the head was published: the composite that
/// links it still publishes the head, and the entry then says so.
#[test]
fn a_counter_applied_before_its_head_is_still_published() {
    workers::block_on(async {
        let db = db::DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        let field = cid(b"counter block");
        let entry = HeadstorePriorityKey::new(DOC, 1, field).bytes();
        let txn = db.new_txn(false).await.unwrap();
        txn.headstore().unwrap().set(&entry, APPLIED).await.unwrap();
        txn.commit().await.unwrap();
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, field, 1, &[]).await;
        txn.commit().await.unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        assert_eq!(live(&view).await, vec![field]);
        assert_eq!(view.get(&entry).await.unwrap().as_deref(), Some(PUBLISHED));
    })
    .unwrap()
}

/// No write path stores a publication guard: local creates and updates and a
/// replicated publication record a head's publication in its priority index
/// entry alone.
#[test]
fn no_write_path_stores_a_publication_guard() {
    workers::block_on(async {
        use query::mutator::DocMutator;
        let db = std::sync::Arc::new(db::DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(schema::CollectionVersion::new(
            "Guarded",
            "guarded-v1",
            "col-guarded",
            vec![
                schema::FieldDescription::new("1", "_docID", schema::FieldKind::doc_id()),
                schema::FieldDescription::new("2", "value", schema::FieldKind::int()),
            ],
        ))
        .await
        .unwrap();
        let autocommit = db::AutoCommitMutator::new(db.clone());
        let doc_id = autocommit
            .create(
                "Guarded",
                document::Document::from_json_str(r#"{"value": 0}"#).unwrap(),
            )
            .await
            .unwrap()
            .doc_id;
        for value in 1..=3 {
            let mut doc = autocommit
                .get_for_update("Guarded", &doc_id)
                .await
                .unwrap()
                .unwrap();
            doc.set("value", document::NormalValue::Int(value));
            autocommit
                .update(
                    "Guarded",
                    doc,
                    rapidhash::RapidHashSet::from_iter(["value".to_string()]),
                )
                .await
                .unwrap();
        }
        let (parent, child) = (cid(b"replicated parent"), cid(b"replicated child"));
        let txn = db.new_txn(false).await.unwrap();
        publish_head(&txn, parent, 1, &[]).await;
        publish_head(&txn, child, 2, &[parent]).await;
        txn.commit().await.unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let view = txn.headstore().unwrap();
        let mut guards = view
            .iterator(IterOptions::new().with_prefix(b"/r/".to_vec()))
            .await
            .unwrap();
        assert!(guards.next().await.unwrap().is_none(), "a guard was stored");
        guards.close().await.unwrap();
    })
    .unwrap()
}
