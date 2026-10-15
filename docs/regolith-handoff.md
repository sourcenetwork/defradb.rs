# Regolith adoption handoff

This is the review entry point for Theo (vertexclique). [PR #1953](https://github.com/sourcenetwork/defradb.rs/pull/1953) is the permanent handoff link: its description will point to the latest stack tip even when later steps add PRs above it. It follows §5.2 of the supplied `regolith_defralevel_docs` bundle. **The stack is incomplete and remains draft.** This file is updated as implementation and verification progress; an implemented item is not a claim that its PR is ready to merge.

Target: new installations. Jack explicitly waived compatibility with existing encrypted/value layouts and migrations. Regolith is pinned to `e844baefe838f3af76403afd46c4b42f1a0b98a5`. Worker-pool/runtime redesign (§5.2 step 9) is outside these eight adoption steps. No PR has been merged.

## Stack and merge order

Merge from the bottom upward, retargeting the next PR after each merge. The branch name on #1920 predates its replacement implementation; its current content is engine encryption, not the old marker proposal.

| Order | Plan | PR | Branch | Base |
|---|---|---|---|---|
| 1 | Prep / step 0 | [#1922](https://github.com/sourcenetwork/defradb.rs/pull/1922) | `fix/step0-kms-writes` | `main` |
| 2 | 1a API bump, still RepeatableRead | [#1949](https://github.com/sourcenetwork/defradb.rs/pull/1949) | `chore/regolith-defralevel-api` | `fix/step0-kms-writes` |
| 3 | 1b engine encryption | [#1920](https://github.com/sourcenetwork/defradb.rs/pull/1920) | `defralevel/s0-change-markers` | `chore/regolith-defralevel-api` |
| 4 | 1c classifier / DefraLevel | [#1951](https://github.com/sourcenetwork/defradb.rs/pull/1951) | `defralevel/01c-key-classifier` | `defralevel/s0-change-markers` |
| 5 | 2 transactional decisions | [#1952](https://github.com/sourcenetwork/defradb.rs/pull/1952) | `defralevel/02-transaction-decisions` | `defralevel/01c-key-classifier` |
| 6 | 3 mechanisms, in progress | [#1953](https://github.com/sourcenetwork/defradb.rs/pull/1953) | `defralevel/03-engine-mechanisms` | `defralevel/02-transaction-decisions` |

The intended total is 11 PRs: these six plus one each for steps 4–8. Split into two stacks if further necessary splits exceed 11. Do not merge superseded generated implementations (#1897/#1899/#1911); their closure remains pending completion of this adoption.

## Implementation status

| Step | Status and remaining work |
|---|---|
| 1 | Implemented in three separate PRs. Engine key providers replace per-value encryption, including read-only opens. Only genuine block CID keys are ContentAddressed. Writers use DefraLevel; the old head cache is gated off for them, pending deletion in step 6. |
| 2 | Implemented broadly: transactional document/version/definition/deletion reads, local ACP authorization and owner registration, validated relationship scans, embedded NAC persistence, downsample series uniqueness. Audit remaining writer copies during retry conversion. Remote Vera stays at the network boundary. |
| 3 | Implemented: commit-ordered arrival append, engine ID allocation, numeric and full-text merges, local document part IDs and field operands, version-key handling, removal of interactive counter finalization, write-free readonly lenses and conservative lens-aware index planning, document head markers and CommutativePrefix classification. HNSW link/count operators are implemented with passing concurrency/operator/recall tests. Remote document composites now use LWW and exact integer operands; floating-point mirror repairs retain validated puts to preserve rounding. Known modified-field writes now project mutation and touched-index dependencies; general query/output reads and counter reads remain conservative. Writable lens retries belong to step 5. |
| 4 | In progress on an isolated branch: propagate actual receipts and queue local events before commit; model delayed callbacks and failed commits. Not yet published. |
| 5 | In progress: engine-owned retries and prepared remote inputs. The bundle is inconsistent: §5.2 asks for synchronous traits, but later accepted D39 adds `transact_async` and D56 explicitly says the async datapath stays. Follow D56 and the pinned engine API; worker-pool redesign remains step 9. |
| 6 | Local branch removes the head-cache publication lock and obsolete head-cache/forced-read APIs; native storage and DB block/merge tests pass. Performance gates remain pending; this is not yet published. Remaining listed locks must be removed individually with before/after gates. Counter finalization is already gone; batch/document guards remain. The local candidate also removes has_for_update from the trait and wrappers. A separate arrival-guard removal candidate awaits validation. |
| 7 | Implemented on an isolated branch: engine commit/policy tickers and per-store on_conflict reason counters, preserving caller instrumentation. Two native regressions and native/WASM Clippy pass. Not yet published above steps 4–6. |
| 8 | Partial local candidate: retired head-cache model/configs and Lean cache module, retained HeadSet, and bound arrival proofs to pinned CommitOrderedAppend. Lean build and direct conformance sample pass; append GREEN and counter RED pass. Interactive counter model retirement awaits batch-gate removal. Not yet published. |

### Step 3 decision-read audit (2026-10-14)

The remaining document conflict is not fixed by narrowing only the old-document read in
`Collection::update_with_indexes_parts`. The local autocommit path first reloads the document through
`LensedDocFetcher::get_by_ids` under its write guard; that calls `get_by_doc_id` and validates all fields in
`read_document_blob_and_version`. Ordinary query reads use the same full-document path. Both can still reject a
transaction after a concurrent disjoint field operand changes the blob.

A sound write projection must include every field the mutation actually consumes: changed fields, changed fields
participating in indexes, changed immutable fields, the downsample `source_doc_id` when it changes, and any embedding
source fields used to regenerate a vector. Index maintenance must skip indexes untouched by the mutation; otherwise
index maintenance unnecessarily depends on unchanged fields. Projected reads return the complete value bytes; only
the validation dependency set is narrowed. Structural puts,
schema-version transitions, migrations, and callers without an explicit modified-field set retain whole-value reads.
Query output reads require the selected fields plus predicate, ordering, join, and computed-field dependencies, so a
field dependency set must flow from query planning into the fetcher before query reads can be narrowed safely.

Counter reads remain validated. `ValueReader::value` is used to materialize counter values, and the document mirror
repair path derives an operand from the stored mirror. An advisory counter read could therefore commit a stale
document mirror and corrupt index/materialized query results even though the authoritative counter accumulator
itself merges correctly.

The implementation now narrows the explicit-modified-field collection update path and the autocommit mutation reload:
the reload requests changed fields, all fields of touched composite indexes, and all source fields used by embedding
regeneration. The old-document index path uses the same dependency set, skips untouched indexes, and only performs
series-key release/claim when `source_doc_id` changes. Conditional (`expected`) updates, general query/output reads,
unknown-modification callers, and lens migrations stay conservative. Counter reads stay validated. Validation passed:
`cargo check -p db`; after review corrections, all 58 write and all 58 index tests pass on the integrated stack, including the explicit
disjoint and same-indexed-field transaction regressions. This does not claim generic query projection or zero
incidental conflicts for explicit transactions that already performed a full document read. A stale composite-index
regression was verified RED with the dependency repair disabled (missing expected index row), then GREEN after
restoring it: untouched index components come from the transaction snapshot, not the caller’s stale document.

## Latest implementation checkpoints

- `c8146173`: document value integration. Local schema field CIDs remain unchanged; separately allocated immutable part IDs drive projected validation. Stored document format is `DFV\x01`; block wire CBOR stays canonical. Remote tombstones preserve winning LWW priority/raw bytes. A standalone counter block followed by a local increment repairs a stale blob with a validated put. Field heads keep field priorities rather than inheriting composite priorities.
- `9e146e42`: document head markers. `/d/` and `/ds/`, and collection `/c/` and `/cs/`, are exact commutative families. Pruning removes parent+markers atomically in a separate transaction. Ordinary `/r/` publication guards survive pruning: they prevent replay from resurrecting reclaimed heads and are separate from counter application markers. Truncation removes publication guards too. Late-parent and old-snapshot cases have regression tests.

## Validation and review limits

Latest step-3 CI repairs: FFI and public embedded stores now forward allocation and transaction statistics; `Store::allocate` is mandatory so wrappers cannot silently omit it. All 170 native minimal-feature FFI tests and the embedded suite pass. The vector-link proof family is registered in the coverage matrix; all five conformance library tests pass. Formatting passes.

The stack has been rebased onto main `532aac3f`. Its newly added DiskANN disk-cost test used the private Regolith `Options.statistics` field; step 1a now uses the public builder. This addresses the current tip CI lint/optional-feature compile failures. Fresh CI and validation of the combined main/stack changes are pending; earlier local test results predate this rebase.


Local checks passed at the document/head checkpoint and subsequent HNSW integration:

- Database block, merge, write and read integration suites; the read suite has one pre-existing ignored test. Earlier collection/definition/query suites passed during document and readonly-lens integration.
- New document-head tests: overlapping writers both commit; prune preserves live heads and old snapshots; a marker received before its parent survives; replay before/after prune cannot resurrect a parent.
- Remote tombstone and independent field/composite priority tests. The stale counter mirror regression was also run with its repair disabled and failed with 13 instead of 18, then passed with the repair restored.
- Native workspace all-targets Clippy and `defra-wasm` Clippy, warnings denied.
- HeadSet TLC GREEN passed (12 distinct states); the eager-delete RED failed `INV_NoWriteConflict`, as expected. Lean was not rerun at this checkpoint.
- HNSW: 227 vector tests passed, 12 measurements remain ignored; concurrent inserts preserve both shared back-links and metadata counts, stale structural pruning conflicts; encrypted reopen and put terminators pass. VectorLinks TLC GREEN passes; stale-put and unvalidated-prune REDs fail as expected. Native/WASM Clippy pass.
- Earlier storage operator contract/property tests cover malformed operands, integer exact folding, ordered float/stat operands, document touches/folding, encrypted reopen, and concurrent full-text updates.

**CI is not all green.** At the 2026-10-14 checkpoint, lower PRs have failing surface-test jobs; #1949 also has a failing build job and #1952 has signed-docs/Go-compat failures. Investigation found missing searchable-encryption artifact propagation in the step-2 batch path; the fix passes both affected Rust external-node tests locally. The surface-test accounting failure was traced to internal short-ID reservation retries omitted from the measurement; `e91ed833` adds their counters and benchmark accounting to step 0 and is carried through the published stack. All four benchmark verification tests pass locally. Go interoperability and fresh CI still need verification. Many macOS jobs and latest-tip checks are queued/running. Consult each PR's current checks rather than treating this snapshot as current CI status.

Local external-node encryption regressions `rust_rust_se_cross_node` and `merged_and_self_written_docs_both_hit_on_same_name` now pass after restoring searchable-encryption artifact propagation in the batch path. The fix is carried in step 2 and above. Go interoperability remains unverified locally. The remote document operand checkpoint passes all 216 merge tests and 55 write tests, plus DB all-targets Clippy with warnings denied. The full pre-head-cache-removal baseline has run (1/4/16/64 writers, 256 operations per writer, three repetitions, Immediate on-disk durability); the matching after run is pending. Creates have zero conflicts at all counts; shared/mixed workloads still exhaust requests at 64 writers. Ambient host load is recorded, so this is not an isolated-machine measurement. No step-6 performance gate is yet satisfied. HNSW clustered-corpus recall@10 (N=2000, ef=64) is 0.9967 at d=16 and 0.9767 at d=128; these are corpus-specific gates, not a real-embedding workload claim. No zero-incidental-conflict or throughput improvement claim is made. Do not remove guards merely because the functional suites pass.

## Review focus / next work

1. Finish step 3's remaining decision-read audit. HNSW and remote document operands are implemented; retain validated pruning and structural replacements.
2. Narrow reads only where decision/index dependencies are known. In particular, do not make counter reads advisory while the document/index mirror still depends on them.
3. Investigate lower-stack CI failures and apply fixes at the earliest affected PR, carrying them upward without dropping newer work.
4. Implement steps 4–8 in order, updating this handoff and the top PR link after each checkpoint.
5. Verify all stack bases, final CI, external integration, proof gates and required benchmarks before marking any incomplete PR ready.
