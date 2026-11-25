# Engine

The machinery the database runs on: query planning and execution, access
control, and storage over regolith. A crate here is a complete subsystem that
the database composes.

| Crate | What |
|-------|------|
| [query](query) | Query engine (GraphQL, BM25) |
| [vera-embedded](vera-embedded) | Transports to a Vera node hosted in the same process |
| [vera](vera) | Vera document ACP provider (native vera.rs) |
| [vera-delegation](vera-delegation) | Vera delegations and the policy commands they digest, without the provider |
| [acp](acp) | Access Control Policy |
| [datastore](datastore) | Data persistence abstractions |
| [blockstore](blockstore) | IPLD block storage |
| [storage](storage) | Storage layer over regolith, the one engine |
