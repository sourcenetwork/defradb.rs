//! The orphan block sweep (`go.shinzo.orphan_gc`). A block fetched during
//! P2P sync carries a to-merge mark holding its fetch time until its merge
//! clears it. A fetch that is abandoned leaves its block marked, and no
//! history prune ever reaches it, since a prune walks out from documents.
//! The sweep reclaims such blocks once their mark is older than a cutoff.
//!
//! It runs in two steps, as Go Shinzo's `ReclaimOrphanBlocks` does: a
//! snapshot scan nominates expired marks, then each batch of nominees is
//! decided again in the transaction that acts on it, reading the mark and
//! the block's owners validated. A merge that clears a mark or records an
//! owner and commits first makes the batch run again, so a merged or owned
//! block is never deleted. Model: `proofs/tla/OrphanGc.tla`.

use std::future::Future as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cid::Cid;
use storage::corekv::{IterOptions, Key, Store};
use storage::keys::blockstore::{BlockstoreKey, ToMergeIndexKey};
use storage::stores::blockstore::marker_time;

use crate::error::{Error, Result};

/// How many nominees one decision transaction decides. A merge touching any
/// of them re-runs the whole batch, so it stays small.
pub const RECLAIM_BATCH: usize = 256;

/// What one call to [`crate::database::DB::reclaim_orphan_blocks`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    /// Where the next call resumes; `None` once the marks were swept to the end.
    pub next_key: Option<Vec<u8>>,
    /// Marks the scan read.
    pub scanned: usize,
    /// Blocks deleted with their mark.
    pub reclaimed: usize,
    /// Marks cleared from blocks a document owns: a mark that outlived the
    /// claim on its block.
    pub repaired: usize,
    /// Decision attempts a committing merge made run again.
    pub conflicts: usize,
    /// Marks whose key names no CID; nothing is done with them.
    pub unparsed: usize,
}

