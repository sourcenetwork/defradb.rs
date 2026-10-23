use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use regolith::{Db, Options};

use super::wrapped::seal;
use super::*;
use crate::backends::RegolithTxn;
use crate::corekv::{IterOptions, Reader, Store};
use crate::RegolithStore;

const KEY: [u8; 32] = [0x5a; 32];
const OTHER_KEY: [u8; 32] = [0x6b; 32];

type Entries = BTreeMap<Vec<u8>, Vec<u8>>;

fn stamp_key() -> Vec<u8> {
    Namespace::Systemstore.prefix_key(INDEX_FORMAT_KEY)
}

/// What a store of the previous release holds, in plain: the index format
/// stamp, small values under every namespace, an empty value, and enough
/// large values to fill more than one copy batch.
fn entries() -> Entries {
    let mut entries = Entries::new();
    entries.insert(stamp_key(), 2u32.to_be_bytes().to_vec());
    for namespace in *b"abdhps" {
        for i in 0u32..50 {
            let key = [&[namespace][..], b"/k/", &i.to_be_bytes()].concat();
            entries.insert(
                key,
                format!("value {i} of {}", namespace as char).into_bytes(),
            );
        }
    }
    entries.insert(b"d/empty".to_vec(), Vec::new());
    for i in 0u8..40 {
        entries.insert([b"d/large/", &[i][..]].concat(), vec![i; 128 << 10]);
    }
    entries
}

/// Writes `entries` the way the previous release did: every value sealed by
/// its wrapper under `key`, into a store the engine does not seal. Half the
/// entries reach a table, the rest stay in the log.
fn write_legacy(path: &Path, key: &[u8; 32], entries: &Entries) {
    let db = Db::open(path, Options::default()).unwrap();
    for (i, (stored_key, value)) in entries.iter().enumerate() {
        db.put(stored_key, &seal(key, stored_key, value)).unwrap();
        if i == entries.len() / 2 {
            db.flush().unwrap();
        }
    }
    db.close().unwrap();
}

fn keyed(key: [u8; 32]) -> RegolithStoreOptions {
    RegolithStoreOptions::default().with_encryption_key(key)
}

async fn read_all(store: &RegolithStore) -> Entries {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn.iterator(IterOptions::new()).await.unwrap();
    let mut entries = Entries::new();
    while let Some(pair) = iter.next().await.unwrap() {
        entries.insert(pair.key.to_vec(), pair.value.to_vec());
    }
    entries
}

/// The store at `path` holds `expected` plus the rewrite marker, and nothing
/// the rewrite uses is left beside it.
async fn assert_rewritten(path: &Path, options: RegolithStoreOptions, expected: &Entries) {
    let store = RegolithStore::open_with_options(path, options).unwrap();
    let mut found = read_all(&store).await;
    let marker = found.remove(&marker_key()).expect("the rewrite marker");
    assert_eq!(marker, (expected.len() as u64).to_be_bytes());
    assert!(found == *expected, "the rewritten store reads differently");
    store.close().await.unwrap();
    let names = Names::new(path).unwrap();
    for leftover in [&names.copy, &names.legacy, &names.lock] {
        assert!(!leftover.exists(), "{} was left behind", leftover.display());
    }
}

/// The store at `path` is the previous release's, untouched: same stamp
/// bytes, every value still sealed by the wrapper.
fn assert_legacy(path: &Path, key: &[u8; 32], expected: &Entries) {
    let db = Db::open_read_only(path, Options::default()).unwrap();
    let mut seen = 0;
    for entry in db.iter() {
        let (stored_key, value) = entry.unwrap();
        let opener = WrapperKey::find([key], &stored_key, &value).expect("a wrapper value");
        let plain = opener.open(&stored_key, &value).unwrap();
        assert_eq!(expected.get(&stored_key), Some(&plain));
        seen += 1;
    }
    assert_eq!(seen, expected.len());
}

/// Opens with a power cut at `cut`: the open panics there, so nothing after
/// that point runs, as when a machine loses power.
fn cut_at(path: &Path, options: &RegolithStoreOptions, cut: Step) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        open_with_steps(path, options.engine_with_keys(), options, &mut |step| {
            if step == cut {
                panic!("power cut at {step:?}");
            }
        })
    }));
    assert!(outcome.is_err(), "the open did not reach {cut:?}");
}

fn store_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("store")
}

#[test]
fn a_wrapper_store_is_rewritten_at_its_first_open() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let expected = entries();
        write_legacy(&path, &KEY, &expected);
        assert_rewritten(&path, keyed(KEY), &expected).await;
        // The rewritten store needs no second rewrite and keeps no plaintext.
        assert_rewritten(&path, keyed(KEY), &expected).await;
        assert!(RegolithStore::open(&path).is_err());
    })
    .unwrap()
}

