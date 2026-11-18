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
//! Only ever raise. On macOS and the BSDs `setrlimit` is also capped by
//! `kern.maxfilesperproc`, and a request above it is either refused or
//! clamped down to it, which would lower a soft limit already above that
//! ceiling. So the target is capped by the ceiling before anything is
//! changed, and nothing is changed unless the result is a raise.

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
    let ceiling = match os_ceiling() {
        Ok(ceiling) => ceiling,
        Err(error) => {
            tracing::warn!(%error, before, "could not read the per-process open-file ceiling");
            return;
        }
    };
    let now = match plan(before, hard, ceiling) {
        None => before,
        Some(raised) => match rlimit::Resource::NOFILE.set(raised, hard) {
            Ok(()) => {
                tracing::info!(before, now = raised, "raised the open-file limit");
                raised
            }
            Err(error) => {
                tracing::warn!(%error, before, raised, "could not raise the open-file limit");
                before
            }
        },
    };
    if now < LOW_LIMIT_WARNING {
        tracing::warn!(
            limit = now,
            "open-file limit is low; regolith compaction may fail with EMFILE"
        );
    }
}

#[cfg(not(unix))]
pub(super) fn raise_nofile_limit() {}

/// The soft limit to set, or `None` to leave it alone: the target capped
/// by the hard limit and the OS ceiling, and only when that is a raise.
#[cfg(unix)]
fn plan(before: u64, hard: u64, ceiling: Option<u64>) -> Option<u64> {
    let raised = TARGET.min(hard).min(ceiling.unwrap_or(u64::MAX));
    (raised > before).then_some(raised)
}

/// `kern.maxfilesperproc`, read the way `rlimit` reads it internally.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly"
))]
fn os_ceiling() -> std::io::Result<Option<u64>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_MAXFILESPERPROC];
    let mut value: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: `mib` names a two-level integer sysctl, `value` is a c_int
    // and `len` its size; no new value is written.
    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut value as *mut libc::c_int).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(u64::try_from(value).ok())
}

/// No per-process ceiling beyond the hard limit.
#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly"
    ))
))]
fn os_ceiling() -> std::io::Result<Option<u64>> {
    Ok(None)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_limit_above_the_ceiling_but_below_the_target_is_left_alone() {
        // kernel ceiling 10_240 < inherited soft 32_768 < TARGET
        assert_eq!(plan(32_768, u64::MAX, Some(10_240)), None);
    }

    #[test]
    fn raises_to_the_lowest_of_target_hard_limit_and_ceiling() {
        assert_eq!(plan(256, u64::MAX, None), Some(TARGET));
        assert_eq!(plan(256, 1_024, None), Some(1_024));
        assert_eq!(plan(256, u64::MAX, Some(10_240)), Some(10_240));
    }

    #[test]
    fn a_limit_already_at_or_above_the_target_is_left_alone() {
        assert_eq!(plan(TARGET, u64::MAX, None), None);
        assert_eq!(plan(1_048_576, u64::MAX, Some(245_760)), None);
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
