# Survey: `crates/crdt/`

## Purpose
Field CRDTs for DefraDB.rs field/document conflict resolution, one `FieldCrdt`
implementation per type (`crates/crdt/src/field/`): the LWW register and the P/PN
counters (Int64 wrapping, Float32 at `f32`, Float64). Each answers what an operation
does, what its block carries, how its state is stored and how two states merge;
every write, merge, storage operand and replay path dispatches through it. The local
document value format (`stored.rs`) merges componentwise, field by field. CRDTs hold
no storage: the merge handler and the storage merge operator own persistence.

## State machines
- **LWW merge** (`lww.rs`): compare `(priority, value-bytes)`. Higher priority wins;
  tie -> lexicographically greater value wins (empty/tombstone loses).
  Outcomes: `Applied | RejectedLowerPriority | RejectedTieBreak`.
- **Counter merge** (`counter.rs`): unconditional addition; idempotency is delegated
  upstream to the applied key and the durable merged-CID gate (`field_ops.rs`), not local.
- **Document value merge** (`stored.rs`): apply each field part componentwise through
  its type's merge. Document status `ACTIVE(1)/DELETED(2)` rides on the composite block.

## Candidates
| name | kind | property | already-modeled | priority |
|---|---|---|---|---|
| LWW merge is a join (comm/assoc/idem) | Lean | order-independent convergence over resolved key | yes (`lwwState_merge_*`) | — |
| Counter Int64 add laws | Lean | wrapping add comm/assoc; raw merge not idempotent | yes (`word64Add_*`) | — |
| Float counter non-convergence | Lean | IEEE-754 add not associative -> divergence | yes (`float_add_not_assoc`) | — |
| Composite componentwise merge | Lean | document state merges per-field as a product join | yes (`composite_merge_*`) | — |
| Applied-set idempotency layer | Lean | durable merged-CID set supplies idempotence | yes (`appliedSet_merge_*`) | — |
| LWW tie-break key is a total order | Lean | `(u64 priority, lex value-bytes)` is a deterministic total order, so the `resolvedKey:=max` abstraction is faithful | NO (abstracted as `Rank`, not derived) | low |

## Verdict
**Model-worthy — but essentially already covered.** This crate is the canonical Rust
source for the existing Lean convergence slice; every merge law that needs a proof
beyond integration tests is already proven there. The only uncovered sliver is that the
Lean model *assumes* the LWW `(priority, value-bytes)` comparison forms a total order
(modeled as `resolvedKey : Nat` with `merge = max`) rather than deriving it from the
byte-level lexicographic comparison. That gap is low priority: total order of
`(u64, &[u8])` lex is standard and the tie-break is exercised by tests. The priority
varint codec and trait plumbing are pure IO/glue — no model needed.
