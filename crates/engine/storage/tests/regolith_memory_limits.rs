//! An in-memory store accepts what an on-disk one accepts.
//!
//! `Options::memory()` is sized from the engine's embedded profile, whose
//! 256 KiB value ceiling is a limit a caller can hit rather than a
//! memory-tuning knob. Adopting it wholesale made the in-memory store refuse
//! documents the on-disk store took, which is a different database rather
//! than a faster one. These pin the two to the same limits.

use storage::corekv::{Reader, Store, Writer};
use storage::{RegolithStore, RegolithStoreOptions};

#[test]
fn the_memory_profile_keeps_the_server_size_limits() {
    workers::block_on(async {
        let server = RegolithStoreOptions::new();
        let memory = RegolithStoreOptions::memory();

        assert_eq!(
            memory.engine.get_max_value_size(),
            server.engine.get_max_value_size()
        );
        assert_eq!(
            memory.engine.get_max_key_size(),
            server.engine.get_max_key_size()
        );
    })
    .unwrap()
}

/// The environment constraints still come from the engine's preset, which is
/// the reason to use it at all: `MemEnv` cannot spawn a compaction worker.
#[test]
fn the_memory_profile_keeps_the_environment_constraints() {
    workers::block_on(async {
        let memory = RegolithStoreOptions::memory();

        assert_eq!(memory.engine.get_max_background_compactions(), 0);
        assert!(!memory.engine.get_env().capabilities().threads);
        assert!(memory.engine.get_inline_compaction());
    })
    .unwrap()
}

#[test]
fn an_in_memory_store_round_trips_a_value_larger_than_the_embedded_ceiling() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        // Comfortably past the 256 KiB the embedded profile allows.
        let value = vec![0xAB; 1024 * 1024];

        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"large", &value).await.unwrap();
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        assert_eq!(
            txn.get(b"large").await.unwrap().as_deref(),
            Some(value.as_slice())
        );
    })
    .unwrap()
}

/// The no-worker profile must keep making progress as memtables fill and
/// L0 reaches its compaction threshold, without caller retries.
#[test]
fn memory_commits_continue_across_flush_and_compaction() {
    workers::block_on(async {
        let mut options = RegolithStoreOptions::memory();
        options.engine = options
            .engine
            .write_buffer_size(4 * 1024)
            .l0_compaction_trigger(2)
            .level0_slowdown_writes_trigger(0)
            .level0_stop_writes_trigger(4);
        let store = RegolithStore::open_with_options("memory-compaction", options).unwrap();
        let value = vec![0xCD; 1024];

        for index in 0u32..400 {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(&index.to_be_bytes(), &value).await.unwrap();
            txn.commit().await.unwrap();
        }

        let txn = store.new_txn(true).await.unwrap();
        for index in 0u32..400 {
            assert_eq!(
                txn.get(&index.to_be_bytes()).await.unwrap().as_deref(),
                Some(value.as_slice())
            );
        }
    })
    .unwrap()
}
