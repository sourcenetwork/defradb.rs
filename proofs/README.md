# Formal methods for defradb.rs

Machine-checked models of defradb.rs's correctness-critical protocols. Two tools,
split by the kind of obligation each is good at:

| Dir | Tool | Proves | Entry |
|---|---|---|---|
| [`tla/`](tla/) | TLA+ / TLC | **temporal / distributed / concurrent** properties — convergence, "eventually merges", safety under adversarial interleavings | [`tla/README.md`](tla/README.md) |
| [`lean/`](lean/) | Lean 4 | **functional / algebraic** properties — CRDT merge laws (commutativity, associativity, idempotence), order-independence | [`lean/README.md`](lean/README.md) |

Some results need both. **DAG convergence = (TLA+: every node receives every delta
under eventual connectivity) × (Lean: applying deltas in any order yields the same
state).** Neither half alone is convergence.

## Coverage map

Status of the effort across the 40-crate surface, from the per-crate survey in
[`survey/`](survey/) (full index, incl. out-of-scope rationale, in
[`survey/INDEX.md`](survey/INDEX.md)). Of 34 crates, **12 are model-worthy**; the other 22
are plumbing covered by integration tests / Go-FFI parity / unit tests. The point of this
section is the **diff**: what is proven vs. the accepted gap.

