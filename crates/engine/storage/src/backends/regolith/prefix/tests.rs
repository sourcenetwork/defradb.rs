//! The prefix extractor against regolith's contract, and what it does to the
//! store's prefix scans.

use std::sync::Arc;

use proptest::prelude::*;
use regolith::{OptimisticTransactionDb, PrefixExtractor, Statistics, Ticker, TxnOptions};

use super::super::families;
use super::super::handle::Handle;
use super::super::iterator::RegolithIterator;
use super::DefraPrefixExtractor;
use crate::corekv::{IterOptions, Iterator};
use crate::RegolithStoreOptions;

proptest! {
    /// regolith's contract for `extract_query`: a table that holds a key
    /// with the prefix holds that key's extracted prefix, which is the
    /// query's. And what is extracted is a prefix of the key.
    #[test]
    fn a_query_names_the_entity_of_every_key_that_extends_it(key in families::keys()) {
        let extracted = DefraPrefixExtractor.extract(&key);
        if let Some(extracted) = extracted {
            prop_assert!(key.starts_with(extracted));
        }
        for cut in 0..=key.len() {
            if let Some(query) = DefraPrefixExtractor.extract_query(&key[..cut]) {
                prop_assert_eq!(extracted, Some(query));
            }
        }
    }
}

/// The name a table records: stable, and naming this extractor alone.
#[test]
fn the_extractor_is_named_for_its_cuts() {
    assert_eq!(DefraPrefixExtractor.name(), "defradb-entities-v2");
    assert_ne!(
        DefraPrefixExtractor.name(),
        regolith::FixedLengthPrefix(4).name()
    );
}

async fn scan(handle: &Arc<Handle>, options: &IterOptions) -> Vec<Vec<u8>> {
    let mut iter = RegolithIterator::open(Arc::clone(handle), options).unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        keys.push(pair.key);
    }
    keys
}

/// A read-only scan of a prefix reads exactly what a transaction's scan of
/// it reads, which is every stored key with the prefix, and passes over the
/// tables whose prefix filter lacks the entity: here, documents, collections,
/// indexes and ACP objects that have no keys between ones that do.
#[test]
fn a_prefix_scan_reads_what_it_did_and_skips_tables_without_its_entity() {
    workers::block_on(async {
        let statistics = Arc::new(Statistics::new());
        let options = RegolithStoreOptions::memory()
            .engine_for_open()
            .statistics(Some(Arc::clone(&statistics)));
        let db = OptimisticTransactionDb::open("prefix-filters", options).unwrap();
        let mut keys: Vec<Vec<u8>> = Vec::new();
        for entity in [1u8, 3] {
            let digit = b'0' + entity;
            keys.extend([
                [b"h/r/".as_slice(), &[entity], b"/field/a"].concat(),
                [b"h/r/".as_slice(), &[entity], b"/field/b"].concat(),
                [b"h/p/".as_slice(), &[entity], b"/0000000000000001/a"].concat(),
                [b"d/d/col".as_slice(), &[digit], b"/\x01"].concat(),
                [b"d/\x01/".as_slice(), &[entity], b"/value/\x01"].concat(),
                [
                    b"a/zanzibar/1/rel/users/bae-".as_slice(),
                    &[digit],
                    b"/owner/x",
                ]
                .concat(),
            ]);
        }
        // A key that names no entity, between the others.
        keys.push(b"h/p/".to_vec());
        keys.sort();
        for key in &keys {
            db.db().put(key, key).unwrap();
        }
        db.db().flush().unwrap();

        let snapshot = Arc::new(Handle::ReadOnly(db.db().snapshot()));
        let transaction = Arc::new(Handle::Writable(Box::new(db.begin(&TxnOptions::new()))));
        let mut prefixes: Vec<Vec<u8>> = Vec::new();
        for entity in [1u8, 2, 3] {
            let digit = b'0' + entity;
            prefixes.extend([
                [b"h/r/".as_slice(), &[entity], b"/"].concat(),
                [b"h/r/".as_slice(), &[entity], b"/field/"].concat(),
                [b"h/p/".as_slice(), &[entity], b"/"].concat(),
                [b"d/d/col".as_slice(), &[digit], b"/"].concat(),
                [b"d/\x01/".as_slice(), &[entity], b"/value/"].concat(),
                [b"a/zanzibar/1/rel/users/bae-".as_slice(), &[digit], b"/"].concat(),
            ]);
        }
        // Prefixes that stop short of an entity, or name none.
        prefixes.extend([b"h/p/".to_vec(), b"h/".to_vec(), b"d/".to_vec(), Vec::new()]);
        prefixes.push(vec![0xff, 0xff]);
        for prefix in prefixes {
            let options = IterOptions::new().with_prefix(prefix.clone());
            let expected: Vec<Vec<u8>> = keys
                .iter()
                .filter(|key| key.starts_with(&prefix))
                .cloned()
                .collect();
            assert_eq!(scan(&snapshot, &options).await, expected, "{prefix:?}");
            assert_eq!(scan(&transaction, &options).await, expected, "{prefix:?}");
        }

        let before = statistics.get_ticker(Ticker::BloomFilterUseful);
        for absent in [
            b"h/r/\x02/".as_slice(),
            b"h/p/\x02/",
            b"d/d/col2/",
            b"d/\x01/\x02/",
            b"a/zanzibar/1/rel/users/bae-2/",
        ] {
            let options = IterOptions::new().with_prefix(absent.to_vec());
            assert!(scan(&snapshot, &options).await.is_empty(), "{absent:?}");
        }
        assert!(
            statistics.get_ticker(Ticker::BloomFilterUseful) > before,
            "no table was passed over by its prefix filter"
        );
    })
    .unwrap()
}
