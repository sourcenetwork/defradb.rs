# What a unique index guarantees

This chapter is the promise a client gets when two writers on one node race on a value a
unique index covers (#3265). It states today's behaviour as the guarantee; nothing here
changes which reads are validated. Each cell says where it comes from: **verified** means
a named test pins it, **by regolith contract** means it follows from regolith's documented
validation and is not tested here.

## Per operation and commit mode

Two writers each add, or update a different document to, the same non-nil value of a
field with `@index(unique: true)`, a value no committed document holds yet. Exactly one
commits. (When another document already holds it, both are refused and neither
commits.) An autocommit loser is told of the unique violation when its retry
budget allows at least one re-run (`with_max_txn_retries`, 5 by default); with
none, it gets the retry-exhausted error. What the other is told:

| Writer's commit mode | The loser is told | Source |
|---|---|---|
| Autocommit create (GraphQL `add_*`) | `can not index a doc's field(s) that violates unique index. DocID: …, <field>: <value>`, code `UNIQUE_CONSTRAINT_VIOLATION`; nothing stored | verified: testo `index::unique_refusal_code`, db `write::conflict_reasons::every_round_of_a_free_unique_race_has_one_winner_and_one_typed_refusal` |
| Autocommit update (GraphQL `update_*`) | `failed to update index: <same text>. IndexName: …`, code `UNIQUE_CONSTRAINT_VIOLATION`; the document keeps its old value | verified: testo `index::unique_refusal_code::an_update_refused_by_a_unique_index_carries_its_code` |
| Explicit transaction, embedded (in-process) | the second commit fails with `transaction conflict. Please retry`; nothing it wrote stays; re-running its create meets the unique code | verified: testo `txn::unique_isolation` |
| Explicit transaction, HTTP | the commit fails with `transaction conflict. Please retry` and no `code`; the re-run's mutation carries `UNIQUE_CONSTRAINT_VIOLATION` | verified for the text and the re-run's code: testo `txn::unique_isolation` (Rust HTTP client). The commit answers `409` with `{"error": "transaction conflict. Please retry", "outcome": "transaction_conflict_aborted"}` (`commit_error_response` in `crates/surfaces/http/src/handlers/graphql/transactions.rs`); that status and receipt were observed against a running node and are not pinned by a test |
| Explicit transaction, CLI | the same commit text; the CLI prints messages only, so no code is shown | verified for the text: testo `txn::unique_isolation::the_second_commit_of_a_unique_value_is_a_conflict` |
| REST document create (`POST /collections/{name}`) | `409` with the unique text and `"code": "UNIQUE_CONSTRAINT_VIOLATION"`; nothing stored | verified: testo `index::unique_refusal_code::a_document_add_refused_by_a_unique_index_carries_its_code` (the Rust HTTP client's `AddDoc` posts to this route), plus the `handlers::documents` and `error` unit tests in `crates/surfaces/http` |
| `commit` and `commit_nowait` | identical: a commit on a worker's queue uses `commit_nowait`, which validates exactly as `commit` does | by regolith contract: regolith v0.2.5 `src/transaction/nowait.rs:1-6`; `RegolithTxn::commit` |

The loser loses because the write path's check of the unique entry
(`UniqueIndex::save`, a `has` of the entry key) is a validated Ordinary read: a commit of
that key after the loser's snapshot aborts it. That validation is regolith's
(`RepeatableRead.tla`, `MC_DefraLevel_*`), assumed here. The race itself, in both modes, is
modeled by `proofs/tla/ClientUniqueAdmission.tla`; the words and codes by
`proofs/lean/UniqueRefusal.lean`.

## Scope

- **Non-nil values only.** A nil value is never checked: its entry key carries the DocID,
  so any number of documents may hold nil (verified: `nil_values_are_not_refused`). A
  filter for nil is a prefix scan of those entries and decides nothing either (verified:
  `a_nil_writers_filter_for_nil_decides_nothing`).
- **No index, no refusal.** Without a unique index, a filtered read for the value is a
  plain scan that decides nothing, so both writers commit (verified:
  `without_an_index_both_commit`). This differs from Go, whose store validates the scan
  and aborts the second explicit commit with `transaction conflict. Please retry`; the
  test is listed in `.github/testo/divergences.txt`.
- **A filter read is not a decision.** The house rule is that a query read is never
  validated (AGENTS.md § Reads and writes), and the unique-index equality plan keeps it:
  it reads the entry with an unvalidated `get_parts(key, &[])`, so a transaction that only
  filtered for a value it does not write still commits when another writer adds that
  value, and so does one that writes nil (verified: `txn::unique_filter_read`
  `a_unique_eq_filter_read_does_not_fail_the_commit` and
  `a_nil_writers_unique_filter_read_does_not_fail_the_commit`). The model configurations
  `MC_ClientUniqueAdmission_Red_FilterReadDecides` and
  `MC_ClientUniqueAdmission_Red_FilterReadDecidesNil` plant the validated read the plan
  used before #3379 and break the rule. Go validates the read and aborts the commit; those
  tests are listed in `.github/testo/divergences.txt`.
- **One document is not a unique race.** Two updates of the same document to one value
  leave one document holding it: `UniqueIndex::update` accepts an entry its own DocID
  holds, so the re-run of an autocommit update that lost finds its own document there
  and both commit (from the code, not tested here).
- **Identical documents are a DocID race.** Two creates with identical content have the
  same DocID and race on the DocID mapping before any unique entry; the tests use
  documents that differ in another field.

## Refusal kinds

| Kind | Where | Message | Code |
|---|---|---|---|
| Transaction conflict | an explicit commit | `transaction conflict. Please retry` | none; HTTP `409` |
| Unique violation | a GraphQL mutation | the index's text, naming the writer's DocID and value | `UNIQUE_CONSTRAINT_VIOLATION` |
| Unique violation | REST | the same text | HTTP `409`, `"code": "UNIQUE_CONSTRAINT_VIOLATION"` |

The code is additive: the message is unchanged, and Go reports no code. The CLI prints
no code either, but a unique refusal it gets over HTTP now prints the message alone,
without the `server returned error: ` prefix it carried before: a script that matched
that prefix must match the message instead. Changing either the
message or the code is a breaking change for clients and goes through a requirement
(`reqs/defra/txn-isolation.md`) first.

## Boundaries

- **Peers.** Replication does not refuse: a merge that brings a second document holding a
  value resolves the collision and publishes a `UniqueIndexConflict` event after it
  commits (`crates/database/db/src/merge/merge_handler/composite.rs`). Which document
  keeps the entry is the merge's rule and is not part of this guarantee.
- **Backends.** regolith is the only storage engine, so the guarantee holds on every
  target (server, embedded, WASM) by the same validation.
