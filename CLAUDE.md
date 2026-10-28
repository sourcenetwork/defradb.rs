# Development Principles

## 0. The North Star: 1.0 Release

**The Go compatibility baseline lives in `crates/defra-version/src/lib.rs`, not
here.** Three constants define it: `GO_COMPAT_BRANCH`, `GO_COMPAT_COMMIT`, and
`GO_COMPAT_TAG`. An empty tag means CI builds the pinned commit from source
instead of downloading a release binary. `.github/workflows/ci.yml` greps those
constants directly, so they are the single source of truth — read them rather
than quoting a release number, which goes stale on every baseline bump.

Go parity is achieved across CLI, HTTP API, GraphQL query engine, and P2P replication. We now validate with Rust-native integration tests that exercise the full stack, and are building Rust-specific features (Iroh transport, BM25 full-text search, Postgres wire protocol, WASM client).

### What "Parity" Means

- Same GraphQL query → Same results (field values, ordering, errors)
- Same document content → Same document ID (content-addressed CIDs)
- Same CRDT operations → Same merged state (convergence)
- Same P2P protocol → Nodes can discover, connect, and replicate
- Same CLI commands → Same behavior and output
- Same HTTP API → Same wire format and response structure

---

## 1. Information Hygiene

This codebase is designed for **AI-human pair programming**. Every structural choice optimizes for **rapid context acquisition**.

**Context clarity is oxygen for productive collaboration.**

## 2. Temporal Boundaries

| Zone | Contains | Lives in |
|------|----------|----------|
| **Past** | How we got here | Git history, closed issues/PRs |
| **Present** | What the code does now | Working tree |
| **Future** | What we might do next | GitHub issues |

**No commented-out code. No TODO comments (create issues instead). No speculative docs.**

## 3. No Documentation Files

Only allowed: `README.md`, `CLAUDE.md`, `Cargo.toml` files.

No `ROADMAP.md`, `DEVELOPMENT.md`, `docs/` directories, or planning documents.

## 4. File Organization

**One concept per file. Small files over large files.**

### Crate Structure

```
crates/
├── acp/                # Access Control Policy
├── bitswap/            # Bitswap block exchange, wire-compatible with Go
├── blockstore/         # IPLD block storage
├── cli/                # Command-line interface
├── crdt/               # CRDT implementations
├── crypto/             # Cryptographic operations
├── cursor/             # Opaque cursor token codec for GraphQL pagination
├── datastore/          # Data persistence abstractions
├── db/                 # Database core, one module per execution role:
│                       #   txn/ (the transaction seam), read/, write/,
│                       #   collection/, block/, definition/, access/,
│                       #   database/, docid/, downsample/, view/,
│                       #   event/, error/, plus the folded former crates:
│                       #   backup/, block/builder/, index/, merge/, nac/,
│                       #   search/. src/ holds only lib.rs; every test
│                       #   lives in db/tests/.
├── defra-core/         # Core types and traits
├── defra-node/         # Reusable embedded node builder
├── defra-version/      # Version metadata and Go compat tracking
├── document/           # Document handling
├── embedded/           # Embedded/mobile node assembly
├── events/             # Pub/sub event bus (subscriptions)
├── ffi/                # C-compatible FFI bindings
├── http/               # HTTP API server
├── identity/           # Identity and JWT management
├── keyring/            # Key storage
├── kms/                # DEK generation, wrapping, and distribution under NAC/DAC
├── lens/               # Schema migration via WASM transforms
├── orbis/              # Threshold BLS signing (Orbis ring client)
├── p2p/                # P2P networking (libp2p + optional Iroh)
├── p2p-adapter/        # P2P adapters for HTTP-facing operations
├── pg-compat/          # Postgres wire protocol compatibility
├── query/              # Query engine (GraphQL, BM25)
├── replication-filter/ # Query-filter-backed replication matcher
├── schema/             # Schema validation
├── vera/               # Vera on-chain ACP client (Cosmos/EVM)
├── storage/            # Storage layer over regolith, the one engine
├── telemetry/          # OpenTelemetry exporter setup
├── wasm/               # Browser client (WebAssembly)
└── zanzibar/           # Google Zanzibar permission engine

tools/
├── apple/                  # Apple embedding: .xcframework build + Swift import smoke test
├── ffi-test/               # FFI compatibility testing against Go
├── hf_embedding_server.py  # Local HuggingFace embedding server for embedding benchmarks
├── integration-test/       # Rust-native integration tests (primary validation)
├── lens-host/              # Standalone WASM Lens transform runner (JSON stdin → stdout)
├── otel-smoke/             # OTLP exporter smoke tests against a Compose-run collector
├── pg-compat-harness/      # Drizzle ORM harness for the Postgres wire protocol
└── vector-bench/           # Corpus, warm-up, memory-cap and usearch tools for vector_disk_cost
```

