//! The key classes against the house rules' reads and writes, one family at
//! a time: the class each key gets, then what that class does to commits
//! through the adapter.

use std::str::FromStr;

use regolith::{KeyClass, KeyClassifier};

use super::DefraKeyClassifier;
use crate::corekv::{Error, Key, Store};
use crate::keys::{HeadstoreColPublishedKey, HeadstoreColSetKey, HeadstoreDocSetKey};
use crate::{RegolithStore, RegolithStoreOptions};

const CID: &str = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";

fn class(key: &[u8]) -> KeyClass {
    DefraKeyClassifier.classify(key)
}

/// A key of the headstore as the store writes it, behind the namespace byte.
fn headstore(key: &[u8]) -> Vec<u8> {
    [b"h".as_slice(), key].concat()
}

/// A head set is one ordinary key, whatever the width of the document's short
/// ID: a merge into it is blind and a read of it names no parts.
#[test]
fn head_set_keys_are_ordinary() {
    let cid = cid::Cid::from_str(CID).unwrap();
    for id in [1u64, 47, 239, 240, 287, 2287, 2288, u64::MAX] {
        assert_eq!(
            class(&headstore(&HeadstoreDocSetKey::new(id).bytes())),
            KeyClass::Ordinary
        );
    }
    for id in [0u32, 7, 42, u32::MAX] {
        assert_eq!(
            class(&headstore(&HeadstoreColSetKey::new(id).bytes())),
            KeyClass::Ordinary
        );
        assert_eq!(
            class(&headstore(&HeadstoreColPublishedKey::new(id, cid).bytes())),
            KeyClass::Ordinary
        );
    }
}

/// A key of the old head layout is not in any family: it stays ordinary,
/// like any key a family does not recognise.
#[test]
fn keys_of_the_old_head_layout_stay_ordinary() {
    for key in [
        b"h/d/\x01/field/cid".as_slice(),
        b"h/ds/\x01/field/parent/child",
        b"h/c/1/cid",
        b"h/cs/1/parent/child",
        b"h/d",
        b"h/different/",
    ] {
        assert_eq!(class(key), KeyClass::Ordinary, "{key:?}");
    }
}

/// Blocks are content addressed, the arrival journal is a log, and every
/// decision key is ordinary: one winner by design, or a blind write.
#[test]
fn every_other_family_has_its_declared_class() {
    assert_eq!(class(b"b\x01\x55\x00\x03abc"), KeyClass::ContentAddressed);
    for key in [
        b"s/arrival/7/head".as_slice(),
        b"s/arrival/7/row/00000000000000000001",
        b"s/arrival/7/doc/bae-1",
        b"d/vplog/7/3/0/head",
        b"d/vplog/7/3/0/row/00000000000000000001",
        b"/vplog/7/3/0/head",
    ] {
        assert_eq!(class(key), KeyClass::Log, "{key:?}");
    }
    for key in [
        b"".as_slice(),
        b"b",
        b"b\x01invalid",
        b"bm\x01\x55\x00\x03abc",
        b"s/arrival",
        b"d/vplog",
        b"h/p/\x01/0000000000000001/cid",
        b"h/p/applied",
        b"h/r/\x01/field/cid",
        b"h/f/users/email/cid",
        b"h/g/users/cid",
        b"d/d/collection/\x01",
        b"d/del/collection/\x01",
        b"d/v/collection/\x01",
        b"d/\x01/\x02/value",
        b"s/d/p/doc",
        b"s/seq/doc",
        b"a/zanzibar/7/rel/users/bae-1/owner/subject",
        b"a/zanzibar/counter",
        b"e\x01\x55\x00\x03abc",
        b"unknown",
    ] {
        assert_eq!(class(key), KeyClass::Ordinary, "{key:?}");
    }
}

