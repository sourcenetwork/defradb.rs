# The merge operator

The merge operator is defradb.rs's main superpower on top of the engine: writes that
*commute* are expressed as merge operands, so concurrent writers of one key never
conflict and the engine folds them in any order. `DefraMergeOperator`
([`crates/storage/src/backends/regolith/merge/mod.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/merge/mod.rs)) implements regolith's
`MergeOperator` under the name `"defra-values-v1"` and serves every operand kind from one
dispatch on a kind byte.

## Operand kinds

Every operand starts with a magic (`DM\x01`) and a kind byte:

| Kind | Meaning | Used by |
|---|---|---|
| `INT64`, `FLOAT32`, `FLOAT64` | counter increments | counter fields |
| `STATS` | `(documents, field_length)` i128 statistics deltas | full-text index shards |
| `DOCUMENT` | a document part operand: field values by stable field ID | document writes |
| `VECTOR_LINKS` | HNSW adjacency additions | vector indexes |
| `VECTOR_COUNTS` | live/waste count adjustments | vector index rebuilds |
| `HEAD_SET` | a head-set state function | DAG head bookkeeping |

## full_merge: applying operands to a base

`full_merge` decodes the base value, applies each operand in order, and re-encodes:

- **Counters**: the base is big-endian and zero-initialized when absent. `INT64` uses
  wrapping addition, so integer counters converge in any order. Floats add plainly, and
  the counter chapter of the CRDT docs is explicit that float addition is not convergent
  in general; the engine keeps it deterministic by order.
- **STATS**: two big-endian i128 halves, added with overflow checks; an overflow fails
  the merge rather than wrapping a statistic.
- **DOCUMENT**: the base is a stored document ([`crates/document/src/stored.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/document/src/stored.rs), fields
  keyed by stable u32 field ID, each with a kind and a priority). Each operand is applied
  field by field: LWW fields take the higher-priority value (empty bytes with a priority
  is a tombstone, and the priority survives), counter fields add. The stored format's
  magic is `DFV\x01`.
- **HEAD_SET** and the vector kinds dispatch to their own modules, below.

## partial_merge: folding operands without the base

The engine can fold operands *before* seeing the base value (they may arrive in a batch).
`partial_merge` is only allowed where folding is exactly equal to applying in sequence:

- `HEAD_SET` operands fold by function composition (below).
- `VECTOR_LINKS` fold by concatenation of the 12-byte addition records.
- A pair of `DOCUMENT` operands folds only when every field is LWW or an integer counter:
  a join of LWW fields is associative, and wrapping integer sums are exact in any
  grouping. Anything else returns "cannot fold", and the engine keeps the operands.
- A pair of `INT64` operands folds by wrapping addition.

Floats and the checked i128 statistics deliberately never fold: folding a pair could
change rounding or hide an overflow that a later base would have surfaced
([`crates/storage/src/backends/regolith/merge/mod.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/merge/mod.rs) documents the reasoning beside the code).

## touches: which parts an operand changed

`touches(key, operand, parts)` answers: does this operand write any of these part IDs?
Projected reads are validated against exactly the operands whose `touches` returns true:

- a `DOCUMENT` operand touches iff one of its field IDs is in `parts`;
- a `HEAD_SET` operand touches iff one of its names hashes to a listed part;
- a vector operand touches iff it carries the links part;
- anything malformed or kind-less touches *true*: the conservative answer, because an
  operand that cannot be understood must be assumed to change everything.

The operator's contract is checked beside it with the engine's own property harnesses
(`check_merge_operator`, `check_touches`) plus per-kind tests in
`crates/storage/src/backends/regolith/merge/tests/`.

## Head sets: state functions that compose

A head set is one value holding the state of many named heads, each name in one of three
states: `Absent`, `Ahead` (a parent seen before its child arrives) or `Live(priority)`.
Writes are per-name state functions ([`crates/storage/src/backends/regolith/merge/head_set.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/merge/head_set.rs)):

| Operand | absent | ahead | live(p) |
|---|---|---|---|
| `add(p)` | live(p) | absent (consumed) | live (kept) |
| `supersede` | absent | ahead | absent |
| `supersede_ahead` | ahead | ahead | absent |

Each operand stores, per name, the images of the three states (a function on three
points). Composition of these functions is associative, which is exactly why
`partial_merge` may fold any grouping: folding by function composition equals applying in
sequence. Names are stored sorted, so folds are one walk of two sorted lists, and the
encoding is canonical.

Each name is one *part*: `part(name) = NAME_PART | rapidhash(name)`. A publication that
decides on some heads (their names) is refused only by operands that touch those names,
so a document publishing a new field head does not conflict with a concurrent publication
of a different field.

> **Why state functions instead of read-modify-write.** The old layout kept one key per
> head and read before writing, which made every publication a race on a growing set of
> keys. Encoding the *transition* as a value (a function that says what to do in every
> state it may find) turns the whole update into one blind, order-independent merge. The
> "ahead" state exists so a child block arriving before its parent records the gap and
> the parent's later arrival consumes it, exactly once.

## Vector index records

HNSW graph maintenance uses the same trick at the node level
([`crates/storage/src/backends/regolith/merge/vector.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/merge/vector.rs)): `links(layer, neighbour)` is a 12-byte addition record; `full`
appends the neighbour to that layer's sorted, deduped list. Two structural rules are
enforced in the operator: a blind addition never raises a node's height (height is
structural, chosen at insert), and self-links are refused. `counts(live, waste)` adjusts
approximate rebuild counters with saturating arithmetic so advisory statistics never
wrap. Reads of these records are advisory (`get_parts(key, &[])` or a single part), so
graph maintenance never conflicts with queries.

> **Why the operator owns structure rules.** If blind merges could corrupt the graph
> (raise a height, add a self-link), commutativity would be worthless: concurrent
> maintenance would need locks. Pushing the invariants into the operator makes the blind
> path the *safe* path, and the invariants are proven in Lean beside it.
