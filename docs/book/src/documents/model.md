# The document model

## The DocID

A document's identity is its **DocID**: a UUIDv5 over a fixed namespace of the genesis
composite block's CID ([`crates/building-blocks/document/src/doc_id.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/building-blocks/document/src/doc_id.rs)). The string form is
`{base32 version}-{uuid}` (for example `bae-...`). The DocID is stable across replicas:
any node that has the same genesis block computes the same DocID, which is what makes
replication, dedup and the once-keys of the arrival journal possible.

The CID itself is not recoverable from the DocID string; it lives in the DAG.

### A signed create's DocID is known before it commits

The guarantee: a document's ID is `derive_doc_id` of its genesis composite block's CID
(`DocID::new_v0`, [`crates/database/db/src/block/builder/mod.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/db/src/block/builder/mod.rs)).
When the create is signed, `compute_document_blocks` signs each field block before
taking its CID, links those CIDs from the composite, and signs the composite before
taking its CID. Every field signature, the composite signature and the collection
version ID are therefore part of the ID, and whoever builds or verifies the blocks knows
the ID before anything commits. No path gives a committed document another ID, except
backup import and the store-upgrade re-key of legacy documents (below). Requirements:
`reqs/defra/document-id.md`; proof: `proofs/lean/Cid/SignedGenesis.lean`
(`node_commits_precomputed`, `client_signed_commits_precomputed`,
`peer_commits_creators_id`, each beside a RED theorem that drops its check, and
`rekey_moves_a_legacy_id`).

| Path | Where the ID is first known | What holds | Test |
|------|-----------------------------|------------|------|
| Node-signed, autocommit | the mutation response | stored under the ID it returned | `txn::signed_doc_id::an_autocommit_node_signed_create_stores_the_id_it_returned` |
| Node-signed, explicit transaction | the mutation response, before commit | the commit keeps it | `txn::signed_doc_id::a_node_signed_create_in_a_txn_commits_the_id_it_returned_{ed25519,secp256k1}` |
| Client-signed, embedded | `graph.doc_id()`, before sending | `TxnNode::execute_verified_create` commits `graph.doc_id()` or discards the transaction | `signature::client_signed::*` (in-process) |
| Client-signed, rust-http | `graph.doc_id()`, before sending | the Rust client refuses an out-of-graph wave and a different `_docID` | `signature::client_signed::*` (rust-http) |
| P2P merge | the creator's response | the peer keeps a block only when it hashes to the CID it arrived under (`verify_block_cid` on a replicator push, gossip, block sync and CAR fetch; `verify_hash` on a bitswap fetch) and derives the ID from that CID | `signature::doc_id_determinism::a_peer_shows_the_creators_doc_id`, `signature::peer::*` |
| Lens migration | unchanged | a migration rewrites field values, never the genesis, so the document is read under its ID | `collection_version::migrations::query::with_doc_id::collection_migration_query_by_doc_id` (unsigned) |
| Store-upgrade re-key | not kept for a legacy ID | `resolve_legacy_identities` maps a legacy document to its genesis's ID; a stored ID that differs becomes an alias | none: no legacy-store fixture |
| Backup import | kept only when the same genesis is rebuilt | a new create under its genesis's ID; a file ID that differs becomes an alias | `backup::simple::import_alias::*` |

The HTTP guarantee is the client's. The server's websocket signer (`SessionSigner`)
checks each signature against the session key and compares no ID, so a websocket
client that signs whatever it is sent gets no guarantee beyond the `_docID` in the
response. go-http and cli have no client-signed create.

Encrypted creates are not client-signable. Their ciphertext is memoized per request, so
a retried attempt keeps the ID, but two separate creates of the same content differ.

Signature determinism by key type:

| Key type | Same key and content, same ID? | Test |
|----------|--------------------------------|------|
| Ed25519 | yes | `signature::doc_id_determinism::the_same_key_and_content_give_the_same_doc_id_ed25519` |
| Secp256k1 (RFC 6979) | yes | `signature::doc_id_determinism::the_same_key_and_content_give_the_same_doc_id_secp256k1` |
| Secp256r1 (remote or Secure Enclave, randomized ECDSA) | no | not run |
| BLS through Orbis | yes by BLS uniqueness | not run |
| Any other remote signer | no guarantee | — |

A different key always gives a different ID, because the signature block names its
public key (`signature::doc_id_determinism::a_different_key_gives_a_different_doc_id`).

A create whose ID is already stored is refused, and nothing is written:

| Stored state | Text (all clients) | Code (in-process, rust-http) | REST status |
|--------------|--------------------|------------------------------|-------------|
| live | `a document with the given ID already exists. DocID: <id>` | `DOCUMENT_ALREADY_EXISTS` | 409 |
| deleted | `a document with the given ID has been deleted. DocID: <id>` | `DOCUMENT_DELETED` | 400 |

The code travels in a GraphQL error's `extensions.code` and in the REST error body's
`code` field, which defradb.rs adds beside Go's `error` (as it does for `STORAGE_FULL`).
cli prints the text alone, as Go's CLI does, and a Go node sends no code. Tests:
`mutation::add::signed_duplicate::*`.

What counts as breaking the guarantee: changing the DocID version or its derivation;
leaving a field signature or the composite signature out of the CID (or adding one in);
leaving the collection version ID out of the genesis; any path that re-signs a genesis or
commits a document under an ID other than its precomputed one; removing the committed-ID
check of `TxnNode::execute_verified_create` or of the Rust client's signing conversation.

There are two documented exceptions. Backup import is a stopgap: it creates each
document again on the importing node, so the document gets its new genesis's ID, and
the ID the file names is kept as an alias of it (`set_doc_id_alias`). The ID is the
file's again only when the importing node rebuilds the same genesis: the same signing
key with a signer that ignores its nonce (or no signing), the same collection version
and the same field values; then there is no alias to keep. An ID-preserving
migration built on collection configuration and the document DAG is meant to replace
it. The store-upgrade re-key (`resolve_legacy_identities` in
`crates/database/db/src/definition/migration/shortid.rs`, run on every open) maps each
legacy document to the ID of its genesis, and keeps a stored ID that differs as an
alias. A document created under the current derivation already has its genesis's ID,
so the re-key leaves it alone; no test covers a legacy store.

## Short IDs

Locally, every document also gets a compact **short ID**: a u64 allocated from
`/seq/doc` in ranges of 1024 by a lock-free CAS allocator ([`crates/database/db/src/docid/map.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/db/src/docid/map.rs)).
Short IDs key the datastore layout (`doc_key(doc_short_id)`), which makes iteration
order match allocation order and keeps keys small. The systemstore holds the bidirectional
mappings (short ID to DocID and back), plus the DocRef, and the block-CID-to-DocID
ownership map that memory reclamation walks.