/// The error a cancelled sweep returns, Go's `context canceled`.
fn cancelled() -> Error {
    Error::Other("context canceled".to_string())
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Reclaim blocks fetched over P2P whose mark is older than
    /// `cutoff_unix` (unix seconds) and that no document owns, scanning at
    /// most `scan_limit` marks from `start_key` (the start of the marks when
    /// `None`). A mark with no time, written by an older build, is never
    /// reclaimed. `cancel` stops the sweep before its scan and between
    /// batches.
    pub async fn reclaim_orphan_blocks(
        self: &Arc<Self>,
        cutoff_unix: u64,
        start_key: Option<Vec<u8>>,
        scan_limit: usize,
        cancel: &AtomicBool,
    ) -> Result<SweepReport> {
        if cancel.load(Ordering::Acquire) {
            return Err(cancelled());
        }
        let mut report = SweepReport::default();
        let nominees = self
            .nominate_orphans(cutoff_unix, start_key, scan_limit, &mut report)
            .await?;
        for batch in nominees.chunks(RECLAIM_BATCH) {
            if cancel.load(Ordering::Acquire) {
                return Err(cancelled());
            }
            let decided = self.decide_orphans(cutoff_unix, batch.to_vec()).await?;
            report.reclaimed += decided.reclaimed;
            report.repaired += decided.repaired;
            report.conflicts += decided.conflicts;
        }
        Ok(report)
    }

    /// The blocks whose mark expired, from a snapshot: the scan nominates,
    /// it decides nothing.
    async fn nominate_orphans(
        &self,
        cutoff_unix: u64,
        start_key: Option<Vec<u8>>,
        scan_limit: usize,
        report: &mut SweepReport,
    ) -> Result<Vec<Cid>> {
        let txn = self.new_txn(true).await?;
        let blockstore = txn.blockstore()?;
        let mut options = IterOptions::new().with_prefix(ToMergeIndexKey::merge_prefix());
        if let Some(start) = start_key {
            options = options.with_start(start);
        }
        let mut iter = blockstore.iterator(options).await.map_err(Error::Storage)?;
        let mut nominees = Vec::new();
        while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
            if report.scanned == scan_limit {
                report.next_key = Some(pair.key.to_vec());
                break;
            }
            report.scanned += 1;
            let Ok(mark) = ToMergeIndexKey::from_bytes(&pair.key) else {
                report.unparsed += 1;
                continue;
            };
            if marker_time(&pair.value).is_some_and(|fetched| fetched < cutoff_unix) {
                nominees.push(mark.cid);
            }
        }
        iter.close().await.map_err(Error::Storage)?;
        drop(blockstore);
        let _ = txn.discard();
        Ok(nominees)
    }

    /// Decide `nominees` again in one transaction: a block whose mark is
    /// gone or renewed is left; an owned one loses its mark; any other is
    /// deleted with its mark. The report counts what was reclaimed and
    /// repaired, and how often a committing merge made the decision run again.
    pub async fn decide_orphans(
        self: &Arc<Self>,
        cutoff_unix: u64,
        nominees: Vec<Cid>,
    ) -> Result<SweepReport> {
        let attempts = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&attempts);
        let (reclaimed, repaired) = self
            .transact_db(move |txn| {
                counted.fetch_add(1, Ordering::Relaxed);
                let nominees = nominees.clone();
                Box::pin(async move {
                    let blockstore = txn.blockstore()?;
                    let systemstore = txn.systemstore()?;
                    let (mut reclaimed, mut repaired) = (0, 0);
                    for cid in nominees {
                        let mark = ToMergeIndexKey::new(cid).bytes();
                        let expired = blockstore
                            .get(&mark)
                            .await
                            .map_err(Error::Storage)?
                            .and_then(|value| marker_time(&value))
                            .is_some_and(|fetched| fetched < cutoff_unix);
                        if !expired {
                            continue;
                        }
                        if crate::docid::map::block_has_owners(&systemstore, &cid.to_string())
                            .await?
                        {
                            repaired += 1;
                        } else {
                            blockstore
                                .delete(&BlockstoreKey::new(cid).bytes())
                                .await
                                .map_err(Error::Storage)?;
                            reclaimed += 1;
                        }
                        blockstore.delete(&mark).await.map_err(Error::Storage)?;
                    }
                    Ok((reclaimed, repaired))
                })
            })
            .await?;
        Ok(SweepReport {
            reclaimed,
            repaired,
            conflicts: attempts.load(Ordering::Relaxed).saturating_sub(1),
            ..SweepReport::default()
        })
    }
}

/// The most marks one sweep examines; the next resumes where it stopped.
pub const MARKS_PER_SWEEP: usize = 100_000;

/// When the orphan sweep runs (Go Shinzo's `node/store_badger.go`), read
/// once when the store opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrphanSweepConfig {
    /// `DEFRA_ORPHAN_GC_DISABLED` unset or false.
    pub enabled: bool,
    /// `DEFRA_ORPHAN_GC_INTERVAL`: the time between sweeps, 5 minutes by default.
    pub interval: std::time::Duration,
    /// `DEFRA_ORPHAN_BLOCK_TTL`: how long a mark is kept, 30 minutes by default.
    pub ttl: std::time::Duration,
}

impl OrphanSweepConfig {
    /// The default interval between sweeps.
    pub const INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
    /// The default age past which a mark expires.
    pub const TTL: std::time::Duration = std::time::Duration::from_secs(30 * 60);

    /// The configuration the process environment gives.
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// The configuration `var` gives for each variable. A boolean that is
    /// not one is ignored with a warning; an unset, empty, unparsable, zero
    /// or negative duration falls back to its default.
    pub fn from_vars<F>(var: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let duration = |name: &str, default| {
            var(name)
                .and_then(|text| go_duration(&text))
                .filter(|duration| !duration.is_zero())
                .unwrap_or(default)
        };
        Self {
            enabled: !go_bool(var("DEFRA_ORPHAN_GC_DISABLED"), "DEFRA_ORPHAN_GC_DISABLED"),
            interval: duration("DEFRA_ORPHAN_GC_INTERVAL", Self::INTERVAL),
            ttl: duration("DEFRA_ORPHAN_BLOCK_TTL", Self::TTL),
        }
    }
}

