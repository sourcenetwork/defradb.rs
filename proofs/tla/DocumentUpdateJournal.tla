---- MODULE DocumentUpdateJournal ----
\* A node stores blocks that update documents, a journal records the blocks
\* that changed a document, and a reader pages the journal inside one read
\* transaction. Documents can be deleted while this goes on.
\* What must never happen: a block that changed its document has no row, a
\* block has two rows, a block that changed nothing has a row, a page skips a
\* row, or a reader is handed a row of a document it may not read or of one
\* its page saw as deleted. Deleting unregisters a document, and the access
\* check alone would then call it public.
\* The access provider either reads in the page's transaction, or stands
\* outside it and answers from the state of the moment it is asked.
\* Not modeled: the reader's errors for a cursor beyond the head and for a
\* missing row; a page here always finds every row it scans.
\* Rust owners: crates/db/src/event/updates.rs (record, read, rows),
\* crates/db/src/event/updates/gate.rs (Gate::decide, Gate::confirm),
\* crates/db/src/write/update.rs (write_blocks),
\* crates/db/src/merge/merge_handler/composite_persist.rs (persist_merged_document)
\* and crates/query/src/runner/arrivals.rs (execute_arrivals_query).
EXTENDS Naturals, FiniteSets, Sequences

\* Docs are the documents of one collection and Blocks the block identities;
\* Allowed holds the documents the reader may read while they are registered;
\* Limit is the most rows one page scans; Provider says where the access
\* provider reads; Mode picks the bug a RED plants.
CONSTANTS Docs, Blocks, Allowed, Limit, Provider, Mode
\* The reader's documents are documents of the collection.
ASSUME /\ Allowed \subseteq Docs
\* A page scans at least one row.
       /\ Limit \in Nat \ {0}
\* "bound" reads in the page's transaction; "outside" reads live state.
       /\ Provider \in {"bound", "outside"}
\* "safe" is the design; every other mode breaks exactly one guard.
       /\ Mode \in {"safe", "duplicate", "detached", "superseded",
                    "unauthorized", "deleted", "stale", "skip"}

\* An update is one block of one document.
Updates == Docs \X Blocks
\* The document an update belongs to.
Doc(u) == u[1]
\* The smaller of two numbers.
Min(x, y) == IF x < y THEN x ELSE y

\* stored holds every block the node keeps; changed holds the ones that
\* changed their document; log is the journal in commit order; deleted holds
\* the deleted documents; retried bounds replay to once per block.
\* seen is what the reader was handed; hidden is what a page passed over
\* because its document was deleted; leaked is what a page handed over
\* although its own snapshot held the document as deleted; cursor is the
\* reader's checkpoint.
\* open says a page's transaction is running; upto is the last row it
\* scanned; snapshot is the set of documents deleted when it began; asked
\* says the provider has answered; admitted is what it let through.
VARIABLES stored, changed, log, deleted, retried,
          seen, hidden, leaked, cursor, open, upto, snapshot, asked, admitted
\* The writer's half of the state.
writer == <<stored, changed, log, deleted, retried>>
\* What earlier pages left behind.
past == <<seen, hidden, leaked, cursor>>
\* The page being read.
page == <<open, upto, snapshot, asked, admitted>>
\* Every variable.
vars == <<stored, changed, log, deleted, retried,
          seen, hidden, leaked, cursor, open, upto, snapshot, asked, admitted>>

\* Rows is the set of updates in the journal sequence.
Rows == {log[i] : i \in 1..Len(log)}

\* A new node has stored nothing, deleted nothing, and read nothing.
Init == /\ stored = {}
        /\ changed = {}
        /\ log = <<>>
        /\ deleted = {}
        /\ retried = {}
        /\ seen = {}
        /\ hidden = {}
        /\ leaked = {}
        /\ cursor = 0
        /\ open = FALSE
        /\ upto = 0
        /\ snapshot = {}
        /\ asked = FALSE
        /\ admitted = {}

\* A local write, or a merged block that changes its document, commits the
\* block and its journal row together. A deleted document takes no update.
Commit(u) ==
  /\ u \in Updates \ stored
  /\ Doc(u) \notin deleted
  /\ stored' = stored \cup {u}
  /\ changed' = changed \cup {u}
  /\ log' = IF Mode = "detached" THEN log ELSE Append(log, u)
  /\ UNCHANGED <<deleted, retried>>
  /\ UNCHANGED <<past, page>>

\* A merged block whose every field loses to the stored value is kept, but
\* the document stays as it was, so the block gets no row. The broken variant
\* journals it anyway.
Supersede(u) ==
  /\ u \in Updates \ stored
  /\ Doc(u) \notin deleted
  /\ stored' = stored \cup {u}
  /\ log' = IF Mode = "superseded" THEN Append(log, u) ELSE log
  /\ UNCHANGED <<changed, deleted, retried>>
  /\ UNCHANGED <<past, page>>

