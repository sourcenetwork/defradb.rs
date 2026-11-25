# Schema Tests

31 tests, none ignored.

## Files

- `encryption.rs` — Encrypted document P2P sync
- `schema_migration.rs` — Schema version migration during replication
- `index.rs` — Index sync across peers
- `one_to_many.rs` — Relational replication
- `se_cross_runtime.rs` — Searchable encryption across runtimes

The Vera-backed encryption and document ACP ports (`encryption_acp_*`) live in
`tests/vera/encryption_acp.rs`.