/// A boolean variable as Go's `strconv.ParseBool` reads it; unset or not a
/// boolean is false.
fn go_bool(value: Option<String>, name: &str) -> bool {
    match value.as_deref() {
        None => false,
        Some("1" | "t" | "T" | "TRUE" | "true" | "True") => true,
        Some("0" | "f" | "F" | "FALSE" | "false" | "False") => false,
        Some(other) => {
            tracing::warn!(
                name,
                value = other,
                "Ignoring malformed boolean environment variable"
            );
            false
        }
    }
}

/// A duration as Go's `time.ParseDuration` reads it (`90s`, `3m`, `1h30m`,
/// `1.5h`, `250ms`); `None` for anything else, a negative one included.
fn go_duration(text: &str) -> Option<std::time::Duration> {
    let text = text.strip_prefix('+').unwrap_or(text);
    if text.is_empty() || text.starts_with('-') {
        return None;
    }
    let mut total = 0f64;
    let mut rest = text;
    while !rest.is_empty() {
        let number_len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
        let number: f64 = rest[..number_len].parse().ok()?;
        rest = &rest[number_len..];
        let unit_len = rest
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(rest.len());
        let nanos = match &rest[..unit_len] {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 60e9,
            "h" => 3600e9,
            _ => return None,
        };
        total += number * nanos;
        rest = &rest[unit_len..];
    }
    Some(std::time::Duration::from_nanos(total as u64))
}

/// The current unix seconds.
fn now_unix() -> u64 {
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Sweep at `config.interval` until `stop` completes, the first sweep one TTL after
/// the start, each examining at most [`MARKS_PER_SWEEP`] marks and resuming
/// where the last one stopped.
pub async fn run_orphan_sweep<S, F>(
    db: Arc<crate::database::DB<S>>,
    config: OrphanSweepConfig,
    stop: F,
) where
    S: Store + 'static,
    F: std::future::Future<Output = ()>,
{
    let mut stop = std::pin::pin!(stop);
    let mut wait = config.ttl;
    let mut start = None;
    let never = AtomicBool::new(false);
    loop {
        let pause = match workers::delay(wait) {
            Ok(pause) => pause,
            Err(error) => {
                tracing::error!(%error, "the orphan sweep stopped: no timer could be set");
                return;
            }
        };
        let mut pause = std::pin::pin!(pause);
        let stopping = std::future::poll_fn(|cx| {
            if stop.as_mut().poll(cx).is_ready() {
                return std::task::Poll::Ready(true);
            }
            pause.as_mut().poll(cx).map(|_| false)
        })
        .await;
        if stopping {
            return;
        }
        wait = config.interval;
        let cutoff = now_unix().saturating_sub(config.ttl.as_secs());
        match db
            .reclaim_orphan_blocks(cutoff, start.take(), MARKS_PER_SWEEP, &never)
            .await
        {
            Ok(report) => {
                if report.reclaimed + report.repaired > 0 {
                    tracing::info!(?report, "orphan block sweep");
                }
                start = report.next_key;
            }
            Err(error) => tracing::warn!(%error, "orphan block sweep failed"),
        }
    }
}

/// Starts [`run_orphan_sweep`] on the database's worker pool, unless the
/// configuration turns it off.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_orphan_sweep<S>(
    db: Arc<crate::database::DB<S>>,
    config: OrphanSweepConfig,
    shutdown: p2p::sync::SyncShutdownHandle,
) -> Option<workers::Background>
where
    S: Store + 'static,
{
    if !config.enabled {
        return None;
    }
    let pool = db.pool().cloned();
    Some(workers::spawn_background(
        pool.as_ref(),
        "db::orphan_sweep",
        move || run_orphan_sweep(db, config, async move { shutdown.cancelled().await }),
    ))
}
