# History & Time Travel

Because every write is a block in a DAG, history is queryable first-class.

## `_commits`

The `_commits` system collection exposes the block DAG
([`crates/query/src/runner/commits.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/query/src/runner/commits.rs), `crates/db/src/read/commits/`):

- by **CID**: one block, or a traversal to a `depth` following block heads;
- from **heads**: the frontier of a document (or collection), depth-limited;
- by **height range**: the commit priority index (`h/p/`) turns "commits of this
  document between these priorities" into an index scan, no DAG walk.

Blocks are decoded into commit documents (field deltas, links, priorities) and returned
in the Go-compatible order. Every result row is ACP-checked: a commit reveals a
document's data, so commit queries are filtered exactly like document queries (this is
the "gate commit queries with DAC" behaviour, ported from Go).

## Time travel (versioned reads)

A versioned read reconstructs a document *as of* a target CID
([`crates/db/src/read/versioned.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/read/versioned.rs)): walk the DAG from the target back to genesis,
replay the CRDT deltas forward, and return the state. Encrypted fields resolve their
DEKs through the KMS during the replay (prepared inputs, never a remote call per field).
Versioned reads are also how "the document at this commit" is served to clients that
pin versions, and how collection-level CIDs work for branchable collections (the
collection block resolves to a set of document composites).

Deleted documents remain visible through indexed historical queries until retention
prunes the history: deletion is a block, not a shredding, and the queries honour that.

## Downsampling and retention (the consumer side)

Downsample plans periodically roll source documents into downsampled targets
(summaries per window); retention policies then prune source history
([Memory reclamation](../memory/index.md) covers the write side). Versioned reads are
what makes "prune old source history" safe: the downsample target holds what queries
need, the source's old heights are gone, and history queries that target a pruned
height fail cleanly rather than lying.

> **Why replay instead of snapshots-per-version.** Storing every version materialized
> multiplies write amplification by the number of queried versions. The DAG already
> holds each delta exactly once; replay is a read-side cost paid only by history
> queries, bounded by the walk, and the priority index keeps the common cases (head,
> range of heights) off the walk entirely.
