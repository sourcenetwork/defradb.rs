# Collection schema-version OCC

`crates/database/db/src/write/autocommit/{create,update,delete}.rs` validates the
selected collection version inside the write attempt through
`DbTxn::validate_collection_for_write`; structural schema updates write the
collection-name mapping and new definition in `crates/database/db/src/definition/*`.
This model captures a writer selecting version 0, a concurrent structural put
advancing to version 1, and the writer's later commit. GREEN rejects the stale
attempt; RED admits its effects. The abstraction does not model the full
collection schema or prove storage's OCC implementation.
