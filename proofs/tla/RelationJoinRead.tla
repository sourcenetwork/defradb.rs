---- MODULE RelationJoinRead ----
\* The story. `Book { publisher: Publisher @primary }` and
\* `Publisher { published: Book }` make a one-to-one relation. Two explicit
\* transactions, T1 and T2, begin together. Each adds one book linked to a
\* publisher: T1 always to p1, T2 to p1 or p2. Then each runs the query
\* `Book { name publisher { name } }`, whose one-to-one join scans every
\* book. Then both try to commit, in either order.
\*
\* What must never happen:
\*   - two committed books link the same publisher (the one-to-one relation
\*     is broken);
\*   - a transaction loses its commit when nobody else linked its publisher,
\*     only because its query scanned a range the other book landed in (a
\*     query read decides nothing, AGENTS.md "Reads and writes").
\*
\* Two guards keep both promises:
\*   - the query's join scan is a plain scan, never validated at commit;
\*   - the uniqueness check a writer makes before it writes its publisher's
\*     relation entry is validated, so a crossing writer of the same
\*     publisher aborts it.
\*
\* Rust:
\*   crates/engine/query/src/runner/query/nested.rs
\*     QueryRunner::execute_nested_select_with_planner (the join scan, which
\*     no longer turns scan validation on);
\*   crates/database/db/src/index/manager/unique.rs
\*     IndexManager::save_local_unique_validated (the writer's validated
\*     uniqueness check).
\* Numbers for "<=" and set sizes for Cardinality.
EXTENDS Naturals, FiniteSets

\* ValidateJoinScan: TRUE plants the bug where the query's join scan is
\*   validated at commit (what #3182 did to copy Go).
\* ValidateOwnEntry: FALSE plants the bug where the writer's own uniqueness
\*   check is not validated.
\* T2Publishers: the publishers T2 may link ("p1", the same as T1, or "p2").
CONSTANTS ValidateJoinScan, ValidateOwnEntry, T2Publishers

\* The two transactions.
Txns == {1, 2}

\* The two publishers.
Pubs == {"p1", "p2"}

\* The key of the book a transaction adds; both lie in the Book range.
BookKey(t) == IF t = 1 THEN "book1" ELSE "book2"

\* Every book key: the range the join scan walks.
BookRange == {"book1", "book2"}

\* The key of a publisher's relation entry in the unique index.
EntryKey(p) == IF p = "p1" THEN "entry1" ELSE "entry2"

\* phase[t]: where t is ("start", "wrote", "read", "done").
\* pub[t]: the publisher t linked ("none" until it writes).
\* writes[t]: the keys t wrote.
\* validated[t]: the keys t's commit checks for crossing writes.
\* changed[t]: the keys the other transaction committed after t began.
\* outcome[t]: what t's commit did ("none", "commit" or "abort").
\* holders[p]: the committed transactions whose book links p.
VARIABLES phase, pub, writes, validated, changed, outcome, holders

\* All the variables, for stuttering steps.
vars == <<phase, pub, writes, validated, changed, outcome, holders>>

\* Both transactions have begun; nothing is written or committed.
Init ==
    \* Neither has written yet.
    /\ phase = [t \in Txns |-> "start"]
    \* Neither has picked a publisher.
    /\ pub = [t \in Txns |-> "none"]
    \* Neither has written a key.
    /\ writes = [t \in Txns |-> {}]
    \* Neither commit checks a key yet.
    /\ validated = [t \in Txns |-> {}]
    \* Nothing was committed since they began.
    /\ changed = [t \in Txns |-> {}]
    \* Neither has tried to commit.
    /\ outcome = [t \in Txns |-> "none"]
    \* No publisher is linked.
    /\ holders = [p \in Pubs |-> {}]

\* The publishers t may link: T1 always p1, T2 what the config allows.
Choices(t) == IF t = 1 THEN {"p1"} ELSE T2Publishers

\* t adds its book linked to p: the book's key and p's relation entry. The
\* check that p's entry is free is validated unless the second bug is planted.
Write(t, p) ==
    \* t has not written yet.
    /\ phase[t] = "start"
    \* t remembers the publisher it linked.
    /\ pub' = [pub EXCEPT ![t] = p]
    \* t writes its book and p's relation entry.
    /\ writes' = [writes EXCEPT ![t] = {BookKey(t), EntryKey(p)}]
    \* The uniqueness check on p's entry counts at commit, if it is guarded.
    /\ validated' = [validated EXCEPT ![t] =
         IF ValidateOwnEntry THEN {EntryKey(p)} ELSE {}]
    \* t moves on to its query.
    /\ phase' = [phase EXCEPT ![t] = "wrote"]
    \* Nothing else changes.
    /\ UNCHANGED <<changed, outcome, holders>>

\* t runs `Book { name publisher { name } }`. Its join scans the Book range;
\* only the buggy version adds that range to the keys its commit checks.
Query(t) ==
    \* t has written its book.
    /\ phase[t] = "wrote"
    \* The scan counts at commit only in the planted bug.
    /\ validated' = [validated EXCEPT ![t] =
         validated[t] \cup (IF ValidateJoinScan THEN BookRange ELSE {})]
    \* t is ready to commit.
    /\ phase' = [phase EXCEPT ![t] = "read"]
    \* Nothing else changes.
    /\ UNCHANGED <<pub, writes, changed, outcome, holders>>

\* t commits: it aborts when a key it checks was changed by the other after
\* t began; otherwise its book lands and the other sees t's keys as changed.
Commit(t) ==
    \* t has run its query.
    /\ phase[t] = "read"
    \* t is finished either way.
    /\ phase' = [phase EXCEPT ![t] = "done"]
    \* A checked key the other changed aborts the commit.
    /\ IF validated[t] \cap changed[t] # {}
         THEN \* Aborted: nothing lands.
              /\ outcome' = [outcome EXCEPT ![t] = "abort"]
              \* Nobody sees a change.
              /\ UNCHANGED <<changed, holders>>
         ELSE \* Committed: t's book links its publisher.
              /\ outcome' = [outcome EXCEPT ![t] = "commit"]
              \* t joins the holders of its publisher.
              /\ holders' = [holders EXCEPT ![pub[t]] = @ \cup {t}]
              \* Every other transaction now sees t's keys as changed.
              /\ changed' = [u \in Txns |->
                   IF u = t THEN changed[u] ELSE changed[u] \cup writes[t]]
    \* What t wrote and checks stays as it was.
    /\ UNCHANGED <<pub, writes, validated>>

\* Any one transaction takes any one step.
Next ==
    \E t \in Txns :
        \* t writes its book for one of its allowed publishers.
        \/ \E p \in Choices(t) : Write(t, p)
        \* t runs its query.
        \/ Query(t)
        \* t commits.
        \/ Commit(t)

\* The whole behaviour: start in Init, then take steps.
Spec == Init /\ [][Next]_vars

\* Promise 1: no publisher is linked by two committed books.
INV_OneBookPerPublisher == \A p \in Pubs : Cardinality(holders[p]) <= 1

\* Promise 2: a transaction loses its commit only when the other committed
\* a book for the same publisher.
INV_JoinReadDecidesNothing ==
    \A t \in Txns :
        outcome[t] = "abort" =>
            \E u \in Txns \ {t} : outcome[u] = "commit" /\ pub[u] = pub[t]
====
