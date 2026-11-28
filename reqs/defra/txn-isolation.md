# `defra.txn.isolation`: What a unique index guarantees under concurrent writers

- **Parents:** ["defra.txn"]

On one node, a unique index holds at most one committed document per non-nil value, whether the writers use explicit transactions or autocommit. Any number of documents may hold nil in the indexed field. Without an index nothing is refused: two documents that differ in any field are two documents. What each commit mode reads and validates is in the book's `txn/guarantees.md`.

# `defra.txn.isolation.unique-explicit`: The second explicit commit of a unique value is a conflict

- **Parents:** ["defra.txn.isolation"]
- **Scenarios:** ["txn::unique_isolation::the_second_commit_of_a_unique_value_is_a_conflict", "txn::unique_isolation::the_losers_rerun_is_refused_with_the_unique_code"]

With `type User { name: String @index(unique: true) age: Int }`, two explicit transactions each run `User(filter: {name: {_eq: "alice"}}) { name }` and get `[]`, then transaction 0 runs `add_User(input: {name: "alice", age: 1})` and transaction 1 runs `add_User(input: {name: "alice", age: 2})`. Committing transaction 0 succeeds. Committing transaction 1 fails with `transaction conflict. Please retry` under every client; the commit carries no error code. Afterwards `User { name age }` returns only `{name: "alice", age: 1}`, and `User(docID: <transaction 1's DocID>)` returns `[]`.

# `defra.txn.isolation.unique-explicit-retry`: Running the loser's create again is a unique refusal

- **Parents:** ["defra.txn.isolation"]
- **Scenarios:** ["txn::unique_isolation::the_losers_rerun_is_refused_with_the_unique_code"]

After `defra.txn.isolation.unique-explicit`, running transaction 1's `add_User(input: {name: "alice", age: 2})` again as an autocommit mutation fails with `can not index a doc's field(s) that violates unique index. DocID: <the new document's DocID>, name: alice`, and, through a client that exposes error codes (in-process and the Rust HTTP client), the GraphQL error's `extensions.code` is `UNIQUE_CONSTRAINT_VIOLATION`. Go and the CLI report no code.

# `defra.txn.isolation.unique-autocommit`: A second autocommit create of a unique value is refused

- **Parents:** ["defra.txn.isolation"]
- **Scenarios:** ["index::unique_refusal_code::a_second_autocommit_create_of_a_value_is_refused_and_stores_nothing", "index::unique_refusal_code::a_second_autocommit_create_of_a_value_carries_its_code"]

With `name: String @index(unique: true)` and a stored `{name: "alice", age: 1}`, the autocommit mutation `add_User(input: {name: "alice", age: 2})` fails with `can not index a doc's field(s) that violates unique index. DocID: <the new document's DocID>, name: alice`; the GraphQL error's `extensions.code` is `UNIQUE_CONSTRAINT_VIOLATION` on every client that reports codes. `User { name age }` still returns only `{name: "alice", age: 1}`. An autocommit create that loses a concurrent race to the same value runs again and is refused the same way.

# `defra.txn.isolation.unique-nil`: Nil is outside the unique guarantee

- **Parents:** ["defra.txn.isolation"]
- **Scenarios:** ["txn::unique_isolation::nil_values_are_not_refused", "txn::unique_isolation::a_nil_writers_filter_for_nil_decides_nothing"]

With `name: String @index(unique: true)`, two explicit transactions that run `add_User(input: {age: 1})` and `add_User(input: {age: 2})`, leaving `name` nil, both commit, and a third autocommit `add_User(input: {age: 3})` succeeds. `User(filter: {name: {_eq: null}}) { age }` returns all three documents. An explicit transaction that first runs `User(filter: {name: {_eq: null}}) { age }` (no documents) and then adds `{age: 1}` still commits after an autocommit `add_User(input: {age: 2})`: a filter for nil decides nothing. This requirement covers the nil value only; a nil writer whose transaction filtered for a non-nil value is covered by `txn::unique_filter_read`.

# `defra.txn.isolation.no-index-not-refused`: Without an index, equal values are not refused

- **Parents:** ["defra.txn.isolation"]
- **Scenarios:** ["txn::unique_isolation::without_an_index_both_commit"]

With `type User { name: String age: Int }` and no index, two explicit transactions that each read `User(filter: {name: {_eq: "alice"}}) { name }` as `[]` and then add `{name: "alice", age: 1}` and `{name: "alice", age: 2}` both commit, and `User { name age }` returns both documents. This is a deliberate divergence from Go, which validates the filter scan and fails the second commit with `transaction conflict. Please retry`: a query read decides nothing (AGENTS.md § Reads and writes).

# `defra.index.unique`: Unique index refusals

- **Parents:** ["defra.index"]

How a write that would put a second document on a unique index value is refused.

# `defra.index.unique.refusal-code`: A unique refusal carries `UNIQUE_CONSTRAINT_VIOLATION`

- **Parents:** ["defra.index.unique"]
- **Scenarios:** ["index::unique_refusal_code::a_create_refused_by_a_unique_index_carries_its_code", "index::unique_refusal_code::an_update_refused_by_a_unique_index_carries_its_code", "index::unique_refusal_code::a_document_add_refused_by_a_unique_index_carries_its_code"]

A GraphQL mutation refused by a unique index reports `extensions.code` `UNIQUE_CONSTRAINT_VIOLATION` beside its message, which is unchanged: `can not index a doc's field(s) that violates unique index. DocID: <writer's DocID>, name: alice` for `add_User`, and `failed to update index: can not index a doc's field(s) that violates unique index. DocID: <writer's DocID>, name: alice. IndexName: User_name_ASC` for an `update_User` that changes `name` to a value another document holds. The REST document API (`POST /collections/User` with `{"name": "alice", "age": 2}`) answers the same refusal with status 409 and the body `{"error": <message>, "code": "UNIQUE_CONSTRAINT_VIOLATION"}`, stores nothing, and the Rust HTTP client returns it as a unique violation with the message unchanged. Go reports no code; the CLI prints only the message, without the `server returned error: ` prefix it printed before this requirement.
