---- MODULE TruncateUnderWrites ----
\* The story: a client truncates a collection while other clients keep
\* creating and updating documents in it. Truncate first lists the documents
\* that are committed when it starts, then deletes them. Each delete runs in a
\* transaction that regolith retries when it conflicts, up to a fixed number
\* of attempts.
\*
\* What truncate must mean under concurrent writes: every document committed
\* before it started is gone when it finishes, and a document created after it
\* started may stay, whole.
\*
\* Who acts:
\*   a writer   updates a listed document that still exists, or creates a new
\*              one after the truncate has listed what it deletes;
\*   truncate   begins an attempt over some listed documents, reading their
\*              heads, then commits it; it ends with a collection-level step.
\*
\* What must never happen:
\*   a listed document survives the truncate;
\*   a document created after the truncate began is damaged by it;
\*   a delete commits over an update it never read, leaving that update's
\*   blocks and heads behind with no document;
\*   the truncate fails because its retries ran out while writers kept writing.
\*
\* A writer updates each listed document at most once during the truncate.
\* That bound keeps the model finite and stands for writers that do not
\* hammer one document forever.
\*
\* Code it models (crates/db/src/collection/ops/truncate/):
\*   all.rs DB::truncate_all                   the listing, then one attempt per document
\*   all.rs DB::collect_doc_short_ids           the listing, from one snapshot
\*   all.rs DB::truncate_doc, truncate_doc_in   one document's delete in transact_db
\*   all.rs DB::truncate_metadata_in            the collection-level step, without a prefix wipe
\*   filtered.rs DB::truncate_filtered_chunk    the same one-attempt-per-document rule for a filter
\*   crates/db/src/txn/attempt.rs DB::transact_db  the engine-retried attempt
EXTENDS Naturals, FiniteSets   \* numbers for attempts, and sets of documents

CONSTANTS
  Listed,       \* the documents committed when the truncate starts
  Fresh,        \* the documents writers may create after it started
  MaxAttempts,  \* how many attempts one transaction gets before it gives up
  Enforced      \* which guards the code really has

\* The two groups of documents are different documents.
ASSUME Listed \cap Fresh = {}
\* Every transaction gets at least one attempt.
ASSUME MaxAttempts >= 1
\* Only these three guards exist.
ASSUME Enforced \subseteq {"perdoc", "keepfresh", "validated"}

VARIABLES
  present,    \* the documents whose data exists right now
  created,    \* the fresh documents a writer has created
  version,    \* for each document, how many updates have committed to it
  updatesLeft,\* for each listed document, whether a writer may still update it
  pending,    \* the listed documents the truncate has yet to delete
  scope,      \* the documents the running transaction covers, empty between them
  reading,    \* the running attempt has read and not yet committed
  readAt,     \* for each document in scope, the version the attempt read
  attempts,   \* how many attempts the running transaction has used
  orphans,    \* documents deleted over an update the delete never read
  phase,      \* "delete", then "meta", then "done", or "failed"
  failed      \* the truncate ran out of attempts

\* Every variable, for saying a step leaves them all alone.
vars == <<present, created, version, updatesLeft, pending, scope, reading,
          readAt, attempts, orphans, phase, failed>>

\* Every document this model knows of.
AllDocs == Listed \cup Fresh

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ present \subseteq AllDocs
  /\ created \subseteq Fresh
  /\ version \in [AllDocs -> Nat]
  /\ updatesLeft \in [Listed -> BOOLEAN]
  /\ pending \subseteq Listed
  /\ scope \subseteq Listed
  /\ reading \in BOOLEAN
  /\ readAt \in [Listed -> Nat]
  /\ attempts \in Nat
  /\ orphans \subseteq Listed
  /\ phase \in {"delete", "meta", "done", "failed"}
  /\ failed \in BOOLEAN

\* The truncate has listed the committed documents; no fresh one exists yet.
Init ==
  /\ present = Listed
  /\ created = {}
  /\ version = [d \in AllDocs |-> 0]
  /\ updatesLeft = [d \in Listed |-> TRUE]
  /\ pending = Listed
  /\ scope = {}
  /\ reading = FALSE
  /\ readAt = [d \in Listed |-> 0]
  /\ attempts = 0
  /\ orphans = {}
  /\ phase = "delete"
  /\ failed = FALSE

