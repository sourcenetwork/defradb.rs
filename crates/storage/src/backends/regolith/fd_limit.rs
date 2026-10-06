//! Raise the process's open-file limit before a store opens.
//!
//! regolith keeps a descriptor open per live SSTable plus those a
//! compaction is reading and writing. macOS launchd and many service
//! managers start processes with a soft limit of 256, which a moderately
//! sized store exhausts; compaction then fails with EMFILE on every retry
//! and the store wedges. The hard limit is almost always far higher, and
//! raising the soft limit toward it needs no privilege, so do that once.
//!
//! Only ever raise. A soft limit already at [`TARGET`] is left alone: on
//! macOS the helper clamps to `kern.maxfilesperproc`, which would lower a
//! soft limit someone deliberately set above it.

/// The soft limit to raise toward.
#[cfg(unix)]
const TARGET: u64 = 65_536;

/// The soft limit below which a store is likely to run out of
/// descriptors under compaction.
#[cfg(unix)]
const LOW_LIMIT_WARNING: u64 = 4096;

#[cfg(unix)]
pub(super) fn raise_nofile_limit() {
    static RAISED: std::sync::Once = std::sync::Once::new();
    RAISED.call_once(|| {
        let before = rlimit::Resource::NOFILE.get().map(|(soft, _)| soft).ok();
        if before.is_some_and(|soft| soft >= TARGET) {
            return;
        }
        match rlimit::increase_nofile_limit(TARGET) {
            Ok(now) => {
                if before != Some(now) {
                    tracing::info!(before, now, "raised the open-file limit for regolith");
                }
                if now < LOW_LIMIT_WARNING {
                    tracing::warn!(
                        limit = now,
                        "open-file limit is low; regolith compaction may fail with EMFILE"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(%error, ?before, "could not raise the open-file limit");
            }
        }
    });
}

#[cfg(not(unix))]
pub(super) fn raise_nofile_limit() {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn raises_a_low_soft_limit_and_never_lowers_one() {
        let (before, hard) = rlimit::Resource::NOFILE.get().unwrap();
        raise_nofile_limit();
        let (after, _) = rlimit::Resource::NOFILE.get().unwrap();
        assert!(
            after >= before,
            "lowered the soft limit {before} -> {after}"
        );
        assert!(after <= hard);
        if before < TARGET {
            // The OS may cap below the hard limit (macOS clamps to
            // `kern.maxfilesperproc`), so the check is that nothing was
            // left to raise, not that the limit reached the target.
            let ceiling = rlimit::increase_nofile_limit(TARGET).unwrap();
            assert_eq!(
                after, ceiling,
                "left the soft limit at {after}, below {ceiling}"
            );
        }
    }
}
