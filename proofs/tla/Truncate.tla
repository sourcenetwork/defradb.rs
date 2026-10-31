---- MODULE Truncate ----
\* The story: a client asks the database to truncate a collection, which
\* removes its documents for good and keeps the collection. With a filter only
\* the documents the filter matches go; without one, every document goes. The
\* database does it once, in one place, for every way in (HTTP, CLI, FFI, the
\* embedded node).
\*
\* Truncate is not transactional: it deletes straight away, outside any
\* transaction the caller has open. So before it deletes anything it checks, in
\* Go's order, and stops at the first that fails:
\*   1. nac         the caller may truncate collections on this node;
\*   2. branchable  a filtered truncate is refused on a branchable collection,
\*                  whose history the documents share;
\*   3. txn         a filtered truncate is refused inside a transaction;
\*   4. shape       the filter is a filter: conditions, or source text that
\*                  parses to conditions.
\* An unfiltered truncate skips checks 2 to 4 and removes every document, in a
\* transaction or not.
\*
\* Why the txn check: inside a transaction the caller sees its own uncommitted
\* writes. A filter evaluated there can match a document only because of a
\* write that is later discarded, yet the delete is already durable. Outside a
\* transaction the filter sees only what is committed.
\*
\* Who acts: the caller's request moving through the checks, then the delete.
\* Which documents the filter matches in committed state, and which it would
\* match in the transaction's own view, is chosen up front, as is every check's
\* answer, so the model explores every combination.
\*
\* What must never happen: a document removed for a caller without permission;
\* a filtered truncate removing anything from a branchable collection; a
\* filtered truncate removing a document that does not match in committed
\* state; a filter that is not one removing anything.
\*
\* Code it models (crates/db/src/collection/ops/truncate/mod.rs):
\*   DB::truncate_collection_as  the whole sequence, for every edge
\*   DB::truncate_target         checks 1 to 3, in Go's order
\*   conditions                  check 4: the filter's shape
\*   DB::truncate                the delete, all (all.rs truncate_all) or matching (filtered.rs truncate_matching)
\*   DB::truncate_parsed_as      the same sequence for the GraphQL truncate mutation, its filter already parsed
EXTENDS Naturals, FiniteSets   \* numbers to compare, and Cardinality to count documents

CONSTANTS
  Docs,      \* the documents in the collection
  Enforced   \* which checks the database really runs (a subset of the four)

\* At least two documents, so a filter can match some and not others.
ASSUME Cardinality(Docs) >= 2
\* Only the four named checks can be enforced.
ASSUME Enforced \subseteq {"nac", "branchable", "txn", "shape"}

VARIABLES
  permitted,       \* the caller holds the truncate permission
  branchable,      \* the collection is branchable
  filtered,        \* the request carries a filter
  inTxn,           \* the request runs inside an explicit transaction
  shapeOk,         \* the filter is a filter
  committedMatch,  \* the documents the filter matches in committed state
  txnMatch,        \* the documents it matches in the transaction's own view
  step,            \* "check", then "delete", then "done"
  removed,         \* the documents this request removed
  refused          \* the request ended with an error

\* Every variable, for saying a step leaves them all as they are.
vars == <<permitted, branchable, filtered, inTxn, shapeOk,
          committedMatch, txnMatch, step, removed, refused>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ permitted \in BOOLEAN
  /\ branchable \in BOOLEAN
  /\ filtered \in BOOLEAN
  /\ inTxn \in BOOLEAN
  /\ shapeOk \in BOOLEAN
  /\ committedMatch \subseteq Docs
  /\ txnMatch \subseteq Docs
  /\ step \in {"check", "delete", "done"}
  /\ removed \subseteq Docs
  /\ refused \in BOOLEAN

\* Any request against any collection can arrive; nothing is removed yet.
Init ==
  /\ permitted \in BOOLEAN
  /\ branchable \in BOOLEAN
  /\ filtered \in BOOLEAN
  /\ inTxn \in BOOLEAN
  /\ shapeOk \in BOOLEAN
  /\ committedMatch \in SUBSET Docs
  /\ txnMatch \in SUBSET Docs
  /\ step = "check"
  /\ removed = {}
  /\ refused = FALSE

\* Whether an enforced check fails for this request. Checks 2 to 4 apply only
\* to a filtered truncate.
Refuse ==
  \/ "nac" \in Enforced /\ ~permitted
  \/ filtered /\ "branchable" \in Enforced /\ branchable
  \/ filtered /\ "txn" \in Enforced /\ inTxn
  \/ filtered /\ "shape" \in Enforced /\ ~shapeOk

\* The checks run before anything is deleted. A failing enforced check ends the
\* request with an error; otherwise the request goes on to the delete, even if
\* a check it skips would have failed (that is the bug a RED config plants).
Check ==
  /\ step = "check"
  /\ IF Refuse
       THEN /\ refused' = TRUE /\ step' = "done"
       ELSE /\ refused' = refused /\ step' = "delete"
  /\ UNCHANGED <<permitted, branchable, filtered, inTxn, shapeOk,
                 committedMatch, txnMatch, removed>>

\* The documents the delete takes. Without a filter, all of them. A filter that
\* is not one, let through, is read as no conditions and matches everything. A
\* filter run inside a transaction, let through, matches on the transaction's
\* own view. Otherwise the filter matches on what is committed.
Targets ==
  IF ~filtered THEN Docs
  ELSE IF ~shapeOk THEN Docs
  ELSE IF inTxn THEN txnMatch
  ELSE committedMatch

\* The delete: durable at once, outside any transaction.
Delete ==
  /\ step = "delete"
  /\ removed' = Targets
  /\ step' = "done"
  /\ UNCHANGED <<permitted, branchable, filtered, inTxn, shapeOk,
                 committedMatch, txnMatch, refused>>

\* Each step of a request is one of the two.
Next == Check \/ Delete

\* A finished request stays finished.
Done == step = "done" /\ UNCHANGED vars

\* A request starts in Init and then only checks, deletes or stays finished.
Spec == Init /\ [][Next \/ Done]_vars

\* No document is removed for a caller without the truncate permission.
INV_NoRemovalWithoutPermission == removed # {} => permitted

\* A filtered truncate removes nothing from a branchable collection.
INV_FilteredNeverOnBranchable == (filtered /\ branchable) => removed = {}

\* A filtered truncate removes only documents that match in committed state.
INV_FilteredRemovesOnlyCommittedMatches == filtered => removed \subseteq committedMatch

\* A filter that is not one removes nothing.
INV_BadFilterRemovesNothing == (filtered /\ ~shapeOk) => removed = {}

\* An unfiltered truncate that was allowed removes every document, in a
\* transaction or not.
INV_UnfilteredRemovesAll ==
  (step = "done" /\ ~filtered /\ ~refused) => removed = Docs
====
