# Summary

[Introduction](./intro.md)

- [The System](./system/index.md)
  - [Who owns what](./system/responsibilities.md)
  - [Design doctrine](./system/doctrine.md)
  - [A request, end to end](./system/request.md)

- [The Storage Engine](./storage/index.md)
  - [Keys, families and classes](./storage/keys.md)
  - [The merge operator](./storage/merge-operator.md)
  - [LSM structure: WAL, SSTs, compaction](./storage/lsm.md)
  - [Encryption at rest](./storage/encryption-at-rest.md)

- [Transactions](./txn/index.md)
  - [Lifecycle and states](./txn/lifecycle.md)
  - [The commit path](./txn/commit.md)
  - [Conflict rules](./txn/conflict-rules.md)
  - [Conflict business logic](./txn/conflict-logic.md)
  - [Explicit transactions](./txn/explicit.md)

- [Reads](./reads/index.md)
  - [Read scenarios](./reads/scenarios.md)

- [Writes](./writes/index.md)
  - [Write scenarios](./writes/scenarios.md)

- [Documents and CRDTs](./documents/index.md)
  - [The document model](./documents/model.md)
  - [Blocks and the DAG](./documents/blocks.md)
  - [CRDT types](./documents/crdt.md)
  - [Head sets](./documents/head-sets.md)
  - [The arrival journal](./documents/arrival-journal.md)

- [Querying](./query/index.md)
  - [The GraphQL pipeline](./query/pipeline.md)
  - [Indexes](./query/indexes.md)
  - [History & Time Travel](./query/history.md)

- [Access Control](./acp/index.md)
  - [The Zanzibar engine](./acp/zanzibar.md)
  - [DAC and NAC](./acp/dac-nac.md)
  - [ACP inside the transaction](./acp/transactional.md)
  - [Vera: external policy decisions](./acp/vera.md)

- [Threading](./threading/index.md)
  - [Why thread-per-core](./threading/why.md)
  - [The worker pool](./threading/pool.md)
  - [The network edge](./threading/edge.md)
  - [One thread: wasm and single-core](./threading/single.md)

- [Replication](./p2p/index.md)
  - [Transports and the peer](./p2p/transports.md)
  - [Push and pull](./p2p/sync.md)
  - [Providers, admission, budgets](./p2p/providers.md)

- [Lens Migrations](./lens/index.md)

- [Events](./events/index.md)

- [Memory Reclamation](./memory/index.md)

- [API Surfaces](./api/index.md)

- [Verification](./proofs/index.md)