/// A power cut at each step of the rewrite. Right after it the disk holds the old store or the checked copy at `P` or
/// `P.legacy`; the next open finishes or restarts the rewrite and reads every
/// value. A cut between a rename and its sync either loses the rename (the
/// state of the step before) or keeps it (the state of the step itself), so
/// the cuts below cover both outcomes.
#[test]
fn a_power_cut_at_every_step_leaves_the_old_store_or_the_checked_copy() {
    workers::block_on(async {
        let expected = entries();
        for cut in [
            Step::Copying,
            Step::Copied,
            Step::Verified,
            Step::MovedAside,
            Step::MovedIn,
            Step::Retired,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = store_path(&dir);
            let names = Names::new(&path).unwrap();
            write_legacy(&path, &KEY, &expected);
            cut_at(&path, &keyed(KEY), cut);

            match cut {
                Step::Copying | Step::Copied | Step::Verified => {
                    assert!(names.copy.exists());
                    assert_legacy(&path, &KEY, &expected);
                }
                Step::MovedAside => {
                    assert!(!path.exists());
                    assert_legacy(&names.legacy, &KEY, &expected);
                }
                Step::MovedIn => {
                    assert_legacy(&names.legacy, &KEY, &expected);
                    // The deletion of the old store may itself be cut half way.
                    std::fs::remove_dir_all(names.legacy.join("sst")).unwrap();
                }
                Step::Retired => assert!(!names.legacy.exists()),
            }
            if cut == Step::Copied {
                // A copy whose files were only partly written.
                std::fs::remove_file(names.copy.join("MANIFEST")).unwrap();
            }
            assert_rewritten(&path, keyed(KEY), &expected).await;
        }
    })
    .unwrap()
}

#[test]
fn a_wrong_copy_is_caught_and_the_store_left_unchanged() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let names = Names::new(&path).unwrap();
        let expected = entries();
        write_legacy(&path, &KEY, &expected);
        let options = keyed(KEY);
        let error = open_with_steps(&path, options.engine_with_keys(), &options, &mut |step| {
            if step == Step::Copied {
                let copy = Db::open(&names.copy, options.engine_with_keys()).unwrap();
                copy.put(b"d/k/\0\0\0\x01", b"a copy bug").unwrap();
                copy.close().unwrap();
            }
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("does not match"), "{error}");
        assert!(!names.copy.exists());
        assert_legacy(&path, &KEY, &expected);
        assert_rewritten(&path, keyed(KEY), &expected).await;
    })
    .unwrap()
}

#[test]
fn a_value_the_wrapper_did_not_write_stops_the_rewrite() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let names = Names::new(&path).unwrap();
        write_legacy(&path, &KEY, &entries());
        let db = Db::open(&path, Options::default()).unwrap();
        db.put(b"d/plain", b"not sealed").unwrap();
        db.close().unwrap();
        drop(db);
        let error = RegolithStore::open_with_options(&path, keyed(KEY))
            .err()
            .unwrap();
        assert!(error.to_string().contains("does not decrypt"), "{error}");
        assert!(!names.copy.exists());
        let stamp = Db::open_read_only(&path, Options::default())
            .unwrap()
            .get(&stamp_key())
            .unwrap()
            .unwrap();
        assert_eq!(stamp.len(), 4 + wrapped::OVERHEAD);
    })
    .unwrap()
}

#[test]
fn a_wrapper_store_without_its_key_is_refused_and_left_unchanged() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let expected = entries();
        write_legacy(&path, &KEY, &expected);
        for (options, message) in [
            (
                RegolithStoreOptions::default(),
                "open it with at-rest encryption on",
            ),
            (keyed(OTHER_KEY), "none of the configured at-rest keys"),
        ] {
            let error = RegolithStore::open_with_options(&path, options)
                .err()
                .unwrap();
            assert!(error.to_string().contains(message), "{error}");
        }
        let error = RegolithTxn::open_read_only(&path, keyed(KEY))
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("open it read-write once"),
            "{error}"
        );
        assert_legacy(&path, &KEY, &expected);
    })
    .unwrap()
}

/// A store written before a key rotation: the wrapper's key is now an older
/// key, the copy is sealed under the current one, and once rewritten the
/// store opens with the current key alone.
#[test]
fn an_older_key_opens_a_store_written_before_a_rotation() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let expected = entries();
        write_legacy(&path, &OTHER_KEY, &expected);
        let rotated = RegolithStoreOptions::default()
            .with_encryption_keys(KEY, [OTHER_KEY])
            .unwrap();
        assert_rewritten(&path, rotated, &expected).await;
        assert_rewritten(&path, keyed(KEY), &expected).await;
        assert!(RegolithStore::open_with_options(&path, keyed(OTHER_KEY)).is_err());
    })
    .unwrap()
}

