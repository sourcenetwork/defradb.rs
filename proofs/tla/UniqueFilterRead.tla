---- MODULE UniqueFilterRead ----
\* The story. A collection has a unique index on `name`. An explicit
\* transaction T first runs a query that filters for name "alice" through
\* that index, finds nobody, and then adds one document: bob, a document with
\* no name (nil), or alice herself. Meanwhile an autocommit writer W adds
\* alice and commits. Then T tries to commit.
\*
\* What must never happen:
\*   - two committed documents hold "alice" (the unique index is broken);
\*   - T loses its commit only because its query looked for alice, when T
\*     never wrote alice (a query read decides nothing, AGENTS.md
\*     "Reads and writes").
\*
\* Two guards keep both promises:
\*   - the filter read of the unique entry is unvalidated
\*     (`get_parts(key, &[])`);
\*   - the uniqueness check a writer makes before writing its own entry is
\*     validated (`has`), so a crossing writer of the same value aborts it.
\*
\* Rust:
\*   crates/engine/storage/src/index/eq_iterator.rs ExactMatchIterator::new_unique
\*     (the filter read);
\*   crates/engine/storage/src/index/unique.rs UniqueIndex::save
\*     (the writer's validated uniqueness check).
\* Numbers for "<=" and set sizes for Cardinality.
EXTENDS Naturals, FiniteSets

\* ValidateFilterRead: TRUE plants the bug where the query's read is validated.
\* ValidateOwnEntry: FALSE plants the bug where the writer's own check is not.
\* Writes: which documents T may add ("bob", "nil" or "alice").
CONSTANTS ValidateFilterRead, ValidateOwnEntry, Writes

\* phase: where T is ("start", "read", "wrote", "done").
\* wrote: the document T added ("none" until it adds one).
\* validated: the index keys T's commit checks for crossing writes.
\* changed: the index keys another writer committed after T began.
\* alice: the committed documents holding the "alice" entry.
\* wDone: whether W has finished (committed or been refused).
\* outcome: what T's commit did ("none", "commit" or "abort").
VARIABLES phase, wrote, validated, changed, alice, wDone, outcome

\* All the variables, for stuttering steps.
vars == <<phase, wrote, validated, changed, alice, wDone, outcome>>

\* At the start T has begun, nothing is written, and nobody holds alice.
Init ==
    \* T has begun but not yet read.
    /\ phase = "start"
    \* T has added nothing yet.
    /\ wrote = "none"
    \* T's commit checks no keys yet.
    /\ validated = {}
    \* Nobody else has committed since T began.
    /\ changed = {}
    \* No committed document holds alice.
    /\ alice = {}
    \* W has not run yet.
    /\ wDone = FALSE
    \* T has not tried to commit.
    /\ outcome = "none"

\* T runs its query: a lookup of the "alice" entry in the unique index.
\* Only the buggy version adds that key to the keys its commit checks.
TRead ==
    \* T must not have read already.
    /\ phase = "start"
    \* The query read is checked at commit only in the planted bug.
    /\ validated' = IF ValidateFilterRead THEN {"alice"} ELSE {}
    \* T moves on to its write.
    /\ phase' = "read"
    \* Nothing else changes.
    /\ UNCHANGED <<wrote, changed, alice, wDone, outcome>>

\* T adds one document. Adding alice first checks that the "alice" entry is
\* free; that check is validated unless the second bug is planted. Bob and a
\* nil name have entries of their own, which nobody else writes.
TWrite(doc) ==
    \* T must have run its query.
    /\ phase = "read"
    \* T remembers what it added.
    /\ wrote' = doc
    \* Only a writer of alice checks the alice entry, if that check counts.
    /\ validated' = validated \cup
         (IF doc = "alice" /\ ValidateOwnEntry THEN {"alice"} ELSE {})
    \* T is ready to commit.
    /\ phase' = "wrote"
    \* Nothing else changes.
    /\ UNCHANGED <<changed, alice, wDone, outcome>>

\* W adds alice in one autocommit step. It is refused if alice is already
\* committed; otherwise it commits, changing the "alice" entry after T began.
WAdd ==
    \* W runs once.
    /\ ~wDone
    \* W is finished either way.
    /\ wDone' = TRUE
    \* W commits only if nobody holds alice yet.
    /\ alice' = IF alice = {} THEN {"W"} ELSE alice
    \* A commit by W is a change T's commit can see.
    /\ changed' = IF alice = {} THEN changed \cup {"alice"} ELSE changed
    \* T is not involved.
    /\ UNCHANGED <<phase, wrote, validated, outcome>>

\* T commits: it aborts when a key it checks was changed by someone else
\* after it began; otherwise its document lands.
TCommit ==
    \* T must have added its document.
    /\ phase = "wrote"
    \* T is finished either way.
    /\ phase' = "done"
    \* A checked key that someone else changed aborts the commit.
    /\ outcome' = IF validated \cap changed # {} THEN "abort" ELSE "commit"
    \* A committed alice joins the holders of the alice entry.
    /\ alice' = IF validated \cap changed = {} /\ wrote = "alice"
                  THEN alice \cup {"T"} ELSE alice
    \* Nothing else changes.
    /\ UNCHANGED <<wrote, validated, changed, wDone>>

\* Any one of the steps above, in any order.
Next ==
    \/ TRead
    \/ \E doc \in Writes : TWrite(doc)
    \/ WAdd
    \/ TCommit

\* The whole behaviour: start in Init, then take steps.
Spec == Init /\ [][Next]_vars

\* Promise 1: at most one committed document holds "alice".
INV_Unique == Cardinality(alice) <= 1

\* Promise 2: T loses its commit only if T itself wrote alice.
INV_FilterReadDecidesNothing == outcome = "abort" => wrote = "alice"
====
