# CRDT types

Conflict-free replicated data types are the reason replication needs no coordinator:
nodes that have received the *same set* of deltas converge to the same state whatever
order they applied them in, and that convergence is a theorem, not a hope. (Nodes with
different subsets are simply behind; floating-point counters are the documented
exception and are covered below.) defradb.rs uses four.

## LWW register

The workhorse. A field value with a priority: given two writes, the higher priority
wins; equal priorities break ties deterministically on the value bytes
([`crates/crdt/src/lww.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/crdt/src/lww.rs)). Empty data is a tombstone whose priority still counts, so a
delete beats a lower-priority write and loses to a higher one. Every ordinary field is
an LWW register by default.

> **Why the byte tie-break.** Two replicas resolving the same two equal-priority writes
> must pick the same winner without communicating. A total order on (priority, bytes)
> gives them one, and the convergence proofs lean on it.

## Counter

A single accumulated value with blind-merge increments
([`crates/crdt/src/counter.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/crdt/src/counter.rs)). This is deliberately *not* a classic PN-counter with two
registers: an increment is a document-part operand (kind `INT64`/`FLOAT32`/`FLOAT64`),
and i64 addition is wrapping and order-independent (proven in Lean), so any grouping of
increments folds to the same sum. Idempotence of replicated application is enforced one
level up: the applied key of a remote block means the same increment block is never
applied twice. A nonce keeps each increment block's CID unique.

Floats are documented as non-convergent under addition reordering (IEEE addition is not
associative); the engine keeps application deterministic by sequence order, and the
honest caveat is written beside the code.

## Composite

The document-level delta: which field blocks this write produced, the document status
(active/deleted), and its priority. The composite is what links a write into the DAG;
its priority is the document max plus one.

## Head sets (the DAG CRDT)

Which heads a document (or collection) currently has, as one value under one key,
updated by blind merges of set-operand state functions. The mechanism gets its
[own chapter](head-sets.md) because it is also the cleanest example of the state-function
pattern.

## The algebra, and where it is proven

The merge laws these types must satisfy (commutativity, associativity, idempotence,
order-independence) are proven in Lean in `proofs/lean/` (CRDT merge laws, convergence
family), and the temporal half (every node eventually receives every delta under
eventual connectivity) is a TLA+ model. Convergence is the *product* of the two: neither
half alone is the theorem. See [Verification](../proofs/index.md).

> **Why CRDTs instead of operational transforms or last-write-wins timestamps.** Wall
> clocks do not order events across nodes, and OT needs a serialization point. CRDT
> merges are associative, commutative and idempotent, which means the transport can
> redeliver, reorder and partially deliver, and the state still converges. The costs are
> explicit: tombstones must survive long enough (priorities carry them), and metadata
> (heads, priorities) has to be maintained, which is what head sets and the priority
> index do.
