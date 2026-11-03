# Lens Migrations

Schemas evolve; stored documents must not be rewritten to follow. A **lens** is a
WASM module that transforms a document from one schema version to another, applied at
*query time* (`crates/lens`): documents stay on disk in the version they were written
in, and readers see them through the lens chain to the target version.

## The pipeline

Given a collection's version history (a DAG of schema versions), the lens history
builder prunes it to one path per source version toward the target, and the pipeline
applies the transform chain lazily: one document per `next()` call, on the caller's
task, inside the reader's transaction (the lensed fetcher, `crates/db/src/read/lensed/`).
Explicit migration runs (backfills) use the same pipeline; nothing is rewritten at
migration-*set* time, so setting a migration is metadata, not a data rewrite.

## The engine and the sandbox

One shared wasmtime `LensEngine` per database: one engine, one linker, one epoch
clock; modules are compiled once at registration into an `InstancePre`, and each call
instantiates and runs on a fiber the caller polls (`crates/lens/src/engine/`). The
default transform store is restrictive-sandboxed WASM; without the wasmtime runtime
feature, the store refuses everything (a build without lens support fails loud, not
silently passing documents through).

## The epoch tick

A lens module is untrusted code, so it runs under two bounds: **fuel/ticks** (a call
out of ticks traps) and the **epoch clock**: `EpochTick::tick()` bumps the engine's
epoch, every running instance yields at its next check, and a module that exceeds its
tick allowance traps deterministically. Tick sources: a periodic job on a worker
pool's deadline queue (`PoolTicks`), or, with no pool, a dedicated parked thread
started on the first lens call (`ThreadTicks`) - a database that never runs a lens
starts no thread.

## Registration and use

Lens modules are registered over HTTP (`POST /lens`) and attached between versions
(`POST /lens/set` or the collections migration route). At query time the lens runs for
documents whose version differs from the target; explicit transactions pin their lens
chain (`PreparedLens`) so a multi-statement transaction sees one consistent transform.

> **Why transform-on-read instead of rewrite-on-migrate.** A rewrite is a full-dataset
> transaction: unbounded work, a fail window, and a rollback problem. Lenses avoid the
> rewrite entirely: setting a migration is metadata, every old version stays intact
> (time travel still works), and a bad migration is replaced (it is code, not data
> damage). The cost moves to the read path: each read of an old-version document
> transforms it through the chain, and repeated reads repeat that work, bounded per
> call by the epoch and fuel limits; an explicit backfill is the escape hatch when
> read cost matters.
