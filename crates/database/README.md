# Database

The database itself: collections and documents, their writes and merges, and
the services that share data between nodes. A crate here combines the engine
into DefraDB's behaviour.

| Crate | What |
|-------|------|
| [db](db) | Database core: transactions, reads, writes, collections, merges, indexes, search |
| [replication-filter](replication-filter) | Query-filter-backed replication matcher |
| [pinc](pinc) | The pinc transport: endpoint, hubs, topics, embedder stream protocols |
| [p2p](p2p) | P2P sync engine: coordinator, replication, KMS topics |
| [kms](kms) | DEK generation, wrapping, and distribution under NAC/DAC |
| [pg-compat](pg-compat) | Postgres wire protocol compatibility |