/// Two writers of one head set each read it without naming parts and merge
/// into it: both commit, and neither read is validated.
#[test]
fn one_head_set_commutes() {
    workers::block_on(async {
        for key in [
            HeadstoreDocSetKey::new(1).bytes(),
            HeadstoreColSetKey::new(1).bytes(),
        ] {
            let key = headstore(&key);
            let store = RegolithStore::in_memory().unwrap();
            let operand = |name: &[u8]| {
                let mut write = crate::merge::head_set::Write::default();
                write.add(name, 1);
                write.operand()
            };
            let mut seed = store.new_txn(false).await.unwrap();
            seed.merge(&key, &operand(b"a")).await.unwrap();
            seed.commit().await.unwrap();
            let mut first = store.new_txn(false).await.unwrap();
            let mut second = store.new_txn(false).await.unwrap();
            for (txn, name) in [(&mut first, b"b"), (&mut second, b"c")] {
                assert!(txn.get_parts(&key, &[]).await.unwrap().is_some());
                txn.merge(&key, &operand(name)).await.unwrap();
            }
            first.commit().await.unwrap();
            second.commit().await.unwrap();
            let read = store.new_txn(true).await.unwrap();
            let set =
                crate::merge::head_set::Set::read(&read.get(&key).await.unwrap().unwrap()).unwrap();
            assert_eq!(set.live().count(), 3, "{key:?}");
        }
    })
    .unwrap()
}

async fn same_block_writes(options: RegolithStoreOptions) {
    let dir = tempfile::tempdir().unwrap();
    let store = RegolithStore::open_with_options(dir.path(), options).unwrap();
    let mut first = store.new_txn(false).await.unwrap();
    let mut second = store.new_txn(false).await.unwrap();
    for txn in [&mut first, &mut second] {
        assert!(txn.get(b"b\x01\x55\x00\x03abc").await.unwrap().is_none());
        txn.set(b"b\x01\x55\x00\x03abc", b"same block")
            .await
            .unwrap();
    }
    first.commit().await.unwrap();
    second.commit().await.unwrap();
}

/// Two creates of one block both commit, sealed at rest or not.
#[test]
fn identical_blocks_commit_with_engine_encryption() {
    workers::block_on(async {
        same_block_writes(RegolithStoreOptions::default().with_encryption_key([42; 32])).await;
        same_block_writes(RegolithStoreOptions::memory()).await;
    })
    .unwrap()
}

/// Decision keys have one winner.
#[test]
fn ordinary_decision_keys_have_one_winner() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        for key in [
            b"bm\x01\x55\x00\x03abc".as_slice(),
            b"h/p/applied",
            b"h/r/\x01/field/cid",
            b"h/rc/1/cid",
            b"s/d/p/doc",
            b"a/zanzibar/owner",
            b"d/del/doc",
        ] {
            let mut first = store.new_txn(false).await.unwrap();
            let mut second = store.new_txn(false).await.unwrap();
            for txn in [&mut first, &mut second] {
                assert!(txn.get(key).await.unwrap().is_none());
                txn.set(key, b"present").await.unwrap();
            }
            first.commit().await.unwrap();
            assert!(
                matches!(second.commit().await, Err(Error::TxnConflictDetails(_))),
                "{key:?}"
            );
        }
    })
    .unwrap()
}

/// A block's key names its bytes: two different values under one key are
/// refused, not merged.
#[test]
fn concurrent_different_block_bytes_are_rejected() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut first = store.new_txn(false).await.unwrap();
        let mut second = store.new_txn(false).await.unwrap();
        first.set(b"b\x01\x55\x00\x03abc", b"first").await.unwrap();
        second
            .set(b"b\x01\x55\x00\x03abc", b"second")
            .await
            .unwrap();
        first.commit().await.unwrap();
        assert!(matches!(second.commit().await, Err(Error::Backend(_))));
        let read = store.new_txn(true).await.unwrap();
        assert_eq!(
            read.get(b"b\x01\x55\x00\x03abc").await.unwrap().as_deref(),
            Some(b"first".as_slice())
        );
    })
    .unwrap()
}

/// A decision taken on a block being present conflicts with its deletion.
#[test]
fn deleting_a_block_invalidates_a_presence_decision() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"b\x01\x55\x00\x03abc", b"block").await.unwrap();
        seed.commit().await.unwrap();
        let mut reader = store.new_txn(false).await.unwrap();
        assert!(reader.get(b"b\x01\x55\x00\x03abc").await.unwrap().is_some());
        reader.set(b"decision", b"block exists").await.unwrap();
        let mut delete = store.new_txn(false).await.unwrap();
        delete.delete(b"b\x01\x55\x00\x03abc").await.unwrap();
        delete.commit().await.unwrap();
        assert!(matches!(
            reader.commit().await,
            Err(Error::TxnConflictDetails(_))
        ));
    })
    .unwrap()
}
