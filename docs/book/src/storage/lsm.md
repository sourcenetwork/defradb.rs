# LSM structure: WAL, SSTs, compaction

[regolith](https://sourcenetwork.github.io/regolith/) is an LSM engine: writes land in a memtable protected by a WAL, memtables flush
to SST files, and compaction merges SSTs in the background. defradb.rs does not reimplement
any of this; it configures it and reads its telemetry. This chapter is what a defradb.rs
developer needs to know about what is underneath.

## Opening a store

`RegolithStore::open_with_options` ([`crates/storage/src/backends/regolith/store.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/store.rs)) is
the single entry point:

1. Engine options are assembled in [`crates/storage/src/backends/regolith/config.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/config.rs): the merge operator, the prefix extractor,
   the at-rest key provider, statistics, and the store's event listener and transaction
   hooks.
2. On native targets the open path first checks for a legacy per-value-encrypted store
   and, if found, performs the one-time rewrite to the file-sealed format
   (`rewrite/`), streaming in bounded batches; then opens
   `OptimisticTransactionDb::open`.
3. On wasm, OPFS is mounted (with a durability fallback to eventual mode when the browser
   reports no durable sync).
4. The classifier and isolation level are attached to the handle.

Configuration profiles ([`crates/storage/src/backends/regolith/config.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/config.rs)):

- **server defaults**: `DurabilityMode::Immediate`, `IsolationLevel::DefraLevel`, a 30 s
  close timeout;
- **embedded**: engine sizes tuned for a 1-4 MiB working set;
- **wasm**: the OPFS environment's own limits;
- **memory**: a `MemEnv` that starts no threads, so inline compaction is enabled: with no
  worker, a write that a stall stops runs the compaction itself, or nothing ever clears
  the stall.

## Threads: who runs what

The pool wraps the engine environment with `placed_on(pool)`
([`crates/storage/src/backends/regolith/config.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/config.rs)), which places the threads regolith
starts (compaction, log writeback) through the workers pool as background-class threads.
That keeps them off the cores running data-path workers and accounted for in one place.
An in-memory environment starts none.

## The read path

A read consults, in order: the transaction's own buffered puts, deletes and merges (in
operation order), then the memtables, then L0, then deeper levels by binary search. All
of it is zero-copy where the engine offers it (defradb.rs reads with `get_slice` and
`Bytes::from_owner`, so values are borrowed, not copied). Block-cache misses are where
`CacheOnly` and the `IoQueue` machinery takes over; see
[Threading](../threading/pool.md).

## Writes, WAL and SSTs

A transaction's writes are buffered in the transaction, not the memtable, until commit.
At commit the engine validates, assigns sequences, writes one WAL record for the whole
commit group and fsyncs it once (see [the commit path](../txn/commit.md)). Memtables
flush to SST files when they fill; the manifest tracks the live file set. defradb.rs never
writes a file itself.

## Compaction

A stripe-aware compaction worker merges SSTs in the background. defradb.rs installs *no
compaction filter*, on principle ([`crates/storage/src/backends/regolith/mod.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/mod.rs) documents
the decision): a head-set entry depends on later writes; nothing expires by time alone; a
document part's tombstone carries the priority a late merge is ordered against; index
entries, blocks, ACP tuples and guards each decide something until another key says
otherwise. A filter that dropped any of them would change what a reader or a later merge
sees. Data retirement is a *business* decision (downsample retention, collection delete),
and those run as ordinary transactions; see
[Memory reclamation](../memory/index.md).

## Statistics and telemetry

Stores keep ticker-level statistics only (`StatsLevel::ExceptHistogramOrTimers`): a timer
would read the clock twice per iterator step, and a histogram sample costs atomics on
every read and commit. Every open store exports its tickers as one OTel counter
([`crates/telemetry/src/engine_tickers.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/telemetry/src/engine_tickers.rs)), and the store's `EventListener` reports
flushes, compactions, table-file creation and deletion, background errors, a discarded
WAL tail at open, and every commit conflict with its reason.

> **Why ticker-level only.** The observable cost of measurement has to be below the noise
> floor of the operation measured, or the measurement changes the system. Histograms and
> timers on per-read paths failed that test; tickers (monotonic counters) do not.
