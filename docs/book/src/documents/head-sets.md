# Head sets

A document's *heads* are the frontier of its DAG: the composite and field block CIDs
that no later block supersedes. Merge needs them (a replicated write applies against the
heads it finds), history queries need them (walk from the heads), publication needs them
(a new block supersedes its parents). defradb.rs tracks heads in a **head set**: one
value under one key per document, updated by blind merges.

## The layout

Document head sets live under `h/hd/`, collection head sets under `h/hc/`
([`crates/db/src/block/heads.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/block/heads.rs)). A set value holds, per name, one of three states:
`Absent`, `Ahead` (a child arrived before its parent), or `Live(priority)`. Names are
`{field}/{cid}` pairs, so one set holds every field's heads plus the composite `C`
heads.

## The operands

A publication merges one operand that says, per name, what to do in every state it may
find (the table is in the [merge operator chapter](../storage/merge-operator.md)):
`add(p)` records a new head (and consumes an `Ahead` marker waiting for it),
`supersede` removes a parent the write consumed, `supersede_ahead` records a parent that
has not arrived yet. Writers never read the set to decide: one operand per set per
transaction, merged blind, so concurrent publications never conflict.

## Why each name is a part

Each name's rapidhash is a part ID. A publication that read the heads it supersedes
(deciding, say, "these are the parents of my new block") did a projected read of those
names' parts, so it conflicts only with operands that touch *those* names: a concurrent
publication of a different field does not disturb it.

## The priority index

Beside the set, each publication writes an entry under `h/p/{doc}/{priority}/{cid}`: the
commit priority index that history queries use to walk a document's DAG by height
without scanning blocks. Entries are ordinary keys; pruning history deletes them (see
[Memory reclamation](../memory/index.md)).

## Collection head sets

Branchable collections keep a collection-level head set (`h/hc/`), blind-merged the same
way, so a collection's published block set is itself a CRDT: replicas converge on the
same set of collection blocks without coordination.

> **Why sets instead of per-head keys.** The pre-[regolith](https://sourcenetwork.github.io/regolith/) layout kept one key per head
> and read before writing: every publication was a read-modify-write over a growing key
> range, serialized by a lock, and a replicated parent arriving late was a special case.
> The set encoding makes the whole update one commutative merge, makes "parent not here
> yet" a first-class state (`Ahead`) instead of an error, and lets two writers publish
> concurrently because their operands compose. It is the same trick as counter
> increments, lifted from numbers to a small state machine.
