# The Storage Engine

defradb.rs stores everything in one engine: **[regolith](https://sourcenetwork.github.io/regolith/)**, an LSM-transaction engine that
defradb.rs does not fork or wrap with adapters. It is opened directly, configured per
target (server, embedded, wasm, memory), and extended through traits that defradb.rs
implements (`crates/storage/src/backends/regolith/`).

This part covers the engine-facing surface a defradb.rs developer works with:

- **Keys, families and classes**: the byte-level layout of every key defradb.rs writes,
  and how the classifier maps key prefixes to the engine's behavioural classes.
- **The merge operator**: how counters, document parts, head sets and vector index
  records merge blindly and associatively inside the engine.
- **LSM structure**: what actually lands on disk (WAL, SSTs, manifest), how compaction
  runs, and how memory is bounded and reclaimed.
- **Encryption at rest**: AES-256-GCM-SIV sealing of every file, keys from a `KeyProvider`,
  values plaintext inside the engine.

The transactional mechanisms the engine provides on top (transactions, conflicts,
append, allocate) are covered in the [Transactions](../txn/index.md) part.
