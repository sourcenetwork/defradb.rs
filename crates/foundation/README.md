# Foundation

The ground everything stands on: core types, the runtimes, and small leaf
utilities. A crate here depends on no other level, so any crate in the
workspace may use it.

| Crate | What |
|-------|------|
| [edge](edge) | The network runtime: one runtime for I/O and networking behind a narrow handle |
| [events](events) | Pub/sub event bus (subscriptions) |
| [schema](schema) | Schema validation |
| [crypto](crypto) | Cryptographic operations |
| [defra-core](defra-core) | Core types and traits |
| [lens](lens) | Schema migration via WASM transforms |
| [workers](workers) | Thread-per-core worker pool: topology, placement, local executors, routing |
| [keyring](keyring) | Key storage |
| [cursor](cursor) | Opaque cursor token codec for GraphQL pagination |
| [defra-version](defra-version) | Version metadata and Go compat tracking |
