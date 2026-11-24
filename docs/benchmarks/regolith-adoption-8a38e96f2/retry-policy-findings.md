# Retry policy comparison for 64-writer contention

This note records source-level policy differences relevant to the final step-8 benchmark result. It does not establish that retry policy caused the exhaustion increase.

## Source and configured behavior

- Final step-8 source inspected: DefraDB `f7813d07f2f16eefb6ebc994ff09fc08b63e0742` (`defradb-regolith-proofs-final`). The auto-commit create paths (`crates/database/db/src/write/autocommit/create.rs:347,760`), update (`.../update.rs:176`), delete (`.../delete.rs:154,393`), and batch mutation (`.../batch.rs:225`) pass `storage::corekv::RetryOptions::new(4)`.
- `crates/engine/storage/src/corekv/traits.rs` documents `RetryOptions.max_attempts` as total transaction attempts, including the first. These paths allow four conflict-budgeted attempts (three commit-conflict re-runs); cache-validation re-runs can invoke the callback again without consuming that budget.
- `crates/engine/storage/src/backends/regolith/store.rs:309` maps this to `regolith::RetryPolicy::new(options.max_attempts.max(1))`; it does not configure `.backoff(...)`.
- The exact pinned Regolith revision is `d1e36aa747913f6b1ecc14c821f4e5e6314674cf` from `Cargo.lock`. In that revision, `RetryPolicy::new` defaults to zero delay (`src/transaction/retry.rs`, `RetryPolicy::new`); the Defra adapter supplies its own four-attempt policy. `run_async` re-runs immediately after an actual commit conflict. It can also re-run after a cache-validation `WouldBlock`; that wait/re-run is distinct from a conflict and does not consume the conflict budget.
- `DbOptions::DEFAULT_MAX_TXN_RETRIES` remains 5 in `crates/database/db/src/database/mod.rs`, but the autocommit callsites above hard-code four attempts. The configurable value is used by index backfill/reindex paths; it is not the autocommit budget in these callsites.

## Historical HTTP policy

The pre-step-5 HTTP implementation is present in main revision `8a38e96f213cd9edc55c89eed272ba8f83c76524`, `crates/surfaces/http/src/query_context.rs` and `crates/surfaces/http/src/router/state.rs`. The outer conflict replay was removed by `2fa4406657cb35e79505f4332b0bf2d40b5e741e`.

At `8a38e96f2`, `AppState` defaults `max_txn_retries` from `db::DEFAULT_MAX_TXN_RETRIES` (five). `execute_request_with_retry_loop` ran the prepared request once, then could replay it up to five more times after transaction-conflict responses: six total request executions. Between replays it chose a random delay uniformly from zero to the current cap. The cap started at 10 ms and doubled to a 250 ms ceiling; with the default five retries, per-retry maximums were 10, 20, 40, 80, and 160 ms (310 ms cumulative worst case). This re-executed the already prepared request; it did not repeat `prepare_request` in that loop.

Thus the concrete policy change is **four immediate engine attempts** versus **up to six HTTP-level executions with randomized exponential waits** in the old path. This compares total attempts and scheduling; it is not a controlled comparison of otherwise identical transaction bodies or dependency sets.

## Evidence and interpretation

The reported 64-writer measurements are 4,775 shared-update exhaustions and 8,434 counter-update exhaustions out of 16,384 each, versus legacy counts 577 and 543. Throughput was 493/299 commits per second versus 242/232, with create throughput of 7,367 commits/second in the final run versus 247 in the legacy run. These are observed gate results. Fewer allowed attempts and zero backoff could plausibly increase exhaustion when many writers collide, while avoiding HTTP delay can raise throughput. The measurements do not identify this as the cause: retry ownership, conflict tracking, transaction scope, and read/write dependency behavior changed as well.

The supplied workload parity checks (including the eight-document corpus and indexed counter label) and the validated counter-read dependencies are relevant constraints. The optional create-only validation fast path cannot explain or repair shared-update/counter contention.

## Next investigation for the engine owner

1. Preserve the exact 64-writer workload and binary/source provenance. Record the typed conflict access/write-kind at each lost attempt and at exhaustion, separating same-field write/write conflicts from read-dependency and other conflicts. Report frequencies and attempts per logical request; do not infer from aggregate exhaustion counts.
2. Run a paired policy sweep on the same source/workload, changing only total attempts and retry delay: current four immediate attempts; a higher attempt budget with zero delay; then the same budget with the engine’s bounded exponential backoff. Its current API uses deterministic delays, not jitter; evaluating jitter would require a separate engine change. Keep throughput, commit/exhaustion counts, and p50/p99 together. Use enough repeated samples and preserve the same load-guard protocol.
3. If exhaustions are dominated by true same-field collisions, assess policy separately from dependency changes. If they are dominated by broad/read dependencies, first inspect typed conflict provenance and dependency precision; do not hide intentional counter reads or weaken validation to improve the gate.
