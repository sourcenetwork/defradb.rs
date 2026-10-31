# `go.shinzo.signing`: Batch signing of written documents

- **Parents:** ["go.shinzo"]

An embedder turns on batch signing for a write by attaching a CID collector to it (`node.NewBatchCIDCollector`, `node.ContextWithBatchSigning`). Instead of signing every block, the node records the documents it writes and signs them once, over a Merkle root, with `node.SignBatch`.

# `go.shinzo.signing.batch_collects_composites`: Batch mode collects each document's composite CID and leaves blocks unsigned

- **Parents:** ["go.shinzo.signing"]
- **Go-Tests:** ["internal/core/block/store_test.go::TestAddDelta_WithBatchCollector_CollectsCompositeCIDsOnly", "internal/core/block/batch_signing_test.go::TestBatchCIDCollector_LenTracksAddedCIDs"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "The BatchSignature layout and Merkle root must match Go byte for byte, so a Go Shinzo node verifies a Rust batch with VerifyBatchSignature and a Rust node verifies a Go one."
- **Go-Bug-Suspect:** "Go's verifier helper CollectDocumentCIDs scans the whole headstore prefix of each document (`HeadstoreDocKey{DocShortID}` with no field ID), so it returns field heads as well as the composite head, while batch mode collects only composite CIDs: a root recomputed from CollectDocumentCIDs differs from the signed root whenever the document has field heads. VerifyBatchSignature also ignores `cidCount`, and the odd-layer duplication means `[a, b, c]` and `[a, b, c, c]` have the same root. SignBatch, VerifyBatchSignature and ComputeMerkleRoot have no Go test."

With batch signing on and a signing identity in context, writing a document stores its blocks with no per-block `signature` link, and the collector holds exactly one CID per document: its composite (`C`) block. Field blocks are not collected, so a field value shared by two documents adds nothing. The collector's length counts every CID added, in order. Without a collector, a signing-enabled write signs each block as before (`go.shinzo.signing.per_block_without_batch`).

`SignBatch` builds the root over the collected CIDs: sort them by their CID bytes, hash each with SHA-256 to make the leaf layer, then repeatedly hash the concatenation `left || right` of each pair (an odd last node is paired with itself) until one 32-byte hash remains; no CIDs gives `SHA-256("")`. It signs the root with the identity's key and returns `BatchSignature { header: SignatureHeader { type, identity }, value, merkleRoot, cidCount }` (IPLD schema `type BatchSignature struct { header SignatureHeader value Bytes merkleRoot Bytes cidCount Int }`): `type` is `ES256K` for a secp256k1 key and `EdDSA` for Ed25519, `identity` is the public key as lowercase hex (compressed SEC1 for secp256k1, raw 32 bytes for Ed25519), and `cidCount` is the number of collected CIDs. With no signing identity in context it fails with `batch signing requires an identity in context`; another key type fails with `unsupported key type for signing`. `VerifyBatchSignature(sig, cids)` recomputes the root from `cids`, returns false when it differs from `merkleRoot`, and otherwise verifies `value` over `merkleRoot` with the key named by the header. It does not compare `cidCount` with the number of `cids`: the count is recorded, not checked, so for a batch signed over `[a, b, c]` the list `[a, b, c, c]` recomputes the same root and verifies too.

