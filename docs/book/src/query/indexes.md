# Indexes

Indexes are maintained inline in the write path: every document write updates exactly
the indexes whose fields it touched. Four kinds exist
(`crates/db/src/index/`, `crates/storage/src/index/`):

## Secondary (ordered)

A sorted mapping from field value to DocID, blind puts and deletes, kept per index
(`IndexDataStoreKey` under the `d/` family). Multiple fields, arrays (multiple value
sets per document) and partial indexes (filtered) are supported. The planner uses them
for exact, range, prefix, IN and OR scans.

## Unique

The same layout with a uniqueness decision: the entry key is an Ordinary key, so a
second distinct document under the same value loses the commit and the client sees a
typed conflict. Two subtleties are deliberate:

- **Self-healing**: a local write that finds a stale unique entry whose holder no longer
  exists overwrites it (the entry was garbage, not a constraint violation).
- **Merge-path resolution**: two replicas concurrently creating the same unique value
  both persist (CRDT merges cannot roll back), and the index resolves deterministically:
  the lexicographically smallest public DocID wins the entry, both documents exist, and a
  `UniqueIndexConflict` event tells the operator. Local writes stay strict; only
  replicated concurrency resolves this way.

## Full-text

Tokenized, language-aware inverted indexes with BM25 scoring
([`crates/storage/src/index/fulltext.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/index/fulltext.rs)), synthesized from `@fulltext` schema directives
under internal names (`__fulltext__:{field}`). Corpus statistics (document counts, field
lengths) are maintained as blind STATS merges (checked i128 deltas), so concurrent
indexers commute. Query time scores with BM25 over the stored statistics.

## Vector

Four engines behind one interface (`crates/db/src/index/vector/engine/`): **flat**
(exact brute force), **HNSW** (the Malkov-Yashunin graph; deterministic level seeds;
links maintained by blind `VECTOR_LINKS` merges), **IVF** (inverted lists with product
quantization) and **SSG** (Satellite System Graph), plus **DiskANN**-style PQ-coded
resident graphs sharing code caches on the DB. Structural rules live in the merge
operator: a blind addition can never raise a node's height or add a self-link, so graph
maintenance never needs locks. Similarity queries route through the planner to the
engine ([`crates/query/src/plan/similarity.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/query/src/plan/similarity.rs)); HNSW advisory reads during maintenance are unvalidated
part reads, so building never conflicts with querying.

## Lifecycle

Indexes are created with a bulk backfill (resumable at open, `crates/db/src/index/
backfill.rs`), rebuilt on demand, and re-created when a schema change requires it (owed
reindexes settle at startup). Deletes remove entries page by page; the unique conflict
event and index reconciliation have TLA+ models (see
[Verification](../proofs/index.md)).

> **Why inline maintenance instead of background catch-up.** An index that lags its
> collection returns wrong answers to queries that trust it. Inline maintenance makes
> index state part of the same transaction as the document write: either both land or
> neither does, and a query's snapshot sees a document and its index entries together.
> The cost (index writes in the commit) is why `on_document_update_modified` updates
> only the indexes whose fields the patch intersects.