/// The engine's own key rotation: files sealed under the older key stay
/// readable while new files are sealed under the current one.
#[test]
fn a_sealed_store_reads_its_older_files_after_a_rotation() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let store = RegolithStore::open_with_options(&path, keyed(OTHER_KEY)).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"before", b"rotation").await.unwrap();
        txn.commit().await.unwrap();
        store.close().await.unwrap();
        drop(store);

        let rotated = || {
            RegolithStoreOptions::default()
                .with_encryption_keys(KEY, [OTHER_KEY])
                .unwrap()
        };
        let store = RegolithStore::open_with_options(&path, rotated()).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"after", b"rotation").await.unwrap();
        txn.commit().await.unwrap();
        store.close().await.unwrap();
        drop(store);

        // The newer files name the current key, so the older key alone is not enough.
        assert!(RegolithStore::open_with_options(&path, keyed(OTHER_KEY)).is_err());
        let store = RegolithStore::open_with_options(&path, rotated()).unwrap();
        let found = read_all(&store).await;
        assert_eq!(
            found.get(&b"before"[..]).map(Vec::as_slice),
            Some(&b"rotation"[..])
        );
        assert_eq!(
            found.get(&b"after"[..]).map(Vec::as_slice),
            Some(&b"rotation"[..])
        );
    })
    .unwrap()
}

#[test]
fn a_rewrite_already_running_elsewhere_refuses_the_open() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let names = Names::new(&path).unwrap();
        let expected = entries();
        write_legacy(&path, &KEY, &expected);
        let env = regolith::env::std_env();
        let held = RewriteLock::take(&names, &*env).unwrap();
        let error = RegolithStore::open_with_options(&path, keyed(KEY))
            .err()
            .unwrap();
        assert!(error.to_string().contains("another program"), "{error}");
        assert_legacy(&path, &KEY, &expected);
        drop(held);
        assert_rewritten(&path, keyed(KEY), &expected).await;
    })
    .unwrap()
}

/// A power cut between the renames, then a program that knows nothing of the rewrite makes an empty store at
/// `P`. The old store at `P.legacy` is the only copy of the data and must not
/// be deleted.
#[test]
fn an_old_store_beside_a_store_that_is_not_its_rewrite_is_kept() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let names = Names::new(&path).unwrap();
        let expected = entries();
        write_legacy(&path, &KEY, &expected);
        cut_at(&path, &keyed(KEY), Step::MovedAside);
        let fresh = Db::open(&path, Options::default()).unwrap();
        fresh.close().unwrap();
        drop(fresh);

        let error = RegolithStore::open_with_options(&path, keyed(KEY))
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("found an earlier store"),
            "{error}"
        );
        assert_legacy(&names.legacy, &KEY, &expected);
    })
    .unwrap()
}

#[test]
fn a_copy_with_no_store_beside_it_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_path(&dir);
    let names = Names::new(&path).unwrap();
    std::fs::create_dir(&names.copy).unwrap();
    let error = RegolithStore::open_with_options(&path, keyed(KEY))
        .err()
        .unwrap();
    assert!(error.to_string().contains("unfinished rewrite"), "{error}");
    assert!(names.copy.exists());
}

/// A store the previous release wrote without at-rest encryption is plain
/// already: it opens as it is, with or without a key, and is not rewritten.
#[test]
fn a_plain_store_of_the_previous_release_is_not_rewritten() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let expected = entries();
        let db = Db::open(&path, Options::default()).unwrap();
        for (key, value) in &expected {
            db.put(key, value).unwrap();
        }
        db.close().unwrap();
        drop(db);
        for options in [RegolithStoreOptions::default(), keyed(KEY)] {
            let store = RegolithStore::open_with_options(&path, options).unwrap();
            assert!(read_all(&store).await == expected);
            store.close().await.unwrap();
        }
    })
    .unwrap()
}

/// A wrapper store whose database never wrote its stamp is found by its
/// first entry when keys are configured.
#[test]
fn a_wrapper_store_without_the_stamp_is_found_by_its_first_entry() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(&dir);
        let mut expected = entries();
        expected.remove(&stamp_key());
        write_legacy(&path, &KEY, &expected);
        assert_rewritten(&path, keyed(KEY), &expected).await;
    })
    .unwrap()
}

/// Times the rewrite of 256 MiB of 4 KiB values and prints its rate. Run with
/// `REWRITE_RATE_DIR=<a folder on the disk to measure> cargo test -p storage
/// --lib -- --ignored --nocapture rewrite_rate`.
#[test]
#[ignore]
fn rewrite_rate() {
    workers::block_on(async {
        let dir = match std::env::var_os("REWRITE_RATE_DIR") {
            Some(base) => tempfile::tempdir_in(base).unwrap(),
            None => tempfile::tempdir().unwrap(),
        };
        let path = store_path(&dir);
        let db = Db::open(&path, Options::default()).unwrap();
        let value = vec![7u8; 4 << 10];
        let count = (256u64 << 20) / value.len() as u64;
        for i in 0..count {
            let key = [&b"d/"[..], &i.to_be_bytes()].concat();
            db.put(&key, &seal(&KEY, &key, &value)).unwrap();
        }
        db.close().unwrap();
        drop(db);
        let started = std::time::Instant::now();
        let store = RegolithStore::open_with_options(&path, keyed(KEY)).unwrap();
        let elapsed = started.elapsed();
        store.close().await.unwrap();
        println!(
            "rewrote {count} values, {} MiB of plaintext, in {elapsed:?}: {:.1} MiB/s",
            256,
            256.0 / elapsed.as_secs_f64()
        );
    })
    .unwrap()
}
