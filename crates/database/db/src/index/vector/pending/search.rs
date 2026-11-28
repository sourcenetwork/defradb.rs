//! A search over a deferred index: the graph's hits and the pending writes,
//! read on one snapshot, as Go's `mergePending`.

use rapidhash::{HashSetExt, RapidHashSet};

use super::entry::{PendingEntry, PendingOp};
use crate::index::error::Result;
use crate::index::vector::engine::ann::{Admit, Neighbor, VectorIndexEngine};
use defra_core::vector::{Element, Metric};

/// The `k` nearest of the graph and `pending`.
///
/// The graph is stale for a node with a pending write, so its hits for those
/// nodes are dropped, and the graph is asked for more while that leaves fewer
/// than `k`; never past `k` plus the pending count, since each pending write
/// knocks out at most one hit. Pending upserts are then scored exactly.
pub(crate) async fn merge_pending<E, Q, A>(
    engine: &E,
    pending: &[PendingEntry],
    metric: Metric,
    query: &[Q],
    k: usize,
    effort: Option<usize>,
    admit: &A,
) -> Result<Vec<Neighbor>>
where
    E: VectorIndexEngine,
    Q: Element,
    A: Admit,
{
    let mut hits = engine.search_where(query, k, effort, admit).await?;
    if pending.is_empty() || k == 0 {
        return Ok(hits);
    }
    let mut stale = RapidHashSet::with_capacity(pending.len());
    stale.extend(pending.iter().map(|entry| entry.id));
    let widest = k.saturating_add(pending.len());
    let mut asked = k;
    let mut merged = loop {
        let kept: Vec<Neighbor> = hits
            .iter()
            .filter(|hit| !stale.contains(&hit.id))
            .copied()
            .collect();
        // Enough kept, the graph ran out, or every pending node was in reach.
        if kept.len() >= k || hits.len() < asked || asked >= widest {
            break kept;
        }
        asked = (asked + (k - kept.len())).min(widest);
        hits = engine.search_where(query, asked, effort, admit).await?;
    };

    let query = metric.prepare(query);
    merged.extend(
        pending
            .iter()
            .filter(|entry| entry.op == PendingOp::Upsert && admit.admits(entry.id))
            .map(|entry| Neighbor {
                id: entry.id,
                distance: metric.distance_stored(&query, &entry.vector),
            }),
    );
    merged.sort_by(|a, b| {
        a.distance
            .total_cmp(&b.distance)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged.truncate(k);
    Ok(merged)
}