\* A writer updates a listed document that still exists. An update of a
\* deleted document finds nothing and changes nothing, so it is not a step.
Update(d) ==
  /\ d \in present
  /\ updatesLeft[d]
  /\ version' = [version EXCEPT ![d] = @ + 1]
  /\ updatesLeft' = [updatesLeft EXCEPT ![d] = FALSE]
  /\ UNCHANGED <<present, created, pending, scope, reading, readAt, attempts,
                 orphans, phase, failed>>

\* A writer creates a fresh document after the truncate started.
Create(f) ==
  /\ f \notin created
  /\ created' = created \cup {f}
  /\ present' = present \cup {f}
  /\ UNCHANGED <<version, updatesLeft, pending, scope, reading, readAt,
                 attempts, orphans, phase, failed>>

\* The documents the next attempt covers: one listed document when each gets
\* its own attempt, or every one left otherwise (the bug a RED config plants).
NextScopes ==
  IF "perdoc" \in Enforced THEN {{d} : d \in pending} ELSE {pending}

\* Truncate begins an attempt: it reads the heads of the documents it covers,
\* which is the version each one is at now. A new transaction picks its
\* documents; a retry runs again over the same ones.
Begin ==
  /\ phase = "delete"
  /\ ~reading
  /\ pending # {}
  /\ \E s \in IF scope = {} THEN NextScopes ELSE {scope} :
       /\ scope' = s
       /\ readAt' = [d \in Listed |-> IF d \in s THEN version[d] ELSE readAt[d]]
  /\ attempts' = IF scope = {} THEN 1 ELSE attempts + 1
  /\ reading' = TRUE
  /\ UNCHANGED <<present, created, version, updatesLeft, pending, orphans,
                 phase, failed>>

\* The documents in scope that an update reached after the attempt read them.
Moved == {d \in scope : version[d] # readAt[d]}

\* The attempt commits. Its deletes are validated, so an update that landed
\* after its read makes it lose; the engine then runs it again, or gives up
\* once the attempts are spent. Without validation the delete commits anyway
\* and leaves the update's blocks behind (the bug a RED config plants).
Commit ==
  /\ phase = "delete"
  /\ reading
  /\ reading' = FALSE
  /\ IF "validated" \in Enforced /\ Moved # {}
       THEN /\ IF attempts >= MaxAttempts
                 THEN /\ phase' = "failed" /\ failed' = TRUE
                 ELSE /\ UNCHANGED <<phase, failed>>
            /\ UNCHANGED <<present, pending, orphans, attempts, scope>>
       ELSE /\ present' = present \ scope
            /\ pending' = pending \ scope
            /\ orphans' = orphans \cup Moved
            /\ scope' = {}
            /\ attempts' = 0
            /\ phase' = IF pending \ scope = {} THEN "meta" ELSE "delete"
            /\ UNCHANGED failed
  /\ UNCHANGED <<created, version, updatesLeft, readAt>>

\* The collection-level step. Keeping fresh documents, it touches no document
\* data; a prefix wipe of the collection's document keys also takes the fresh
\* ones (the bug a RED config plants).
Meta ==
  /\ phase = "meta"
  /\ present' = IF "keepfresh" \in Enforced THEN present ELSE present \ Fresh
  /\ phase' = "done"
  /\ UNCHANGED <<created, version, updatesLeft, pending, scope, reading,
                 readAt, attempts, orphans, failed>>

\* Each step is a writer's or the truncate's.
Next ==
  \/ \E d \in Listed : Update(d)
  \/ \E f \in Fresh : Create(f)
  \/ Begin
  \/ Commit
  \/ Meta

\* Once the truncate has finished or failed, the writers may go on, which
\* the steps above allow; with nothing left to do the model may stop.
Spec == Init /\ [][Next]_vars

\* A finished truncate left no listed document.
INV_ListedGone == phase = "done" => present \cap Listed = {}

\* A finished truncate left every fresh document a writer created.
INV_FreshIntact == phase = "done" => created \subseteq present

\* No delete committed over an update it never read.
INV_NoOrphans == orphans = {}

\* The truncate never runs out of attempts while writers keep writing.
INV_NoExhaustion == ~failed
====
