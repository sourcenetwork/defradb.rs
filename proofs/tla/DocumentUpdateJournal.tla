---- MODULE DocumentUpdateJournal ----
\* A node commits document updates, a journal records their block identities,
\* and an authorized reader pages the committed journal. A replay of one block
\* must not create another entry, a commit must not lose its journal entry,
\* and a reader must not receive an unauthorized document's entry.
\* Rust owners: crates/db/src/event/updates.rs and update/merge transactions;
\* crates/query/src/runner/arrivals.rs supplies the document authorization seam.
EXTENDS Naturals, FiniteSets, Sequences

\* Each update denotes one collection/document/block identity; Allowed is the
\* reader's document-level authorization at the time of its read.
CONSTANTS Updates, Allowed, Mode
ASSUME /\ Allowed \subseteq Updates
       /\ Mode \in {"safe", "duplicate", "detached", "unauthorized", "skip"}

\* committed holds durable updates; log is their commit-ordered journal; seen
\* is what the reader was handed; cursor is the last scanned position;
\* retried bounds replay to once per identity.
VARIABLES committed, log, seen, cursor, retried
vars == <<committed, log, seen, cursor, retried>>

\* Rows is the set of identities in the journal sequence.
Rows == {log[i] : i \in 1..Len(log)}

\* A new node has no committed updates, journal rows, reads, or retries.
Init == /\ committed = {}
        /\ log = <<>>
        /\ seen = {}
        /\ cursor = 0
        /\ retried = {}

\* A successful transaction commits its update and its journal row together.
Commit(u) ==
  /\ u \in Updates \ committed
  /\ committed' = committed \cup {u}
  /\ log' = IF Mode = "detached" THEN log ELSE Append(log, u)
  /\ UNCHANGED <<seen, cursor, retried>>

\* Re-delivery of the same block is a no-op; the broken variant appends it.
Retry(u) ==
  /\ u \in committed \ retried
  /\ retried' = retried \cup {u}
  /\ log' = IF Mode = "duplicate" THEN Append(log, u) ELSE log
  /\ UNCHANGED <<committed, seen, cursor>>

\* One bounded page scans one row, advances, and returns only authorized rows.
Read ==
  /\ cursor < Len(log)
  /\ cursor' = IF Mode = "skip" /\ cursor + 1 < Len(log)
                  THEN cursor + 2 ELSE cursor + 1
  /\ seen' = seen \cup (IF Mode = "unauthorized" \/ log[cursor + 1] \in Allowed
                           THEN {log[cursor + 1]} ELSE {})
  /\ UNCHANGED <<committed, log, retried>>

\* Each step commits, retries, or reads one page.
Next == (\E u \in Updates : Commit(u) \/ Retry(u)) \/ Read
Spec == Init /\ [][Next]_vars

\* Every durable update is already represented in the journal.
INV_Atomic == committed \subseteq Rows
\* The journal never contains the same update twice.
INV_Once == Len(log) = Cardinality(Rows)
\* No unauthorized update is returned to the reader.
INV_Authorized == seen \subseteq Allowed
\* A checkpoint never skips a row outside the snapshot already available.
INV_CursorBound == cursor <= Len(log)
\* Every visible row behind the checkpoint was returned on an earlier page.
INV_NoSkippedVisible == \A i \in 1..cursor : log[i] \in Allowed => log[i] \in seen
====
