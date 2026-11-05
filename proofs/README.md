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

### Modeled families
| Family | Tool | Crates it covers |
|---|---|---|
| Explicit embedded signing | TLA+ / Lean | defra-node, query, db |
| Verified client graph transaction | TLA+ / Lean | defra-node, query, db |
| HTTP client-signed create | TLA+ / Lean | http, query, db |
| HTTP client-signed update | TLA+ / Lean | http, query, db |
| HTTP client-signed conditional update | Lean | http, query |
| SSE loss and error delivery | TLA+ / Lean | events, http, query |
| Storage failure classification | TLA+ / Lean | storage, db, query, http |
| B3 filtered replication | TLA+ | p2p, db-merge |
| Explicit transactions in the request context | TLA+ / Lean | client, db, query, http, ffi, wasm, defra-node |
| Transactional document relationships | TLA+ / Lean | acp, query, http, defra-node |
| Transactional node source grants | TLA+ / Lean | acp, query, http, defra-node |
| Schema-evolution operations in the core | TLA+ / Lean | client, db, lens, query, http, cli, ffi, defra-node |
| DAG convergence (partition / eviction / restart) | TLA+ | p2p, db-merge, blockstore |
| CRDT merge laws | Lean | crdt, defra-core |
| Replicator lifecycle (no-loss / resume) | TLA+ | p2p, db-merge, embedded |
| Sync ownership transfer (head hint / receiver pull) | TLA+ | p2p, db-merge, storage, embedded, defra-node |
| Multi-instance claim | TLA+ | gents |
| Block integrity / signatures | TLA+ | db-merge, defra-core, blockstore |
| KMS key distribution | TLA+ / Lean | kms, db, db-merge |
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
| Query fragment nesting bound | Lean | query |
| Index-maintenance consistency | Lean + TLA+ | db-index, db-merge |
| Concurrent collection-head transitions | TLA+ & Lean | db-block-builder, storage |
| Commit-ordered document arrivals | TLA+ | db, storage (pinned regolith) |
| DiskANN pending cache | TLA+ & Lean | db-index (vector), storage |
| DiskANN shared quantizer | TLA+ & Lean | db-index (vector), db-write |
| RobustPrune blocked covered checks | Lean | db-index (vector), defra-core |
| DiskANN back-edges in place | Lean | db-index (vector) |
| Retry input and marker fencing | TLA+ & Lean | db, query, storage, p2p, p2p-adapter |
| Collection schema-version OCC | TLA+ | db |
| Schema publication completion | TLA+ / Lean | storage, db |
| Peer connection observation | TLA+ / Lean | p2p |
| HTTP router composition | TLA+ / Lean | http, defra-node |
| Explicit transaction statement atomicity | TLA+ | query, db, storage |
| Engine ticker export | TLA+ & Lean | telemetry, storage |
| Worker pool CPU ledger | TLA+ & Lean | workers |
| Worker default pool | TLA+ & Lean | workers |
| Node shutdown order | TLA+ / Lean | defra-node, embedded, edge, acp, http, query, db, storage |
| Inbound stream admission | TLA+, Lean | p2p |
| Vector index link maintenance | TLA+ | storage, db-index |
| Peer fetch stats | Lean | p2p |
| DAG fetch provider ranking | TLA+, Lean | p2p |
| DAG fetch arbitration (EWMA) | Lean | p2p |
| Gossip connection ownership | TLA+, Lean | p2p |
| Iroh service records | TLA+, Lean | p2p |
| Iroh solicited replies | TLA+ | p2p |
| Reply-driven DAG fetch | TLA+ | p2p |
| Depth-cut DAG fetch | TLA+, Lean | p2p |
| Index drop during backfill | TLA+ | db-index |
| Purge by DocID | TLA+ | db |
| Replication filter | TLA+ | db-merge |
| Node startup | TLA+, Lean | defra-node, storage |
| Text CRDT convergence | TLA+ & Lean | crdt, db-merge |
| JSON CRDT per-path merge | TLA+ & Lean | crdt, db-merge (specified before the code, #2129) |
| Custom CRDT types | TLA+ & Lean | crdt, schema, db-merge, p2p (#2183) |
| Document write pipeline | TLA+ | db, db-block-builder |
| Postgres wire connection loop | TLA+ | pg-compat |
| HTTP client redirects and body deadline | Lean | net |
| Connect fallback race | TLA+ | net |
| Coordinator shutdown drain | TLA+ | p2p, edge |
| Edge completion | TLA+ | edge |
| Edge gate continuation | TLA+ | edge, net (#2546) |
| Iroh dial outcome | TLA+ | p2p, p2p-adapter |
| Server drain | TLA+ | net, pg-compat, edge |
| Document topic ingress | TLA+ | p2p, p2p-adapter (#2545) |
| Add collection | TLA+ | db |
| Collection truncate | TLA+ | db |
| Add DAC policy | TLA+ | db, acp, vera, http, cli, ffi, defra-node |
| Node access control operations | TLA+ | db, acp, http, cli, ffi, defra-node |
| Filter relation paths and scalar leaves | Lean | query (#2427, #2430) |
| Orphan block sweep | TLA+ & Lean | db, storage |
| Per-peer stream cap | TLA+ | p2p |
| Receive gate | TLA+ | p2p, db |
| P2P delivery liveness | TLA+ | p2p, p2p-adapter, db-merge, storage |

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
artifact." The modeled families are the registry's families, so adding a model
is one `PROPERTIES` entry and one row above;
`matrix::readme_lists_exactly_the_modeled_families` names any family that has
one without the other. `anchor::every_registry_anchor_resolves` resolves each
anchor's files and symbols against the tree, so a rename that moves modeled code
fails until its anchor follows it. A `Contract` or `Behavioral` row names the
tests that bind it in `bound_by`, and `matrix::every_bound_tier_names_its_tests`
fails when one names none or names a test that is not declared.

[`src/constants.rs`](src/constants.rs) states once each P2P constant the models
depend on: the Rust constant that sets it, and the model constants that restate
it exactly or check a scaled-down instance of it.
`constants::every_constant_agrees` fails when the code, a restating model or the
table disagree.

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
| DAG convergence | Boundary | `replication.rs` (live-forward under a configured replicator) + `partition.rs` (converges after an explicit replicator re-set; concurrent same-doc merge — the bug it found+fixed). Head rediscovery is embedder-owned |
| Replicator lifecycle | Behavioral | `replicator_lifecycle.rs` (backfill no-loss + resume across node restart) |
| Sync ownership transfer | Boundary + external integration | deterministic full-DAG/head-hint A/B fence; `p2p_admission_restart`; `p2p_backlog` (outbound push backlog with a SIGSTOPped replicator); marker migration/storage tests; mixed Go/Rust PushLog fixture. These run in repository CI, not this conformance binary's `tests/behavioral/` harness. |
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
| P2P delivery liveness | Behavioral (trace) + Boundary | `model_trace.rs` replays recorded runs through `Trace_P2PDelivery.tla`; eventual connectivity, the bounded instance and the restart scenarios stay Boundary |

```bash
proofs/verify-all.sh                 # TLA + Lean + conformance (behavioral if binary present)
cargo build -p cli                   # produce fresh target/debug/defra
cargo test -p conformance            # Lean axis + behavioral; DEFRA_CONFORMANCE_BINARY overrides target
                                     # iroh families: DEFRA_CONFORMANCE_IROH_BINARY, else built with --features iroh
```

### Trace validation (P2P delivery)

A `defra` built with `--features model-trace` logs one JSON event per
[`P2PDelivery`](tla/P2PDelivery.tla) action under the `model_trace` log target
(`crates/p2p/src/model_trace.rs`; the event table and its code sites are in
[`P2PDelivery_DESIGN.md`](tla/P2PDelivery_DESIGN.md)). Without the feature the
`model_trace!` sites compile to nothing. [`src/model_trace.rs`] reads the
events from each node's log, orders them by time, names the nodes and the heads
of one document, and writes a TLA+ module and TLC configuration that
[`Trace_P2PDelivery.tla`](tla/Trace_P2PDelivery.tla) replays: each event must
be a model step from the state the earlier events left, under the fixed
design's knobs, with the model's invariants holding and the run settled at the
end. A deadlock names the first event the model forbids.

```bash
cargo build -p cli --features iroh,model-trace && cp target/debug/defra target/defra-trace
DEFRA_TRACE_BINARY=$PWD/target/defra-trace cargo test -p conformance --test tla_conformance -- --ignored model_trace::
```

Each run leaves `events.jsonl`, the module, the configuration and `tlc.out` in
`target/model-trace/<Scenario>/`. A replay is accepted only when TLC starts
from one state and walks past every event, so a trace whose start the model
rules out cannot pass by checking nothing. The recorded runs
`MC_P2PDelivery_Trace_TwoNode` and `MC_P2PDelivery_Trace_HubRelay` and eleven
RED configurations of planted mutants (`MC_P2PDelivery_Trace_Mutants.tla`, one
per replay guard) also run in `tla/run-all.sh`, so the replay is checked
without a binary. Their rows say where the replay must end: `TRACE:<n>` walks
`n` states (events + 1) with no error, `DEADLOCK:<l>` stops at event `l`.

To add a trace for a new scenario:

1. Write a test in `tests/behavioral/model_trace.rs` that builds its cluster
   with `cluster(n)`, connects the nodes and sets every replicator before the
   first write (the model's edges exist from the start), writes one document
   and waits until every node the edges lead to has each write.
2. Call `replay(&cluster, names, edges, &doc, "<Scenario>", story)` with the
   nodes' model names in cluster order, the edges by name and the document to
   follow. It asserts that TLC accepts the run.
3. Assert the steps you meant to exercise are in the trace (`Trace::count`,
   `Trace::steps`), so an empty run cannot pass.
4. To pin the run, copy the generated `MC_P2PDelivery_Trace_<Scenario>.{tla,cfg}`
   into `tla/`, add a `TRACE:<events + 1>` row to `run-all.sh`, and plant RED
   mutants of it in a mutants module, each with a `DEADLOCK:<index>` row.
5. A code path that takes a model step nobody logs yet gets a
   `p2p::model_trace!(Variant { .. })` at its site, a case in
   `Trace_P2PDelivery.tla`'s `Replay`, and, for a new field, a line in
   `model_trace::step`.

A trace follows one document, and every head in it must be written during the
run. Pull sync (`p2p_collection_add`) runs advisory fetches the model counts only
from one provider, so scenarios replicate through replicators alone. A scenario
that drives a behaviour main still has (a RED of `P2PDelivery`) fails the replay
under the fixed design's knobs; setting that one knob to main's value in the
generated configuration confirms it is the known defect.

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
  reachable holder of the block); **eventual connectivity**, with the heads owed by a
  retry marker or a durable pending record, for the convergence liveness results (head
  rediscovery is the embedder's); weak fairness on protocol actions; an **eventually
  serving provider** (strong fairness on some completed fetch) for the SyncOwnership
  dispatch and memory liveness results, whose `Dead` and `Trickle` providers get no
  merge promise. Those results give fairness to the scheduler only, never to a root; the
  memory results give it to each sending peer's retry ledger and to the receiver's answers
  to that peer, never to a scope, and promise nothing to a peer whose durable quota a
  never-serving root can hold.
  The PushBacklog liveness result assumes every running push send ends, by a reply or
  its send timeout; its dispatch fairness is the scheduler's only, never a peer's.
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
compare-delete and the peer-incarnation fence on clear across forget/re-add,
and the sweep over several scopes: a tick reads one probe and at most one page
after a durable cursor, and a purged document's marker is discharged with only
the tick, the pass and the clock fair. Its mutants read every marker before the
due check, keep the vanished marker, or never move the cursor.
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
check. It also models the fan-out caller that registers a committed head with
each replicator. `INV_FanOutPeerLocal` requires that a gone, re-incarnated,
covered or failed peer is decided on its own, and that every registered peer
is queued. `Red_FanOutAbort` aborts the fan-out on the first rejection. This
finite model does not verify the Rust implementation.

`RetrySchedule.tla` models the one retry clock a sender keeps per peer. An
acknowledged delivery, live, fenced by a newer marker generation, or replayed,
returns it to the first rung, never later than it was, so a push that fails
after an acknowledgement, or a timeout after a pass the cap cut short, is not
charged the rung an outage earned; a new marker and a reconnect keep the rung,
so a peer that keeps failing still climbs the ladder; and the sweep reads a
deadline past the longest rung, which only a backward step of the wall clock
leaves, as due. Its negative variants keep the escalated clock after any
delivery, a fenced one or a replayed one, restart it on a new marker or a
reconnect, or honor the stepped-back deadline.
`RetrySchedule.Delivery` proves the restart's arithmetic in Lean. Time is
abstract and which marker a bounded pass reaches first is out of scope.

The adapter-owned `Ssi.tla` and `SsiRange.tla` families and their eight TLC
configurations are retired. They modeled the removed `ConflictTracker`, including
its old document-scan carve-out. The real point-read write-skew regression remains
bound to regolith's pinned `DefraLevel.tla` as an engine boundary; it does not
claim that every DefraLevel read class provides serializable scans. Historical
survey material describes the retired implementation and is retained as such.
