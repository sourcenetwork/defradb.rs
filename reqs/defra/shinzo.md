# `go.shinzo.index.range_filter`: A two-sided range filter fetches only the range

- **Parents:** ["go.shinzo.index"]
- **Scenarios:** ["index.range_filter.a-closed-range-fetches-only-the-range", "index.range_filter.an-open-range-on-a-descending-index-fetches-only-the-range", "index.range_filter.an-empty-range-fetches-nothing"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Over the index suite's ten users (ages 20, 23, 28, 30, 32, 33, 42, 44, 48, 55), `User(filter: {age: {_geq: 28, _leq: 33}}) { name }` on `age: Int @index` returns Fred, John, Islam, Andy with `indexFetches: 4`; `filter: {age: {_gt: 28, _lt: 44}}` on `age: Int @index(direction: DESC)` returns Addo, Andy, Islam, John with `indexFetches: 4`; and the empty range `{_gt: 44, _lt: 30}` returns `[]` with `indexFetches: 0`. Both bounds limit the index scan, not just the lower one.

# `go.shinzo.docid.no_neighbour_conflict`: Reading one document does not conflict with its neighbour

- **Parents:** ["go.shinzo.docid"]
- **Scenarios:** ["txn.neighbour.updating-alice-survives-a-rewrite-of-bob", "txn.neighbour.updating-bob-survives-a-rewrite-of-alice"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A transaction that reads document alice and updates her `age` to 10 commits successfully even though another writer rewrote every field of bob (the next document in the collection) and committed in between.

# `go.shinzo.document.typed_nillable_arrays`: Typed lists fill nillable-element arrays

- **Parents:** ["go.shinzo.document"]
- **Scenarios:** ["mutation.add.field_kinds.typed_nillable_arrays.string-and-datetime-lists-fill-nillable-element-arrays"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A `[String]` field given a typed string list (`["bafyA", "bafyB", "bafyC"]`) and a `[DateTime]` field given a typed time list (`2026-07-07T12:00:00Z`, `2026-07-08T12:00:00Z`) are stored with every element present, in order. Before Shinzo, Go dropped such a value silently.

# `go.shinzo.collection_id`: Collection IDs are deterministic

- **Parents:** ["go.shinzo"]
- **Scenarios:** ["collection.deterministic_ids.a-collection-beside-a-longer-namesake-gets-one-id"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a collection's version ID is content-addressed and must be identical on a Go Shinzo node and a Rust node given the same SDL, or the nodes do not agree on the collection"

Adding `type Foo { name: String } type FooBar { name: String }` to a fresh node gives `Foo` the same `VersionID` on every run (30 runs, one ID), even though `Foo` is a name prefix of `FooBar`. Its version ID is `bafyreigj37qgcjknekn3jqcmpai5ut6gqsfvjgczmu5iopkeeznjojbqri` on a Rust node and on a Go Shinzo node.
