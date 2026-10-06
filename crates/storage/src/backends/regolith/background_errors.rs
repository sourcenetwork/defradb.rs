//! Report regolith's background failures.
//!
//! regolith keeps running when a flush, compaction, manifest write or WAL
//! write fails in the background, and retries later. A failure that never
//! clears (an exhausted file-descriptor table, a full disk) is then visible
//! only as a repeated log line inside the engine, while the store slowly
//! wedges. This listener turns each one into a
//! `defradb.storage.background.errors` metric labelled with its reason.
//! regolith logs the failure itself, so this only counts it.

use regolith::{BackgroundErrorReason, Error, EventListener};

#[derive(Debug, Default)]
pub(super) struct BackgroundErrorListener;

impl EventListener for BackgroundErrorListener {
    fn on_background_error(&self, reason: BackgroundErrorReason, _err: &Error) {
        telemetry::record_storage_background_error("regolith", reason_label(reason));
    }
}

fn reason_label(reason: BackgroundErrorReason) -> &'static str {
    match reason {
        BackgroundErrorReason::Flush => "flush",
        BackgroundErrorReason::Compaction => "compaction",
        BackgroundErrorReason::Manifest => "manifest",
        BackgroundErrorReason::WriteAheadLog => "wal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_is_counted() {
        let before = telemetry::storage_background_error_count();
        let err = Error::from(std::io::Error::other("Too many open files (os error 24)"));
        BackgroundErrorListener.on_background_error(BackgroundErrorReason::Compaction, &err);
        BackgroundErrorListener.on_background_error(BackgroundErrorReason::Flush, &err);
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
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len());
    }
}
