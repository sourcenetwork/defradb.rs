# Introduction

<img src="diagrams/logo-defradb.svg" alt="DefraDB logo" class="defra-logo" />

The Book of Defra is the developer guide to **defradb.rs**: the Rust implementation of
[DefraDB](https://github.com/sourcenetwork/defradb.rs), a local-first, source-available database built
around three ideas:

1. **Documents as Merkle DAGs.** Every write produces content-addressed blocks (CIDs) linked
   into a per-document DAG. History is first-class: any past state can be re-materialized,
   replicated, and verified.
2. **Conflict-free replication.** Fields are CRDTs. Two nodes that apply the same set of
   blocks in any order converge to the same state; replication never needs a coordinator.
3. **A real storage engine underneath.** defradb.rs does not run on an embedded KV library
   with a mutex bolted on. It runs on **[regolith](https://sourcenetwork.github.io/regolith/)**, an LSM engine with MVCC transactions,
   server-side merges, commit-ordered logs, per-thread I/O queues, and encryption at rest.
   defradb.rs implements regolith's extension traits and keeps every business decision
   (queries, CRDT rules, access control, migrations) inside ordinary transactions.

## What this book covers

Each chapter takes one subsystem and answers three questions: **how does it work**, **where
does it live in the code**, and **why was it built this way**. Every claim in this book was
checked against the source of the `main` branch; file paths are given relative to the
repository root so you can jump straight to the code.

The book follows the reading order of the system itself:

- **The System**: the ownership map, the design doctrine, and one request traced end to end.
- **The Storage Engine**: key families, the merge operator, the LSM layout, encryption at rest.
- **Transactions**: lifecycle, the commit path, conflict rules and their business logic.
- **Reads and Writes**: which read and write mechanisms exist and when each is correct.
- **Documents and CRDTs**: the document model, blocks, CRDT types, head sets, the arrival journal.
- **Querying**: the GraphQL pipeline, indexes, history queries.
- **Access Control**: the Zanzibar engine, document- and node-level policy, external deciders.
- **Threading**: thread-per-core workers, the network edge, single-threaded targets.
- **Replication**: transports, push/pull sync, provider discovery.
- **Lens Migrations, Events, Memory Reclamation, API Surfaces, Verification.**

## Conventions

Every diagram is embedded as a rendered image, with its Mermaid source kept beside it in
`src/diagrams/` so it can be edited and re-rendered. The colour code is used consistently:

- **blue**: regolith, the storage engine;
- **green**: defradb.rs database logic;
- **purple**: trait implementations that plug defradb.rs into regolith mechanisms;
- **orange**: a race that needs one winner, or a client;
- **yellow**: a validated step, a gate, or I/O;
- **grey**: the network boundary.

Code references look like [`crates/storage/src/backends/regolith/classifier.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/classifier.rs), optionally
with a line number at the time of writing. The book is regenerated and deployed by CI
whenever it changes; if you find prose that disagrees with the code, trust the code and fix
the book.

## A note on scope

defradb.rs is the Rust sibling of the Go implementation
([sourcenetwork/defradb](https://github.com/sourcenetwork/defradb)) and tracks its API
surface, HTTP routes, and on-disk formats where compatibility matters (Go-compatibility
notes appear throughout the code). The storage engine and the execution architecture
described here, however, are native to the Rust implementation.
