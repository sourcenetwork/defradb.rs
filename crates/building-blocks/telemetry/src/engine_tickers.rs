//! The storage engine's tickers, exported as one OpenTelemetry counter.
//!
//! Every open store hands its engine statistics to [`export_engine_tickers`]
//! and holds the returned [`EngineTickerExport`] while it is open. The
//! `defradb.storage.engine.tickers` counter reports, per exported ticker, a
//! total that only grows ([`engine_ticker_totals`]): each read first adds to it
//! what every live engine counted since it was last added, and a store that
//! closes adds its last counts as its guard drops, so an exported total never
//! goes back when a store closes.
//!
//! Opening and closing a store costs the same however many stores are open:
//! one entry in a lock-free map, and a closing store's counts added once.
//! Each count is added exactly once whoever adds it, a read or the close: an
//! adder claims the counts it adds with a compare-and-swap on the engine's
//! published mark.
//!
//! The exported tickers are the engine's commit and policy families, read from
//! `regolith::Ticker::ALL` by [`exported_engine_tickers`]; a store's statistics
//! snapshot reports the same list through the same function.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, Lazy};
use regolith::{Statistics, Ticker};

/// Name prefixes of the ticker families reported outside the engine.
const EXPORTED_FAMILIES: [&str; 2] = ["regolith.commit.", "regolith.policy."];

const TICKERS: usize = Ticker::ALL.len();

/// The exported tickers, in `Ticker::ALL` order.
static EXPORTED: Lazy<Vec<Ticker>> = Lazy::new(|| {
    Ticker::ALL
        .iter()
        .copied()
        .filter(|ticker| is_exported(*ticker))
        .collect()
});

/// What every engine counted and was added, by position in `Ticker::ALL`.
static TOTALS: [AtomicU64; TICKERS] = [const { AtomicU64::new(0) }; TICKERS];

/// The live engines, by export id.
static LIVE: Lazy<HopscotchMap<u64, Arc<Source>, RandomState>> =
    Lazy::new(|| HopscotchMap::with_hasher(RandomState::default()));

static NEXT_SOURCE: AtomicU64 = AtomicU64::new(0);

/// The engine tickers reported outside the engine: its commit and policy
/// families, in `Ticker::ALL` order.
pub fn exported_engine_tickers() -> impl Iterator<Item = Ticker> {
    EXPORTED.iter().copied()
}

fn is_exported(ticker: Ticker) -> bool {
    EXPORTED_FAMILIES
        .iter()
        .any(|family| ticker.name().starts_with(family))
}

/// One live engine: its statistics and, per exported ticker, its count when
/// that count was last added to [`TOTALS`].
///
/// Weak, so the map entry a close retires (freed later by the map's
/// reclamation) never keeps an engine's per-thread statistics alive: they go
/// when the store lets go of them.
struct Source {
    statistics: Weak<Statistics>,
    published: [AtomicU64; TICKERS],
}

impl Source {
    /// Add to [`TOTALS`] what `statistics` counted since the last addition.
    ///
    /// Counts only grow, so a reading at or below the mark was added already,
    /// by an earlier call or by one racing this one with a newer reading. A
    /// count `Statistics::reset` took back is added again only past the mark.
    fn publish(&self, statistics: &Statistics) {
        for ticker in exported_engine_tickers() {
            let at = ticker as usize;
            let now = statistics.get_ticker(ticker);
            let mut seen = self.published[at].load(Ordering::Acquire);
            while now > seen {
                match self.published[at].compare_exchange_weak(
                    seen,
                    now,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        TOTALS[at].fetch_add(now - seen, Ordering::AcqRel);
                        break;
                    }
                    Err(current) => seen = current,
                }
            }
        }
    }
}

/// Keeps one engine's tickers in the export until it is dropped; then what the
/// engine counted stays in the exported totals.
#[must_use = "the engine leaves the export when this guard is dropped"]
pub struct EngineTickerExport {
    id: u64,
    source: Arc<Source>,
    /// Held so the last counts can be added when the guard drops, whoever
    /// else let go of the statistics first.
    statistics: Arc<Statistics>,
}