Allocation is `Db::allocate`: engine-side, atomic, and safe to call inside a retried
closure *because* a reserved range survives attempts (an attempt never allocates twice;
the prepared-inputs rule from the doctrine chapter).

## The stored format: documents as parts

The materialized value of a document is a `StoredDocument` ([`crates/building-blocks/document/src/stored.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/building-blocks/document/src/stored.rs)):
magic `DFV\x01`, then fields keyed by **stable per-version u32 field IDs** (not names:
renames must not rewrite every document), each with a kind (`Lww`, `Int64`, `Float32`,
`Float64`), a priority, and bytes (CBOR for LWW values, big-endian numbers for counters).
An LWW field with empty bytes is a tombstone, and the priority survives materialization:
a delete is an LWW write that wins by priority, not a special row.

The whole document is one value under one key, and the *field IDs* are the document's
**parts**: this is what makes projected reads (`get_parts(key, parts)`) and blind
part-operand merges the read and write primitives of the document. Part IDs are
allocated from the systemstore and persisted per schema version
([`crates/database/db/src/collection/parts.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/database/db/src/collection/parts.rs)), so two writers on two replicas name the same
fields with the same part IDs.

## The in-memory document

The `Document` in memory ([`crates/building-blocks/document/src/document.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/building-blocks/document/src/document.rs)) carries the schema-typed
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
