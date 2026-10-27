# Blocks and the DAG

Every write to a document produces **blocks**: content-addressed CBOR payloads whose
links form a Merkle DAG per document. Blocks are the replication unit: P2P ships blocks,
merges apply blocks, history queries walk blocks, signatures sign blocks.

## What a write produces

The block builder (`crates/db/src/block/builder/`) computes, for one write:

- **Field blocks**: one per modified field, carrying the CRDT delta (an LWW value with
  its priority, or a counter increment) and linking the field's previous heads.
- **Encryption blocks**: when a field is encrypted, a separate `Encryption` block holds
  the fresh AES-256 key, and the field block links it. The block CID is computed over
  the ciphertext. With a KMS configured, the key is released through it (gated by its
  policy); without one, the node reads the key from the `Encryption` block it stores
  and decrypts locally, so field confidentiality against the operator exists only with
  a KMS in the path.
- **Signature blocks**: when signing is configured, a signature over the block (or a
  batch signature: a Merkle root over a set of CIDs, signed once per commit).
- **Immutable blocks**: fields marked `@immutable` are frozen as plaintext blocks at
  creation.
- **The composite block**: links all field blocks of this write, carries the document
  status (active/deleted), and links the previous composite heads as parents. The
  composite block's CID at create time is the document's genesis, hence its DocID.
- **Collection blocks**: for branchable collections, a collection-level delta linking
  the document's composite block, blind-merged into the collection head set.

## CIDs

A CID is computed from the block's canonical bytes
([`crates/defra-core/src/block.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/defra-core/src/block.rs)). Identical bytes produce identical CIDs: that is why
two replicas that apply the same writes produce the same history, and why block writes
are ContentAddressed and cannot conflict.

## Priorities

Every CRDT delta carries a priority (a u64 varint, [`crates/crdt/src/priority.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/crdt/src/priority.rs)):
create writes priority 1; an update writes the field's current max priority plus one;
the composite writes the document's max plus one. Priorities are what order concurrent
LWW writes deterministically without a coordinator: given two values, the higher
priority wins, and the merge algebra is proven convergent in Lean.

## Verification

Replicated blocks are checked before merge ([`crates/db/src/block/verify.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/block/verify.rs)): the CID
must match the bytes (content integrity), and attached signatures are verified when
present (including BLS-augmented batch signatures); unsigned blocks are supported. The
read gate checks ACP before a block is even served. A
document's DAG that fails verification is quarantined (the `PendingDagQuarantined` event
carries the forensics).

## Reading the DAG back

History reads walk the DAG: `_commits` by CID or depth
(`crates/db/src/read/commits/`), versioned (time-travel) reads walk from a target CID
back to genesis and replay CRDT deltas forward ([`crates/db/src/read/versioned.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/read/versioned.rs)). The
[query chapter](../query/history.md) covers the API surface.

> **Why blocks when there is a local value.** The local value is an index of "now"; the
> DAG is the truth. Replication, verification, encryption scoping, signing, history and
> GC-retention all operate on blocks, and any replica can rebuild any document's value
> from them. The materialized value exists so queries never walk a DAG to answer "what
> is this field".