/// Export `statistics`' tickers until the returned guard is dropped.
pub fn export_engine_tickers(statistics: &Arc<Statistics>) -> EngineTickerExport {
    let id = NEXT_SOURCE.fetch_add(1, Ordering::Relaxed);
    let source = Arc::new(Source {
        statistics: Arc::downgrade(statistics),
        published: [const { AtomicU64::new(0) }; TICKERS],
    });
    LIVE.insert(id, Arc::clone(&source));
    EngineTickerExport {
        id,
        source,
        statistics: Arc::clone(statistics),
    }
}

impl Drop for EngineTickerExport {
    fn drop(&mut self) {
        // Added before the entry leaves the map: a read that no longer finds
        // the entry finds its counts in the totals.
        self.source.publish(&self.statistics);
        LIVE.remove(&self.id);
    }
}

/// Every exported ticker's name and total: what closed engines counted plus
/// what each live engine counts now.
pub fn engine_ticker_totals() -> Vec<(&'static str, u64)> {
    for (_, source) in LIVE.iter() {
        if let Some(statistics) = source.statistics.upgrade() {
            source.publish(&statistics);
        }
    }
    exported_engine_tickers()
        .map(|ticker| {
            (
                ticker.name(),
                TOTALS[ticker as usize].load(Ordering::Acquire),
            )
        })
        .collect()
}

/// Register the counter that reports [`engine_ticker_totals`] at every
/// collection, one series per ticker.
#[cfg(feature = "otlp")]
pub(crate) fn register(meter: &opentelemetry::metrics::Meter) {
    let _ = meter
        .u64_observable_counter("defradb.storage.engine.tickers")
        .with_description("Storage engine commit and policy counters, by ticker")
        .with_callback(|observer| {
            for (name, total) in engine_ticker_totals() {
                observer.observe(total, &[opentelemetry::KeyValue::new("ticker", name)]);
            }
        })
        .build();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_ids() -> Vec<u64> {
        LIVE.keys().collect()
    }

    #[test]
    fn the_export_names_the_commit_and_policy_families_of_ticker_all() {
        let exported: Vec<Ticker> = exported_engine_tickers().collect();
        assert!(exported.contains(&Ticker::CommitCount));
        assert!(exported.contains(&Ticker::CommitConflicts));
        assert!(exported.contains(&Ticker::PolicyBlindMergesCommuted));
        assert!(!exported.contains(&Ticker::BytesWritten));
        let names: Vec<&str> = engine_ticker_totals()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let expected: Vec<&str> = exported.iter().map(|ticker| ticker.name()).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn an_engine_is_exported_until_its_guard_drops() {
        let statistics = Arc::new(Statistics::new());
        let export = export_engine_tickers(&statistics);
        let id = export.id;
        assert!(live_ids().contains(&id));
        drop(export);
        assert!(!live_ids().contains(&id));
    }

    fn exported_commits() -> u64 {
        engine_ticker_totals()
            .into_iter()
            .find(|(name, _)| *name == Ticker::CommitCount.name())
            .map_or(0, |(_, total)| total)
    }

    /// Reads and the close each add what they find not yet added, and every
    /// commit lands in the total exactly once, however the two interleave.
    /// The only commits in this test binary are this test's.
    #[test]
    fn every_count_reaches_the_total_once_through_reads_and_the_close() {
        let statistics = Arc::new(Statistics::new());
        let options = regolith::Options::memory().statistics(Some(Arc::clone(&statistics)));
        let db = regolith::OptimisticTransactionDb::open("engine-tickers-once", options).unwrap();
        let commit = |key: &[u8]| {
            let txn = db.begin(&regolith::TxnOptions::new());
            txn.put(key, b"value").unwrap();
            txn.commit().unwrap();
        };
        let export = export_engine_tickers(&statistics);
        let before = exported_commits();
        for round in 0..3u8 {
            commit(&[round]);
            let total = exported_commits();
            assert_eq!(total, before + u64::from(round) + 1);
            assert_eq!(exported_commits(), total, "a second read adds nothing");
        }
        commit(b"after the last read");
        drop(export);
        assert_eq!(exported_commits(), before + 4);
        commit(b"after the close");
        assert_eq!(
            exported_commits(),
            before + 4,
            "a closed engine adds nothing"
        );
    }
}