Only `ffi-test`, `integration-test`, and `lens-host` are workspace members; the
rest are scripts and non-Rust harnesses.

### File Size Guidelines

- Under 200 lines: Fine
- 200-400 lines: Check if doing one thing
- Over 400 lines: Consider splitting

## 5. Naming Conventions

| Thing | Convention | Example |
|-------|------------|---------|
| Crates | lowercase, hyphens | `defra-core` |
| Files/Modules | snake_case | `lww.rs` |
| Types | PascalCase | `LwwDelta` |
| Functions | snake_case | `encode_priority()` |
| Constants | SCREAMING_SNAKE_CASE | `STATUS_ACTIVE` |

## 6. Comments Policy

**Minimal comments. Code should be self-documenting.**

✅ Comment: Non-obvious WHY, safety invariants, public API docs (`///`)

❌ Don't: What the code does, TODO/FIXME, commented-out code, change history

## 7. Git Worktree Workflow

```bash
cd ../defradb.rs-foo     # Work on feature foo
cd ../defradb.rs-bar     # Work on feature bar
```

Each worktree is isolated, no branch switching overhead.

## 8. House Rules

The bare minimum for every change. A change that breaks one of them does not merge. They describe the architecture
defradb.rs is moving to: regolith owns every transaction mechanism, defradb.rs runs thread-per-core on top of it, and
the network stays at the edge. Code that does not fit yet is moved toward it, never further away.

### The boundary: regolith owns the transaction mechanics

- regolith (at `IsolationLevel::DefraLevel`) owns everything transactional: validation, conflicts and their reasons,
  retries (`transact`, `transact_async`), merges, commit-ordered append, ID allocation, the commit receipt, encryption
  at rest. defradb.rs implements regolith's traits and calls its methods:
  - `KeyClassifier`: one `match` on key prefixes;
  - `MergeOperator`: counters, head sets, document part operands and their `touches`;
  - `LogLayout`: the arrival journal's keys;
  - `EventListener::on_conflict`: conflict telemetry;
  - transaction callbacks and `TransactionHooks`: post-commit work;
  - `KeyProvider`: the keys for encryption at rest. Document field encryption through the KMS stays in defradb.rs.
- defradb.rs carries no transaction mechanics: no lock around storage, no retry loop, no read outside a transaction
  feeding a decision inside it, no key-layout trick to dodge a conflict, no pending state, no second commit, no guard.
  A conflict problem is solved by a regolith mechanism; any other fix is a plain correction to a write path. Changes
  only ever remove coordination from defradb.rs, never add it.
- Nothing valid is rejected. Fail loud only about a state the code cannot know, with an actionable message; a feature
  that is not built yet is built, not refused.

### regolith's traits: implemented fully, on the latest regolith

- defradb.rs pins the latest regolith and implements every regolith trait its use of the engine calls for, fully and
  correctly: every method whose default would lose behaviour is implemented, every contract in the trait's rustdoc is
  honoured, and each implementation is tested against that contract (regolith's `testing` checks, such as
  `check_merge_operator` and `check_touches`, where regolith ships one; a contract test of its own otherwise).
