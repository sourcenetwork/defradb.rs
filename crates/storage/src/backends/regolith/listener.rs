//! What the engine reports, counted.
//!
//! regolith tells its listeners about every flush, compaction and table
//! file, each background failure, each conflict, a transaction callback that
//! panicked after its outcome, and a write-ahead-log tail an open dropped.
//! This listener counts each one as `defradb.storage.events` by kind (with
//! the bytes it moved), conflicts by reason as well, and says out loud the
//! two a person should look at: a callback panic and a dropped log tail.
//! regolith logs a background failure itself, so that one is only counted.

use regolith::{
    BackgroundErrorReason, CompactionJobInfo, Conflict, Error, EventListener,
    ExternalFileIngestionInfo, FlushJobInfo, TableFileCreationInfo, TableFileDeletionInfo,
    WalTailDiscardedInfo,
};
use telemetry::StorageEvent;

use crate::backends::shared::TransactionStatsHandle;

pub(super) struct StoreListener(pub(super) TransactionStatsHandle);

impl EventListener for StoreListener {
    fn on_flush_completed(&self, info: &FlushJobInfo) {
        telemetry::record_storage_event(StorageEvent::Flush, info.file_size);
    }

    fn on_compaction_begin(&self, _info: &CompactionJobInfo) {
        telemetry::record_storage_event(StorageEvent::CompactionBegin, 0);
    }

    fn on_compaction_completed(&self, _info: &CompactionJobInfo) {
        telemetry::record_storage_event(StorageEvent::Compaction, 0);
    }

    fn on_table_file_created(&self, info: &TableFileCreationInfo) {
        telemetry::record_storage_event(StorageEvent::TableFileCreated, info.file_size);
    }

    fn on_table_file_deleted(&self, _info: &TableFileDeletionInfo) {
        telemetry::record_storage_event(StorageEvent::TableFileDeleted, 0);
    }

    fn on_external_file_ingested(&self, info: &ExternalFileIngestionInfo) {
        telemetry::record_storage_event(StorageEvent::Ingest, info.file_size);
    }

    fn on_background_error(&self, reason: BackgroundErrorReason, _err: &Error) {
        telemetry::record_storage_background_error("regolith", reason_label(reason));
    }

    fn on_conflict(&self, conflict: &Conflict) {
        self.0.record_conflict(conflict);
    }

    fn on_callback_panic(&self, callback: &'static str) {
        super::post_commit::report(StorageEvent::PostCommitCallbackPanicked, &callback);
    }

    fn on_wal_tail_discarded(&self, info: &WalTailDiscardedInfo) {
        tracing::warn!(
            path = %info.file_path.display(),
            offset = info.offset,
            discarded_bytes = info.discarded_bytes,
            last_sequence = info.last_sequence,
            "Dropped an unusable write-ahead log tail at open; writes up to last_sequence are intact"
        );
        telemetry::record_storage_event(StorageEvent::WalTailDiscarded, info.discarded_bytes);
    }
}

fn reason_label(reason: BackgroundErrorReason) -> &'static str {
    match reason {
        BackgroundErrorReason::Flush => "flush",
        BackgroundErrorReason::Compaction => "compaction",
        BackgroundErrorReason::Manifest => "manifest",
        BackgroundErrorReason::WriteAheadLog => "wal",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_background_failure_is_counted() {
        let listener = StoreListener(TransactionStatsHandle::from_engine(Default::default()));
        let before = telemetry::storage_background_error_count();
        let err = Error::from(std::io::Error::other("Too many open files (os error 24)"));
        listener.on_background_error(BackgroundErrorReason::Compaction, &err);
        listener.on_background_error(BackgroundErrorReason::Flush, &err);
        assert!(telemetry::storage_background_error_count() >= before + 2);
    }

    #[test]
    fn every_reason_has_a_distinct_label() {
        let labels = [
            BackgroundErrorReason::Flush,
            BackgroundErrorReason::Compaction,
            BackgroundErrorReason::Manifest,
            BackgroundErrorReason::WriteAheadLog,
        ]
        .map(reason_label);
        let unique: std::collections::BTreeSet<_> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len());
    }

    #[test]
    fn a_callback_panic_is_counted_as_post_commit_work_that_failed() {
        let listener = StoreListener(TransactionStatsHandle::from_engine(Default::default()));
        let before = telemetry::storage_event_count(StorageEvent::PostCommitCallbackPanicked);
        listener.on_callback_panic("on_commit");
        assert!(telemetry::storage_event_count(StorageEvent::PostCommitCallbackPanicked) > before);
    }
}

#[cfg(test)]
mod listener_tests {
    use crate::backends::RegolithStore;
    use crate::corekv::Store;
    use telemetry::StorageEvent;

    /// A store's flushes and table files reach the listener it opens with.
    #[test]
    fn a_flush_and_its_table_file_are_counted() {
        workers::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let store = RegolithStore::open(directory.path()).unwrap();
            let flushes = telemetry::storage_event_count(StorageEvent::Flush);
            let tables = telemetry::storage_event_count(StorageEvent::TableFileCreated);
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(b"row", b"value").await.unwrap();
            txn.commit().await.unwrap();
            store.engine().db().flush().unwrap();
            assert!(telemetry::storage_event_count(StorageEvent::Flush) > flushes);
            assert!(telemetry::storage_event_count(StorageEvent::TableFileCreated) > tables);
            store.close().await.unwrap();
        })
        .unwrap()
    }
}
