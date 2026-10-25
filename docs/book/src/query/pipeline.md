# The GraphQL pipeline

The pipeline, in one line ([`crates/query/src/lib.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/query/src/lib.rs)):

```
GraphQL string -> Parser -> Mapper -> Planner -> Plan nodes -> Executor -> Results
```

## Parse and map

The parser (`query_parse/`) is built on `graphql_parser`. It produces a typed
`ParsedOperation`: queries (with selects, filters, ordering, aggregates, cursors, limits),
mutations (create/update/upsert/delete on collections, or raw documents), subscriptions,
and introspection. System collections are parsed here too: `_commits` (with `depth`,
`cid`, height ranges) and `_documentArrivals` (with `after`, `limit`, `docID`). Depth,
width and filter complexity are bounded by explicit limits, because a query is untrusted
input at a trust boundary.

Schema SDL is parsed in the same crate (`sdl_parse/`), and the GraphQL schema a client
introspects is generated from collection definitions (`schema_gen/`).

## Prepare

Mutation preparation ([`crates/query/src/runner/prepare.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/query/src/runner/prepare.rs)) runs once per request, outside any
transaction retry: write preparation resolves KMS keys, embeddings and signatures;
batches group; ACP-transactional providers bind. Remote inputs are resolved in waves at
the edge: an input that is not yet local ends the wave (`NeedsInput`), is fetched, and
the request re-prepares with it memoized by request bytes. This is the doctrine's
"everything with an outside effect is prepared" made concrete.

## Plan

The planner (`planner/`) turns the mapped operation into a tree of plan nodes, in the
Go-compatible order (joins below selects):

- **Scan and index selection**: filters are matched against available indexes
  (`filter_to_scan`, cost estimates, scan types: exact, range, prefix, IN, OR). Vector
  similarity routing picks a vector engine when the filter shape allows.
- **Joins**: relation joins (one/many), secondary-id joins, aggregate joins.
- **Aggregates, group-by, order-by, limit**; **cursors** applied by walking the tree to
  the scan nodes, so pagination stays index-ordered.
- **Views**: materialized views read a cached node; live views plan the underlying
  query.
- **Permission filters**: when a collection has a policy, the planner wraps its plan in
  a `PermissionFilterNode` that checks each document's access (bounded, with a small
  in-flight cap). App read validators hook the same node.

## Execute

The executor ([`crates/query/src/runner/executor.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/query/src/runner/executor.rs)) is a Volcano-style iterator over the plan: each node
pulls from its children. Reads stream: a scan yields pages from the storage iterator,
never a materialized set. Execution happens on a pool worker, routed at the edge; when
the request names an explicit transaction, it goes to that transaction's owning worker
(see [Explicit transactions](../txn/explicit.md)).

ACP runs in two places, both inside the request's snapshot:

- **Node access control (NAC)** before execution: a denial yields an empty result (Go
  parity), not an error, for queries.
- **Document access control (DAC)** as the permission filter node per document, and for
  mutations as pre-filtered visible doc IDs. `_commits` and arrivals entries are filtered
  the same way.

Deadlines ride the request end to end; a slow query fails its deadline rather than
running unbounded.

> **Why Volcano iterators and why planning joins below selects.** Streaming execution
> keeps memory flat in the result size (a page in flight, never the set), and pull-based
> iterators let limit/order/cursor pushdowns do their work before a row ever reaches the
> surface. The join placement is Go compatibility: plans match the reference
> implementation so explain outputs and optimizations correspond.