- The set, and what each one covers:
  - `KeyClassifier`: every key family classified as the reads-and-writes rules below require; unknown or malformed
    keys stay Ordinary;
  - `MergeOperator`: `full_merge`, an exact `partial_merge` where the operation allows one, and `touches` for every
    operand that carries parts;
  - `LogLayout`: the arrival journal's head, entry and doc keys;
  - `EventListener`: every event the node reports or acts on: conflicts by reason, background errors, callback panics,
    a discarded WAL tail, flush, compaction and table-file events for telemetry;
  - `TransactionHooks` and the per-transaction callbacks (`before_commit`, `on_commit`, `on_abort`): post-commit work;
  - `KeyProvider`: the current key and every older key a store still names, so a key rotation works;
  - `CompactionFilter`: data compaction may drop (expired data);
  - `PrefixExtractor`: defradb.rs's prefix families, so prefix seeks use the prefix bloom filters.
  Where regolith ships an implementation that fits (`Env`, `RateLimiter`), use it.
- When the regolith pin moves, every implementation is rechecked against the new trait docs, and a trait regolith adds
  is implemented in the same change.

### Reads and writes: pick the mechanism by what the decision depends on

Reads:
- a query, a status read, an arrival page: a snapshot read or a write-free transaction, one snapshot, never validated,
  never conflicting;
- a decision on a whole value: `get` (an Ordinary key, validated at commit; an identical rewrite is not a change);
- a decision on some parts of a document: `get_parts(key, parts)`, validated only against writes that touch those parts;
- traversal hints that decide nothing (HNSW neighbours): `get_parts(key, &[])`, never validated;
- content-addressed blocks: a ContentAddressed read; the arrival journal: a Log read;
- a set that decides (ACP owners and tuples, a claim queue): `scan_validated` or a cursor with `ScanCheck`, so a
  write inside the range aborts the decision;
- a head set: one `get_parts(key, &[])` point read of the set's key, never validated.

Writes:
- never conflict: blocks (`b` + CID, ContentAddressed puts), head sets (one key per set, blind merges of the head-set
  operand), document values (blind merges of part operands), secondary index entries (blind puts and
  deletes), block ownership (blind puts), counters (blind merges with an exact `partial_merge`), the arrival journal
  (`append` with the document as its once key), short IDs and policy IDs (`Db::allocate`);
- one winner by design, surfaced as a `Conflict` with its reason: a unique index key, the DocID mapping, the deletion
  marker, the definition key against its writers, ACP owners and tuples in the authorized transaction, the applied key
  of a remote block, the same document parts written by two updates.
Disjoint parts of one document never conflict. A read that validates more than its decision needs is a defect.

### Transactions, retries and conflicts

- Every write runs in `transact` or `transact_async`. Its closure may run more than once, so everything with an effect
  outside the transaction is prepared before it: KMS keys, embeddings, signatures, short IDs. Nothing remote is
  fetched inside a closure; an input that depends on the transaction's reads ends the attempt, and the attempt runs
  again once the input is local, memoized by the exact request bytes.
- A conflict is handled as business logic keyed on regolith's `Conflict` reason: an already-deleted document is a no-op
  or not-found; a re-run after a changed part merges its writes again; a lost unique-key or compare-and-set race is
  reported to the client; a revoked ACP tuple denies; a changed definition re-runs against the new one. One retry
  policy for every attempt, from configuration, through one function.
- Interactive (explicit) transactions return their conflicts to their caller; a failed or cancelled statement rolls
  back to its savepoint.
- Post-commit work runs as regolith transaction callbacks: exactly one outcome per attempt. Once a commit is durable
  its outcome stands: a follow-up that fails is reported and retried durably, never turned into a failed write.
- Events and in-memory state follow the commit order through `CommitReceipt.seq`. A `seq` is never persisted across
  `drop_all` or a restore.

### Execution boundaries to preserve

- Preserve the embedded autocommit owner in
  [`execute_autocommit_request`](crates/defra-node/src/lib.rs) and its signed
  caller in [`signed_query`](crates/defra-node/src/signed_query/mod.rs).
  `prepare_request` and `resolve_retry_input` run on the caller, outside database
  workers; `execute_prepared` routes the database attempt. Replacing this sequence
  with `PooledExecutor::execute` moves remote preparation onto a database worker.
  The placement contract is in [`PooledExecutor`](crates/query/src/pooled/mod.rs);
  the one-preparation/one-attempt regression is in
  [`request_preparation_tests`](crates/defra-node/src/request_preparation_tests.rs).
