//! Raise the process's open-file limit before the node starts.
//!
//! regolith keeps a descriptor open per live SSTable plus those a
//! compaction is reading and writing. macOS launchd and many service
//! managers start processes with a soft limit of 256, which a moderately
//! sized store exhausts; compaction then fails with EMFILE on every retry
//! and the store wedges. The hard limit is almost always far higher, and
//! raising the soft limit toward it needs no privilege.
//!
//! This is process policy, so it lives here at `start` rather than in the
//! storage library: an embedding application owns its own limits.
//!
//! Only ever raise. `rlimit::increase_nofile_limit` clamps to macOS's
//! `kern.maxfilesperproc` after comparing against the request, so a soft
//! limit above that ceiling but below [`TARGET`] would come back lowered;
//! that case is detected and the original limit restored.

/// The soft limit to raise toward.
#[cfg(unix)]
const TARGET: u64 = 65_536;

/// The soft limit below which a store is likely to run out of
/// descriptors under compaction.
#[cfg(unix)]
const LOW_LIMIT_WARNING: u64 = 4096;

#[cfg(unix)]
pub(super) fn raise_nofile_limit() {
    let (before, hard) = match rlimit::Resource::NOFILE.get() {
        Ok(limits) => limits,
        Err(error) => {
            tracing::warn!(%error, "could not read the open-file limit");
            return;
        }
    };
    if before >= TARGET {
        return;
    }
    let now = match rlimit::increase_nofile_limit(TARGET) {
        Ok(raised) => match restore_to(before, raised) {
            None => raised,
            Some(previous) => {
                if let Err(error) = rlimit::Resource::NOFILE.set(previous, hard) {
                    tracing::warn!(%error, before, raised, "could not restore the open-file limit");
                }
                previous
            }
        },
        Err(error) => {
            tracing::warn!(%error, before, "could not raise the open-file limit");
            before
        }
    };
    if now != before {
        tracing::info!(before, now, "raised the open-file limit");
    }
    if now < LOW_LIMIT_WARNING {
        tracing::warn!(
            limit = now,
            "open-file limit is low; regolith compaction may fail with EMFILE"
        );
    }
}

#[cfg(not(unix))]
pub(super) fn raise_nofile_limit() {}

/// The soft limit to put back when raising left it lower than it started,
/// which happens when the starting limit was already above the OS ceiling.
#[cfg(unix)]
fn restore_to(before: u64, raised: u64) -> Option<u64> {
    (raised < before).then_some(before)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_limit_above_the_ceiling_but_below_the_target_is_restored() {
        // kernel ceiling 10_240 < inherited soft 32_768 < TARGET
        assert_eq!(restore_to(32_768, 10_240), Some(32_768));
    }

    #[test]
    fn a_raise_is_kept() {
        assert_eq!(restore_to(256, 10_240), None);
        assert_eq!(restore_to(256, 256), None);
    }

    #[test]
    fn raising_never_leaves_the_soft_limit_lower() {
        let (before, hard) = rlimit::Resource::NOFILE.get().unwrap();
        raise_nofile_limit();
        let (after, _) = rlimit::Resource::NOFILE.get().unwrap();
        assert!(
            after >= before,
            "lowered the soft limit {before} -> {after}"
        );
        assert!(after <= hard);
    }
}
