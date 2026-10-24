# The document model

## The DocID

A document's identity is its **DocID**: a UUIDv5 over a fixed namespace of the genesis
composite block's CID ([`crates/document/src/doc_id.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/document/src/doc_id.rs)). The string form is
`{base32 version}-{uuid}` (for example `bae-...`). The DocID is stable across replicas:
any node that has the same genesis block computes the same DocID, which is what makes
replication, dedup and the once-keys of the arrival journal possible.

The CID itself is not recoverable from the DocID string; it lives in the DAG.

## Short IDs

Locally, every document also gets a compact **short ID**: a u64 allocated from
`/seq/doc` in ranges of 1024 by a lock-free CAS allocator ([`crates/db/src/docid/map.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/docid/map.rs)).
Short IDs key the datastore layout (`doc_key(doc_short_id)`), which makes iteration
order match allocation order and keeps keys small. The systemstore holds the bidirectional
mappings (short ID to DocID and back), plus the DocRef, and the block-CID-to-DocID
ownership map that memory reclamation walks.

Allocation is `Db::allocate`: engine-side, atomic, and safe to call inside a retried
closure *because* a reserved range survives attempts (an attempt never allocates twice;
the prepared-inputs rule from the doctrine chapter).

## The stored format: documents as parts

The materialized value of a document is a `StoredDocument` ([`crates/document/src/stored.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/document/src/stored.rs)):
magic `DFV\x01`, then fields keyed by **stable per-version u32 field IDs** (not names:
renames must not rewrite every document), each with a kind (`Lww`, `Int64`, `Float32`,
`Float64`), a priority, and bytes (CBOR for LWW values, big-endian numbers for counters).
An LWW field with empty bytes is a tombstone, and the priority survives materialization:
a delete is an LWW write that wins by priority, not a special row.

The whole document is one value under one key, and the *field IDs* are the document's
**parts**: this is what makes projected reads (`get_parts(key, parts)`) and blind
part-operand merges the read and write primitives of the document. Part IDs are
allocated from the systemstore and persisted per schema version
([`crates/db/src/collection/parts.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/collection/parts.rs)), so two writers on two replicas name the same
fields with the same part IDs.

## The in-memory document

The `Document` in memory ([`crates/document/src/document.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/document/src/document.rs)) carries the schema-typed
field map, the current head CID, dirty tracking, the collection reference, counter
deltas for updates, and the write preparation (precomputed keys and identity). Documents
are built from client JSON against the collection's schema, validated, and then either
computed into blocks (create/update) or read back from the stored format.

## Deletion

Deletion is logical: a deletion marker under `d/del/` (an Ordinary key, so racing deletes
have a winner), the index entries removed, the series released, and a delete block
appended to the DAG (status `DELETED`). The blocks and value history remain until a
retention policy prunes them (see [Memory reclamation](../memory/index.md)), so a delete
replicates like any other write and time travel still sees the document's life.

> **Why UUIDv5 of the genesis CID, and why short IDs at all.** The DocID gives a
> replica-stable identity derived from content (no coordinator, no counter, no
> allocation race across nodes). The short ID gives a compact, locally ordered key. Two
> identities, two jobs: one travels the network, one stays in the keys.
