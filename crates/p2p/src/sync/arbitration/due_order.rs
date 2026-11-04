//! The tie-break order for due roots, measured but not applied: due times are
//! bucketed by `TIE_BUCKET` from the earliest due root of the pass, and roots
//! order by (bucket, no-progress score, due time, CID). A score can only
//! reorder roots due within one bucket of each other (Lean
//! `bucket_order_any_score`, `inversion_within_bucket`).

use std::time::Duration;

/// Width of a due-time bucket. Under the tie-break, `ACT_AttemptsStartInDueOrder`
/// (`proofs/tla/MC_SyncOwnership_Dispatch.tla`) holds at this resolution: an
/// attempt starts only for a root due less than one bucket after the oldest due
/// root that can run. Being at most `PENDING_RETRY_BASE`, it keeps
/// `ACT_EndedAttemptQueuesBehind` exact for a root re-armed a backoff after its
/// attempt; a preempted root, due again at once, keeps it at this resolution.
pub const TIE_BUCKET: Duration = Duration::from_secs(1);

fn bucket(offset: Duration) -> u64 {
    let width = TIE_BUCKET.as_millis().max(1);
    u64::try_from(offset.as_millis() / width).unwrap_or(u64::MAX)
}

/// The order the tie-break gives a due set, as indexes into `due`, which is in
/// today's order (due time, then CID): each root keyed by its offset from the
/// earliest due root and its no-progress score.
pub(crate) fn tie_break_order(due: &[(Duration, u16)]) -> Vec<usize> {
    let mut keyed: Vec<(u64, u16, usize)> = due
        .iter()
        .enumerate()
        .map(|(index, (offset, score))| (bucket(*offset), *score, index))
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, _, index)| index).collect()
}

/// Whether the tie-break would reorder a due set that is in today's order.
pub(crate) fn tie_break_reorders(due: &[(Duration, u16)]) -> bool {
    due.len() > 1
        && tie_break_order(due)
            .into_iter()
            .enumerate()
            .any(|(position, index)| index != position)
}