### Modeled: 46 families (bounded model claims)
| Family | Tool | Crates it covers |
|---|---|---|
| Storage failure classification | TLA+ / Lean | storage, db, query, http |
| B3 filtered replication | TLA+ | p2p, db-merge |
| DAG convergence (partition / eviction / restart) | TLA+ | p2p, db-merge, blockstore |
| CRDT merge laws | Lean | crdt, defra-core |
| Replicator lifecycle (no-loss / resume) | TLA+ | p2p, db-merge, embedded |
| Sync ownership transfer (head hint / receiver pull) | TLA+ | p2p, db-merge, storage, embedded |
| Multi-instance claim | TLA+ | gents |
| Block integrity / signatures | TLA+ | db-merge, defra-core, blockstore |
| KMS key distribution | TLA+ | kms, db-merge |
| Encrypted LWW restart/replay | TLA+ + existing LWW Lean law | kms, p2p, db-merge, crdt |
| Management-channel auth (NAC gate) | TLA+ | http, db-nac, pg-compat |
| ACP soundness + revocation + dual-path commits | TLA+ & Lean | acp, zanzibar, vera, query, db |
| Storage transaction validation | TLA+ | storage |
| P2P explicit-replay capability gate | TLA+ | p2p |
| NAC lifecycle privilege-escalation | TLA+ | acp, db-nac |
| Transaction & merge-queue concurrency | TLA+ | db, db-merge |
| Document materialization status convergence | TLA+ & Lean | db-merge, crdt, db |
| JWT issuer / algorithm binding | TLA+ | identity |
| CID content-addressing determinism + Block canonicalization | Lean | defra-core |
| Deferred-ACP overlay consistency | TLA+ | query |
| Order-preserving key encoding | Lean | storage |
| DiskANN resident code table | Lean | db-index |
| DiskANN walk cutoff | Lean | db-index (vector) |
| PQ distance four sums | Lean | db-index (vector) |
| Index-maintenance consistency | Lean + TLA+ | db-index, db-merge |
| Concurrent collection-head transitions | TLA+ & Lean | db-block-builder, storage |
| Commit-ordered document arrivals | TLA+ | db, storage (pinned regolith) |
| DiskANN pending cache | TLA+ & Lean | db-index (vector), storage |
| Retry input and marker fencing | TLA+ | db, query, storage |
| Collection schema-version OCC | TLA+ | db |
| Schema publication completion | TLA+ / Lean | storage, db |
| Peer connection observation | TLA+ / Lean | p2p |
| Explicit transaction statement atomicity | TLA+ | query, db, storage |
| Engine ticker export | TLA+ & Lean | telemetry, storage |
| Worker pool CPU ledger | TLA+ & Lean | workers |
| Worker default pool | TLA+ & Lean | workers |
| Node shutdown order | TLA+ | defra-node, embedded, edge, acp |
| Inbound stream admission | TLA+, Lean | p2p |
| Vector index link maintenance | TLA+ | storage, db-index |
| Peer fetch stats | Lean | p2p |
| Gossip connection ownership | TLA+, Lean | p2p |
| Iroh service records | TLA+, Lean | p2p |
| Iroh solicited replies | TLA+ | p2p |
| Reply-driven DAG fetch | TLA+ | p2p |
| Text CRDT convergence | TLA+ & Lean | crdt, db-merge |
| JSON CRDT per-path merge | TLA+ & Lean | crdt, db-merge (specified before the code, #2129) |
| Custom CRDT types | TLA+ & Lean | crdt, schema, db-merge, p2p (#2183) |

### Backlog — current retry-era gaps
This inventory includes legacy models and does not mean every current mechanism is
proven. Engine OCC remains an implementation boundary. `HeadSet.tla` models derived head
keys and scan/write conflicts, not collection-version validation. Remote ACP/Vera checks,
dynamic KMS policy/scope changes, cold
read-time decryption, and complete attempt-level preparation coverage also remain audit
gaps. The new retry input and peer marker models cover only their stated protocol
abstractions. Reconcile these surfaces against the survey before making completeness claims.

### Deferred — Lean-lemma appendix (13 low)
Low-risk hardening lemmas parked for later: index-extraction determinism, cartesian-product
laws, LWW tie-break total order, storage encode round-trip, EncryptedStore key-binding,
capability rate-limiter fairness, Merkle batch-root determinism, DocID parse round-trip,
token clock-skew validity, DID-derivation determinism, schema update-immutability,
single-active-version invariant, blockstore cache↔storage coherence. (Full rows: `survey/INDEX.md` §2.)

### Out of scope — 28 crates
Plumbing/glue with no novel modelable invariant; one-line rationale per crate in
`survey/INDEX.md` §3 (cli, ffi, wasm, http-transport, storage-encoding-glue, …).

## Build / run everything

Prereqs: **Java 11+** (TLC) and **Lean via elan** (`lean-toolchain` pins the version).

```bash
# everything — TLA+ red/green oracle + Lean build, single exit code:
proofs/verify-all.sh

# or each half on its own:
cd proofs/tla && ./run-all.sh   # all TLA+ models (red+green oracle, non-zero on any mismatch)
cd proofs/lean && lake build    # all Lean proofs (builds clean, zero `sorry`)
```

`proofs/tla/tools/tla2tools.jar` is git-ignored; the wrapper re-downloads it if missing.
It pins the stable TLC 1.7.4 release by SHA-256. The upstream 1.8.0 asset is a
rolling pre-release and is intentionally not used for a reproducible gate.

## Conformance — binding the models to the real binary

The models prove properties of *abstractions*. The `conformance` crate (rooted
here in `proofs/`, alongside `tla/` and `lean/`) keeps them honest against the
shipped code, on two axes matching the two tools:

| Axis | What it checks | Needs a binary? | Entry |
|---|---|---|---|
| **Lean (auto)** | `lean/Conformance.lean` emits a JSON contract (vocabularies derived from the Lean models); the live Rust types are asserted to still match — anti-drift | no | `tests/lean_conformance.rs` |
| **TLA (behavioral)** | each family's invariant is driven against the **running DefraDB binary** via the backbone `defra-harness` | yes | `tests/tla_conformance.rs` |

[`src/registry.rs`] (`PROPERTIES`) is the spine: every *Modeled* family above maps
to its headline invariant, source anchor, the model that proves it, and its
binding tier (`Behavioral` / `Contract` / `Boundary`). `Boundary` marks what is
*assumed*, never asserted (crypto, eventual connectivity, bounded-N, foreign
substrate) — so a green run never reads as "this was proven against the
artifact." `matrix::every_modeled_family_is_bound` fails if a model lands without
a binding.

### Realized status — all 21 families bound

**19 behavioral tests** (driven against fresh `target/debug/defra`, each break-tested
for non-vacuity), **2 Lean-axis contract bindings**, **7 honest Boundaries** (one,
Transaction & merge-queue concurrency, is now both — a Behavioral no-loss/no-double-apply
storm leg plus a Boundary internal-serialization leg). One
of these (`partition::convergence_concurrent_same_doc_writes_merge`) found a real
Rust-specific CRDT convergence bug — divergent materialization on identical DAGs
after a restart — which is now **fixed** in `crates/db (merge)/.../field_state.rs`
(`reseed_from_document` re-seeds the datastore LWW from the authoritative
headstore); go↔go vs rust↔rust parity (`parity.rs`) localized it to Rust.

| Family | Binding | Where |
|---|---|---|
| B3 filtered replication | Behavioral | `replication.rs` |
| DAG convergence | Behavioral | `replication.rs` (live-forward) + `partition.rs` (partition heal; concurrent same-doc merge — the bug it found+fixed) |
| Replicator lifecycle | Behavioral | `replicator_lifecycle.rs` (backfill no-loss + resume across node restart) |
| Sync ownership transfer | Boundary + external integration | deterministic full-DAG/head-hint A/B fence; `p2p_admission_restart`; marker migration/storage tests; mixed Go/Rust PushLog fixture. These run in repository CI, not this conformance binary's `tests/behavioral/` harness. |
| ACP soundness + revocation + commits | Behavioral + Contract | `acp.rs`; `RelationExpression` vocab |
| Storage transaction validation | Behavioral | `ssi.rs` (real `409 Conflict` on write-skew) |
| Management-channel auth (NAC gate) | Behavioral | `nac.rs` |
| NAC lifecycle priv-escalation | Behavioral | `nac_lifecycle.rs` |
| Transaction & merge-queue concurrency | Behavioral + Boundary | `partition::{convergence_concurrent_pncounter_signed_deltas_sum,convergence_restart_pncounter_signed_deltas_sum,convergence_concurrent_pncounter_same_doc_merge_storm}` (PNCounter exact signed sum across live, restart, and storm); `partition::convergence_concurrent_same_doc_merge_storm` (no-loss/no-double-apply exact-sum oracle); `partition::{convergence_concurrent_mixed_lww_and_counter_fields_merge,convergence_restart_mixed_lww_and_counter_fields_merge,convergence_mixed_lww_and_counter_3node_full_mesh}` (mixed Counter×LWW exact-state); `MC_MixedFieldMaterialization_{Red_WholeDoc,Green}`; engine OCC and the txn-registry sweep stay Boundary |
| Document materialization status convergence | Behavioral | `partition::convergence_delete_update_race_preserves_tombstone`; `MC_DocumentMaterialization_{Red_Overwrite,Green}`; `DefraConvergence.DocumentMaterialization` |
| Deferred-ACP overlay | Behavioral | `deferred_acp.rs` (txn-local gating) |
| CID determinism | Behavioral | `cid.rs` |
| Index-maintenance | Behavioral | `index.rs` (single-node create/update/delete + indexed LWW restart/merge reconciliation); `MC_IndexReconciliation_{Red_SaveOnly,Green}` |
| KMS key distribution | Behavioral | `kms.rs` (node0 denies the DEK to unauthorized node1) |
| Encrypted LWW restart/replay | Behavioral + Boundary | `partition::convergence_encrypted_lww_restart_merge` binds the decrypted LWW winner after a restart-induced partition; filtered replication binds encrypted-field preservation. Durable acknowledgement-backed retry through a receiver restart remains Boundary; `MC_EncryptedLwwReplay_{Green,Red_*}`; reuses `PriorityReconcile.lwwCM` |
| CRDT merge laws | Contract | `MergeResult` vocab; `DefraConvergence.MixedField` product proof; `MixedFieldMaterialization.tla` |
| Multi-instance claim | Boundary | gents substrate, not this binary |
| Block integrity / signatures | Boundary | needs adversarial peer; Rust verify mandatory |
| P2P capability replay gate | Boundary | P2P-wire internal |
| JWT issuer / algorithm binding | Boundary | DID-binding via `acp.rs`; forge unreachable via CLI |
| Order-preserving key encoding | Boundary | ordered query may sort in memory; consts `pub(crate)` |
| DiskANN resident code table | Boundary | in-memory search cache, not observable on the wire; `DiskAnnCodes`; unit tests in `crates/db/tests/vector/diskann_resident.rs` |

```bash
proofs/verify-all.sh                 # TLA + Lean + conformance (behavioral if binary present)
cargo build -p cli                   # produce fresh target/debug/defra
cargo test -p conformance            # Lean axis + behavioral; DEFRA_CONFORMANCE_BINARY overrides target
```

## Conventions (follow these in new slices)

- **One model family per concern.** Each family = a base spec (`tla/<Family>.tla`) plus
  thin scenario wrappers (`tla/MC_<Family>_*.tla` + `.cfg`) that `EXTENDS` it. Lean
  mirrors this with a barrel module + focused submodules.
- **Red/green TDD-for-models.** Every claim has a config the *buggy* policy violates (a
  TLC counterexample) and a config where the fix holds. A property that only ever passes
  green proves little.
- **Naming:** invariants/safety `INV_*`; liveness as `<>[]...` properties; scenario
  wrappers `MC_<Family>_<Case>`.
- **Source-anchored.** Every family has a `<Family>_DESIGN.md` grounding the abstraction
  in specific defradb.rs / gents source modules (file:line). Model the real code
  paths, not an abstraction in a vacuum.
- **Lean:** mathlib-free, toolchain pinned, **no `sorry`/`admit`**, and `#print axioms`
  status recorded in `lean/README.md`.

## Boundaries — what is proven vs assumed vs out of scope

Read this before trusting a verdict; it states the model's honest reach.

- **Bounded instances.** TLC runs at small N (2–3 nodes, ≤~6 blocks). These are the
  *minimal witnessing shapes* for each property; conclusions are structural (a guard on a
  fetch, a datum on a block), not quantity-sensitive — but this is exhaustive over the
  bound, not a proof for all deployments.
- **Environment assumptions (encoded, not proven):** `ProviderAvailable` (a fetch needs a
  reachable holder of the block); **eventual connectivity** + fair head rediscovery for
  the convergence liveness results; weak fairness on protocol actions.
- **Crypto boundary (KMS):** ECIES is abstracted — a node can use an envelope iff it is
  the intended recipient. Real ECIES/IND-CCA security is assumed, not modeled.
- **Float counter divergence (proven, real):** IEEE-754 addition is not associative, so
  float counters are NOT order-independent — proven by `float_add_not_assoc` in
  `lean/DefraConvergence/LocalState.lean`. The 2026-06-03 model→code audit confirmed both
  Go and Rust ship `Float32/64` counters with raw `+` and no mitigation: a real latent
  replica divergence (fix is a Go-parity-constrained product decision, tracked separately).
- **Excluded by design:** key rotation (a node revoked *after* already holding a key is
  out of scope). Model B's `INV_DagComplete` is deliberately relaxed (see `tla/README.md`).
- **Audit outcomes (2026-06-03):** model→code audit of all 9 families confirmed the GREEN
  claims hold in Go+Rust, and surfaced real Go-side gaps the models predicted — `_commits`
  is ACP-ungated in Go (Rust gates it; a known DefraDB limitation), and Go's P2P block
  signature verification is optional with an unverified author/creator field (Rust is
  hardened: mandatory verify + author-binding). See the slice `*_DESIGN.md` for details.
- **Model ≠ code.** These prove properties of the *models*. They are anchored to source by
  the `*_DESIGN.md` docs and bound to the implementation by the `conformance` crate (see
  *Conformance* above) — the Lean axis asserts model vocabularies against live Rust types,
  the TLA axis drives the selected DefraDB binary. What conformance does **not** reach is marked
  `Boundary` in the registry (crypto, eventual connectivity, bounded-N, the gents
  claim substrate): assumed, never asserted against the artifact.

## Regolith adoption bindings

Document arrivals use the pinned engine's `Transaction::append`, with the document
arrival marker as `once_key`. `crates/db/src/event/arrivals.rs` never allocates a
position itself; rows and their cursor positions appear only after commit. The
binding in `src/registry.rs` is to
[CommitOrderedAppend.tla](https://github.com/sourcenetwork/regolith/blob/e844baefe838f3af76403afd46c4b42f1a0b98a5/proofs/tla/CommitOrderedAppend.tla).
Its GREEN Groups/Pipeline/Faults configurations cover dense numbering, commit
order, at-most-once keys and snapshot prefixes. Counter, OnceFromView and
AssignBeforeValidation are negative controls. The engine's model is kept at the
same revision as Cargo.lock rather than copied into this repository. DefraDB's
`arrival_transactions` tests exercise reverse commit order, rollback, duplicate
once keys and reopen through the adapter.

`RetryPreparedInputs.tla` models exact-context provider answers resolved outside
an attempt and reused after a commit conflict; its RED cases run provider work
inside an attempt or clear the memo and repeat provider work. It does not establish that every remote ACP/Vera/KMS/decrypt call
is lifted out of retry attempts. `RetryMarkerGeneration.tla` models generation
compare-delete and the peer-incarnation fence on clear across forget/re-add.
`RetryPushAdmission.tla` checks that queued pushes still own the exact live marker
and satisfy backpressure at admission. Its mutants remove presence, generation
or deadline checks; already-admitted external I/O is outside that boundary. Their
conformance tier is deliberately `Boundary`: these models are not driven against
the public binary. The focused storage and DB tests remain the implementation
checks. `CollectionVersionOCC.tla` separately models why a writer selected against
an old schema version must abort after a concurrent structural put. It remains a
boundary model and does not prove storage's implementation of validation.

`HeadSetCache.tla`, its configurations and `HeadSet/Cache.lean` are retired with
the adapter cache and publication lock. `HeadSet.tla` and `HeadSet/Core.lean`
remain the head-route soundness basis. The Rust/Lean conformance test now imports
Core directly and retains the sibling, pruning, abort and old-snapshot cases.
`InteractiveTxnCounter.tla` and its configurations are retired with counter finalization and `batch_gate`. The historical `MergeQueue` model is also retired with `DocWriteQueue`: its no-mutex RED describes the removed read-modify-write implementation, not engine merge operands. Exactly-once application now relies on ordinary transaction markers, and counter/document mirror dependencies remain validated. The explicit counter-versus-merge regression exercises conflict, rollback and retry convergence. Engine OCC remains a boundary; this retirement does not claim that an old mutex model proves the replacement.

`RetryRegistrationFence.tla` separates token allocation from registration commit.
It checks that a delayed registration cannot cross forget/re-add or replace a
newer pending marker. Its negative variants remove the incarnation or ordering
check. This finite model does not verify the Rust implementation.

The adapter-owned `Ssi.tla` and `SsiRange.tla` families and their eight TLC
configurations are retired. They modeled the removed `ConflictTracker`, including
its old document-scan carve-out. The real point-read write-skew regression remains
bound to regolith's pinned `DefraLevel.tla` as an engine boundary; it does not
claim that every DefraLevel read class provides serializable scans. Historical
survey material describes the retired implementation and is retained as such.
