# `go.shinzo.signing`: Batch signing of written documents

- **Parents:** ["go.shinzo"]

An embedder turns on batch signing for a write by attaching a CID collector to it (`node.NewBatchCIDCollector`, `node.ContextWithBatchSigning`). Instead of signing every block, the node records the documents it writes and signs them once, over a Merkle root, with `node.SignBatch`.