- Preserve the worker-local commit path in
  [`WorkerIo`](crates/storage/src/backends/regolith/worker_io/mod.rs) and
  [`RegolithTxn::commit`](crates/storage/src/backends/regolith/transaction.rs).
  A cache-only transaction commits with `commit_nowait`; its I/O queue completes
  the ticket. Do not move commit validation to `Pool::spawn_blocking`, call
  blocking `Transaction::commit` on a database worker, or move the transaction to
  another executor to hide a cache miss. `WouldBlock::Io` requires a Regolith
  asynchronous mechanism with the transaction's ownership intact. A missing
  mechanism belongs in Regolith; preserve its regression rather than add an
  adapter retry or blocking fallback. Regolith's owning contracts are
  [`commit_nowait`](https://github.com/sourcenetwork/regolith/blob/main/src/transaction/nowait.rs)
  and [`transact_async`](https://github.com/sourcenetwork/regolith/blob/main/src/transaction/retry.rs).

### Synchronization: regolith sync, lock-free first

- Synchronization comes from `regolith::sync`. Never `std::sync::{Mutex, RwLock, Condvar, Barrier, OnceLock, LazyLock}`,
  `parking_lot`, `tokio::sync` or `async_lock`. This holds for every line of the workspace, tests and tools included:
  the final code has no `std::sync::Mutex` and no `std::sync::RwLock` anywhere.
- Lock-free wherever it applies. Reach for these first:
  - a shared value: `regolith::sync::Atom`, swapped as a whole immutable snapshot, so readers never wait;
  - a shared map: `regolith::sync::HopscotchMap`, with the rapidhash fast hasher (below);
  - a queue: `regolith::sync::ArrayQueue` (bounded) or `SegQueue`;
  - work handed between tasks or cores: `regolith::sync::bounded` and `unbounded` channels;
  - a counter or a flag: a plain atomic;
  - one-time initialization: `regolith::sync::OnceCell` and `Lazy`.
- Only where waiting is part of the meaning (one task must wait for another) use a `regolith::sync` future: `Mutex`,
  `RwLock`, `Semaphore`, `Notify`, `Event`, `Latch`, `Barrier`. They suspend the task and never block the thread.
- Never add a lock to make a value `Sync`, to serialize writers, or to avoid a transaction conflict. Shared durable
  state goes through regolith; shared memory state goes into a lock-free structure; state one task owns is a plain
  owned value.

### Hash maps: rapidhash, fast

- Every hash map and hash set is `rapidhash::RapidHashMap` or `rapidhash::RapidHashSet`, the fast hasher
  (`rapidhash::fast::RandomState`; the `rapidhash::fast::RapidHashMap` path is a deprecated alias of the same type),
  built with `new()`, `default()` or `with_capacity` (`rapidhash::HashMapExt`, `HashSetExt`). Never
  `std::collections::HashMap` or `HashSet` with the standard hasher, and never the `rapidhash::quality` variant.
- A map shared between threads is kovan's `HopscotchMap` (re-exported as `regolith::sync::HopscotchMap`) built with
  `rapidhash::fast::RandomState` (`HopscotchMap::with_hasher`), not with its default hasher.
- This holds for every line of the workspace, tests and tools included: the final code has no
  `std::collections::HashMap` or `HashSet`. A type that needs `std`'s map at an API boundary (a deserializer, a third-party
  signature) takes it as `HashMap<K, V, rapidhash::fast::RandomState>`, which is `RapidHashMap`.

### Thread-per-core

Every code path is written for a thread-per-core (TPC) architecture. Throughput and performance are the key: a design
that is correct but slower is not done.
- **Workers.** One pinned worker per core, each running its own local executor (not tokio) with no task stealing. Each
  owns a run queue, a bounded inbox other threads push to, a deadline queue (timers, periodic jobs, the idle sweep,
  read through the Env clock), its own regolith `IoQueue`, and the transactions it owns, held by value.
- **Data stays local to its thread.** A worker's state, buffers, caches and transactions live on that worker and are
  touched only by it; data moves to another thread only as a message, never as shared memory. Read-mostly state
  (schema and collection definitions, the ACP policy snapshot, lens stores) is copied per worker or published as an
  `Atom` snapshot; an update is a broadcast message.
- **Tasks stay on their core.** Spawned with `spawn_local`, they run where they were placed until they finish; a waker
  pushes onto its own core's run queue. Data-path futures need not be `Send`.
- **Routing at the edge.** No request hops from worker to worker. An explicit transaction's requests go to its owner,
  which the transaction ID names; a single-document write and a P2P merge go to the worker chosen by a hash of the
  collection and the document ID, which runs one task per document at a time; read-only queries go to the
  least-loaded worker. Correctness never depends on placement: regolith validates every commit wherever it runs.
- **Nothing blocks a thread.** No `block_on`, `block_in_place`, `thread::sleep`, spin-wait or polling loop on the data
  path. regolith calls run inline on the calling core; reads use `ReadMode::CacheOnly` on the worker's queue and a miss
  awaits its `IoWait`; commits use `commit_nowait` or `transact_async` and await their `CommitTicket`. Every wait is an
  `.await` on something the worker's own core completes.
- **Long tasks yield** after a bounded slice (a page), so other tasks on the core run; cursors hold no borrow across
  the yield.
- **Admission and bounds.** Overload waits at the network edge on a `regolith::sync::Semaphore`; inboxes and every
  queue and buffer are bounded, by bytes where items vary in size. Stream: hold a page, never the whole set.
- **One worker.** The same code runs with a single worker (the wasm main thread, a single-core embedded target, the
  application's event loop polling the worker's `IoQueue` when idle). Nothing requires a second thread.
- **Idle.** A worker with nothing to run spins briefly, then parks on its `IoQueue` and inbox; a busy worker is never
  woken.

### The network edge

- One tokio runtime, held in one place, serves all networking (HTTP, pg-compat, P2P, the Orbis, Vera and KMS clients,
  embeddings, telemetry export). It is passed in as a narrow handle at construction; nothing finds a runtime ambiently
  (`Handle::current`), and no other runtime is built.
- The data-path crates (storage, datastore, blockstore, crdt, defra-core, query, db, acp, zanzibar, lens, kms, events)
  do not depend on tokio: no `tokio::spawn`, `tokio::time`, `Handle::current`, `block_in_place` or `block_on`.
- A network task that panics fails only its own request. Every completion a worker waits on carries a deadline.
- Request context (identity, signing configuration, the DAC bypass) travels with the task or message, never in a
  thread-local or task-local.
- Shutdown in one order: stop accepting, drain in-flight requests with a deadline, stop the workers, close regolith,
  drop the runtime outside any async context.

### Remote inputs: KMS keys, signatures, Vera decisions, embeddings

- Resolved at the network edge before the transaction that needs them, never inside it.
- Each input kind has one service: single-flight (a lock-free map from the request key to a `regolith::sync::Event`),
  batching of requests that arrive while one is in flight, a cache bounded by bytes where the input allows one, a
  deadline, bounded jittered retries on the Env clock, an in-flight cap, and metrics (latency, round trips per
  operation, cache hits, batch sizes).
- A P2P merge is admitted only once its blocks and KMS keys are local. Creates sign their blocks in waves before the
  transaction; a signed block is never redone.

### ACP

- Every ACP read and write runs in the transaction it authorizes. Deciding point reads are validated; deciding scans are
  validated scans. A revocation, grant or registration that commits first aborts the decision, and the re-run decides
  again.
- Registration happens in the create transaction, only on the create branch (never for an existing document an upsert
  found); unregistration in the delete transaction. No window where a new document is public.
- Policy IDs come from `Db::allocate`. A policy is created if absent; a different body under an existing ID is a typed
  error. The check path writes nothing. The policy snapshot is an `Atom`, swapped on load and reload.
- Read-only queries decide ACP at the same snapshot as their rows. Remote ACP (Vera) stays at the network edge, with
  its staleness documented.

### Proofs

- No logic change without its models. A change to what the code does ships with its TLA+ model and its Lean proof
  in the same change, and both pass before the code is written or merged. That covers behavior, an ordering, a
  protocol, retries, recovery, conflict rules, an encoding, and a data structure's invariants. A model that does
  not pass means the design is broken: it is not implemented until the design is fixed and TLC and Lean pass. A
  change that keeps behavior identical (a refactor, a speedup with identical results, tests, docs) still updates
  the correspondence of every model whose code it moves.
- Anything temporal or concurrent (an ordering, a protocol between tasks or cores, retries, recovery, a race,
  progress) has a TLA+ model. Every data structure (its invariants, encodings and round-trips, merge and operator
  laws) has a Lean proof. This is the bare minimum, and the model is written before its code: the spec is the
  contract.
- Each TLA+ model has GREEN configurations and, for every guard, a RED configuration that removes or breaks that
  guard and must fail on its named invariant.
- Each TLC configuration finishes in under 60 seconds, and the whole suite in under 10 minutes. A slow model is
  shrunk, never given a longer timeout or more shards.
- Lean proofs use no `sorry`, `admit` or new `axiom`.
- A model names the Rust code it models (file and function), and is wired into `proofs/tla/run-all.sh` and
  `proofs/src/registry.rs`. A model that no longer matches the code is a defect.
- Proofs read ELI5, line by line, for someone who knows nothing about proofs or model checkers:
  - each file opens with a plain-words story: who acts, what can happen, what must never happen;
  - every line of a spec has a plain-words comment (`\*` in TLA+, `--` in Lean), and every definition, action and
    non-trivial expression is explained;
  - each RED configuration says in plain sentences which bug it plants and what goes wrong;
  - each theorem and invariant opens with a one-sentence plain statement of what it promises.

  Proof files are the exception to the comments policy (section 6).

### Performance

- Reads are zero-copy where regolith offers it; one boundary crossing per batch, never per row.
- Every change that can move a number ships before-and-after numbers: commits per second, conflicts per commit,
  retries per layer, p50 and p99 at 1, 4, 16 and 64 writers on creates, shared updates and counters with indexes,
  plus allocations per operation. No change may raise the conflict rate or lower throughput.

### Code shape

- Trait bounds go in a `where` clause, on functions, impls, types and traits alike, never inline in the generic
  parameter list:

  ```rust
  fn apply<T, S>(value: T, store: &S) -> Result<()>
  where
      T: Encode + Send,
      S: Store,
  ```

  not `fn apply<T: Encode + Send, S: Store>(value: T, store: &S)`.
- Callback shapes are named traits or `type` aliases, with lifetimes that have meaningful names.
- Errors use `thiserror`, and only `thiserror`, everywhere: library code, binaries, tools and tests. Each crate has its own
  error enum derived with `thiserror`; a public API returns that crate's `Result`. No `anyhow`, `eyre` or
  `Box<dyn Error>` in a signature. A test returns `()` and asserts, or returns the crate's `Result`. Remove `anyhow`
  wherever it is found.

## Build Dependencies

- **Rust** (1.99+): `rustup`
- **protoc**: `brew install protobuf` — required by `crates/orbis`, whose `build.rs` compiles `proto/orbis.proto` via `tonic-prost-build`
- **cbindgen**: `cargo install cbindgen` — generates C headers for FFI
- **cargo-nextest**: `cargo install cargo-nextest --locked` — runs the tests; `.config/nextest.toml` keeps tests that start nodes serial
- **Go** (1.26+): Required for FFI compatibility tests, and to build the pinned `GO_COMPAT_COMMIT`, which declares `go 1.26.7`

## Common Commands

### Integration Tests (Primary Validation)

```bash
# Run all integration tests
cargo nextest run -p integration-test

# Run a specific area
cargo nextest run -p integration-test --test acp
cargo nextest run -p integration-test --test p2p
cargo nextest run -p integration-test --test basic

# Run a specific submodule within an area
cargo nextest run -p integration-test --test acp -- negative::

# Run a specific test
cargo nextest run -p integration-test --test acp -- basic::rust_acp_basic
```

Integration tests live in `tools/integration-test/tests/` and exercise the full
Rust node via CLI + HTTP API. Each area is a `[[test]]` binary with submodules:

| Area | Binary | Modules |
|------|--------|---------|
| Basic | `--test basic` | batch_mutations, collection_delete_4657, crdt_input_ops, collection_management, document_lifecycle, regolith_crash_reopen, multi_collection, patch_secondary_relation_4709, self_ref_relations_4712, smoke, transactions, truncate_parallel |
| Query | `--test query` | commits_aggregate, commits_collection_id, commits_height_filter, continuous_rollup, datetime_index_range, default_values_v1, downsample, downsample_gc, exhaustive_orphans_4454, explain_nested, gql_list_args, index_fallback_4633, index_management, lens, lens_persistence, lens_reindex_secondary_index_979, limits, multi_cid_vectors, planner_4656, planner_4684, sdl_generate, subscription_docid, subscription_root_shape, view |
| ACP | `--test acp` | audit, basic, cross_object, custom_policy, events_sse, index, link_collection, multi_identity, multi_role, negative, negative_p2p, node_access, p2p, p2p_lifecycle, policy_validation, register_ops, relation_queries, relationship, revoke_lifecycle, secp256k1_round_trip, transaction_rollback, xarchive_access_matrix |
| P2P Iroh | `--test p2p_iroh` | acp, connection, peer, replication, schema, sync |
| NAC | `--test nac` | core_operations, cross_compartment_isolation, dac_access_matrix, document_acp, multi_doc_create, operations, p2p_management, policy_evolution, relation_admin |
| P2P | `--test p2p` | connection_manager, document, feature_binaries, filtered_replication, idempotent_replay, long_history, manage_relay, management, quarantine, receiver_pull, replication, replication_advanced, resilience, sync, text_crdt, transports, trust_boundary, write_contention |
| FTS | `--test fts` | basic, edge_cases, lifecycle, relation_paths, scoring |
| Vector | `--test vector` | CLI/HTTP Flat, HNSW, IVF-PQ and SSG routing, updates, deletes, and restart persistence |
| Encryption | `--test encryption` | acp, block_verify, cross_runtime_p2p, index, key_management, se_cross_runtime |
| Identity | `--test identity` | keyring_dev_mode, keyring_lifecycle, lifecycle, negative, node_identity, types |
| Backup | `--test backup` | dev_mode, dump, purge, restore |
| Cursor | `--test cursor` | composite_index, error_paths, reindex_datetime_visibility, smoke |
| Vera | `--test vera` | acp_tuning, compartments, encryption_acp, p2p_acp, policy_lifecycle, resilience, smoke |
| Vera.rs | `--test verars` | compartments, p2p_acp, policy_lifecycle, smoke |

Single-purpose binaries, each its own `[[test]]` with no submodules:

| Binary | Covers |
|--------|--------|
| `--test p2p_admission` | replicator fan-in against a tiny pending-DAG cap (injects `DEFRA_P2P_MAX_PENDING_DAGS`) |
| `--test p2p_admission_restart` | hub restart must not lose success-acked pending-DAG registrations |
| `--test p2p_admission_fairness` | per-source-peer pending-DAG quotas under a noisy pusher |
| `--test p2p_backlog` | outbound push-backlog bounds and overload observability |
| `--test p2p_push_storm` | outbound push-storm reproduction and profiling harness |
| `--test p2p_deep_catchup` | deep full-DAG push under intake rate limiting |
| `--test p2p_interop_bench` | cross-runtime P2P replication cost |
| `--test issue1154_repro` | at-scale merge of every success-acked document after hub restart |
| `--test issue1194_repro` | concurrent updates to distinct documents must not conflict |
| `--test issue1211_repro` | same, through a branchable collection's head set |
| `--test issue1294_bytes_json` | Blob-as-Bytes create+query must return lowercase hex |

### Rust Commands

```bash
cargo nextest run                  # Run all unit tests (cargo test --doc for doctests)
cargo nextest run -p crdt          # Test specific crate
cargo clippy --all --all-targets -- -D warnings  # Lint, benches included
just check-node-graph              # Feature-graph contracts for defra-node
cargo fmt --all                    # Format
cargo build --release              # Build release
just test-wasm                     # Browser wasm tests in headless Firefox
just test-browser-p2p              # Browser peer ↔ native node through a hosted iroh relay
```

### Build Profile in Worktrees

Every worktree keeps its own `target/`, and a stock dev build retains one debug
object per codegen unit forever, so concurrent worktrees add up fast. The
justfile's `profile` variable (default `dev`) selects the profile for the build
and test recipes; `super-dev` keeps `file:line` backtrace frames for workspace
crates while dropping dependency debug info, for ~7.5x less object volume.

```bash
just profile=super-dev test              # unit tests, into target/super-dev/
just profile=super-dev integration-suite acp
```

The override must precede the recipe name. Prefer it in agent worktrees; stay
on `dev` when stepping through code in lldb.

`just sweep` (keeps 7 days) deletes this worktree's stale artifacts. The
profile shrinks each build generation; sweeping bounds how many pile up.

### Tracking Go Upstream

```bash
# Go repo location — point DEFRADB_GO_REPO at your Go DefraDB checkout
cd "$DEFRADB_GO_REPO"

# Check what's landed on develop
git fetch origin develop
git log origin/develop --oneline -20

# Compare with our last sync point
git log origin/develop --oneline --since="1 week ago"
```

The Go repo has two remotes:
- `origin` → `sourcenetwork/defradb` (upstream)
- `fork` → `edjroz/defradb` (our fork; the rustffi client branches
  `edjroz/ffi-rust-compat` and `jack/ffi-rust-compat` both live on `origin`)

`DEFRADB_GO_REPO` covers the shell commands above and `ffi-test`, which also
accepts `--go-path <PATH>` to override it. Integration tests that consume Go
build artifacts resolve them at compile time under
`$HOME/go/src/github.com/sourcenetwork/defradb` — see `COPY_WASM_PATH` in
`tools/integration-test/tests/p2p_iroh/sync/version.rs`. A checkout elsewhere
needs a symlink at that path, or the `p2p_iroh` lens tests fail on a missing
WASM binary.

### Git Worktrees

```bash
git worktree list                                  # List worktrees
git worktree add ../defradb.rs-foo -b feat/foo     # Create worktree
git worktree remove ../defradb.rs-foo              # Remove worktree
```

## Before Committing

1. `cargo nextest run` and `cargo test --doc` pass
2. `cargo clippy --all --all-targets -- -D warnings` clean
3. `cargo fmt --all` applied
4. If touching core behavior: `cargo nextest run -p integration-test` passes

## ACP / Searchable Encryption

- When fixing ACP (Access Control Policy) filtering, always verify BOTH User queries AND Commits queries are filtered. These are two separate code paths that both require ACP checks.
- After fixing any ACP-related code, run the full ACP test suite not just the immediately failing one.

## Storage

[regolith](https://github.com/sourcenetwork/regolith) is the storage engine, on
every target: server, embedded, and WASM. There is no other backend and no
feature flag selecting one.

`--store` chooses whether anything is persisted:

| Value | Meaning |
|-------|---------|
| `regolith` | A regolith database on disk. The default. |
| `memory` | A regolith database kept in memory, which nothing outlives. |

The names of the backends regolith replaced (`lark`, `redb`, `fjall`, `rocksdb`,
`badger`) are still accepted and resolve to `regolith`, so an existing config
file keeps loading.

Profiles come from regolith itself: `Options::default()` for a server,
`Options::embedded()` for a 1-4 MiB working set, and `Options::wasm()` for a
browser or wasi module. Every transaction runs at `IsolationLevel::DefraLevel`,
with defradb.rs's `KeyClassifier` installed through `with_policy`: each key
family's class (Ordinary, ContentAddressed or Log) decides how
its reads and writes are validated (section 8), with `DurabilityMode::Immediate`.

## Goal

**New contributor feels ready to do productive work immediately.**

Fast context acquisition → Confident changes → Productive iteration.