\* Re-delivery of a journaled block is a no-op; the broken variant appends it.
Retry(u) ==
  /\ u \in changed \ retried
  /\ retried' = retried \cup {u}
  /\ log' = IF Mode = "duplicate" THEN Append(log, u) ELSE log
  /\ UNCHANGED <<stored, changed, deleted>>
  /\ UNCHANGED <<past, page>>

\* A document is deleted, and its access record goes in the same commit.
Delete(d) ==
  /\ d \in Docs \ deleted
  /\ deleted' = deleted \cup {d}
  /\ UNCHANGED <<stored, changed, log, retried>>
  /\ UNCHANGED <<past, page>>

\* A page begins: one transaction scans up to n rows after the checkpoint in
\* one pass, and sees which documents are deleted at that moment.
Scan(n) ==
  /\ ~open
  /\ cursor < Len(log)
  /\ open' = TRUE
  /\ upto' = Min(cursor + n, Len(log))
  /\ snapshot' = deleted
  /\ UNCHANGED <<asked, admitted>>
  /\ UNCHANGED past
  /\ UNCHANGED writer

\* The rows the open page scanned.
Scanned == {log[i] : i \in (cursor + 1)..upto}

\* Whether the page's own snapshot holds the update's document as deleted.
Gone(u) == Doc(u) \in snapshot

\* Whether the provider lets a scanned row through. The design asks only for
\* documents alive in the page's snapshot. A bound provider reads that same
\* snapshot. A provider outside reads the state of the moment, where a
\* document deleted since the page began is unregistered and so public.
\* "unauthorized" checks nothing. "deleted" asks the provider alone, which
\* calls a document its snapshot holds as deleted public.
Admit(u) ==
  CASE Mode = "unauthorized" -> TRUE
    [] Mode = "deleted" -> Doc(u) \in Allowed \/ Gone(u)
    [] Provider = "outside" -> ~Gone(u) /\ (Doc(u) \in Allowed \/ Doc(u) \in deleted)
    [] OTHER -> ~Gone(u) /\ Doc(u) \in Allowed

\* The provider answers for the rows the page scanned.
Ask ==
  /\ open
  /\ ~asked
  /\ asked' = TRUE
  /\ admitted' = {u \in Scanned : Admit(u)}
  /\ UNCHANGED <<open, upto, snapshot>>
  /\ UNCHANGED past
  /\ UNCHANGED writer

\* What the page returns. After a provider outside the transaction has
\* answered, the page looks again and drops every document deleted by then:
\* the provider can call a document public only after its delete committed.
\* "stale" skips that second look.
Returned ==
  IF Provider = "outside" /\ Mode # "stale"
    THEN {u \in admitted : Doc(u) \notin deleted}
    ELSE admitted

\* The page ends: it returns its rows, remembers the rows it hid for a
\* deleted document, and moves the checkpoint to the last row scanned.
\* The broken variant moves the checkpoint one row further than it scanned.
Decide ==
  /\ open
  /\ asked
  /\ seen' = seen \cup Returned
  /\ hidden' = hidden \cup {u \in Scanned : Gone(u) \/ (Provider = "outside" /\ Doc(u) \in deleted)}
  /\ leaked' = leaked \cup {u \in Returned : Gone(u)}
  /\ cursor' = IF Mode = "skip" /\ upto < Len(log) THEN upto + 1 ELSE upto
  /\ open' = FALSE
  /\ upto' = cursor'
  /\ snapshot' = {}
  /\ asked' = FALSE
  /\ admitted' = {}
  /\ UNCHANGED writer

\* Each step stores a block, replays one, deletes a document, or runs one
\* part of a page.
Next ==
  \/ \E u \in Updates : Commit(u) \/ Supersede(u) \/ Retry(u)
  \/ \E d \in Docs : Delete(d)
  \/ \E n \in 1..Limit : Scan(n)
  \/ Ask
  \/ Decide
\* The system starts in Init and then only takes those steps.
Spec == Init /\ [][Next]_vars

\* Every block that changed its document is in the journal.
INV_Atomic == changed \subseteq Rows
\* The journal never holds the same block twice.
INV_Once == Len(log) = Cardinality(Rows)
\* The journal holds no block that left its document unchanged.
INV_OnlyChanges == Rows \subseteq changed
\* No row of a document the reader may not read is returned.
INV_Authorized == \A u \in seen : Doc(u) \in Allowed
\* No page returns a row of a document its own snapshot held as deleted,
\* whoever is reading.
INV_DeletedHidden == leaked = {}
\* A checkpoint never passes the end of the journal.
INV_CursorBound == cursor <= Len(log) /\ upto <= Len(log)
\* Every readable row behind the checkpoint was returned on an earlier page,
\* unless its document was deleted when that page read it.
INV_NoSkippedVisible ==
  \A i \in 1..cursor : Doc(log[i]) \in Allowed => log[i] \in seen \cup hidden
====
