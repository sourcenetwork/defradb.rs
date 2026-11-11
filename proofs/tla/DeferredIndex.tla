---- MODULE DeferredIndex ----
\* The story: documents have a counter, and an index on it. An increment is
\* blind: it adds to the counter without reading it, so it never conflicts
\* with another increment. It cannot move the index entry either, because it
\* never learns the new value. Instead the increment appends the document to
\* the index's pending log in the same transaction, which is also blind.
\*
\* A drain fixes the index later. It plans on a snapshot: the log's head P
\* and the documents in the log up to P. Then, on its own transaction, it
\* reads each planned document's position record (where its entry is now)
\* and its live value, moves the entry, rewrites the record, and truncates
\* the log through P. Two drains can run at once.
\*
\* Until a drain catches up, a read through the index skips the stored
\* entries of owed documents (those still in the log) and checks those
\* documents' live values instead. Inside an explicit transaction, the
\* documents it appended itself count as owed too. The transaction reads the
\* position records at its snapshot without validating them, so a drain that
\* moves a record after the read does not abort the transaction.
\*
\* A reindex (after a lens migration) clears the entries but keeps the
\* records, then walks the documents one by one and writes each one's entry
\* again. Drains keep running meanwhile, so the walk reads the document's
\* record and removes the entry it names before it writes its own.
\*
\* A drop of the index removes its entries and records, then truncates what
\* is left of the log in pages, each its own transaction, each starting at
\* the first entry still there.
\*
\* Who acts: plain increments and deletes (one transaction each), one
\* explicit transaction that increments and reads, two drains, a reindex, a
\* drop, and a crash that loses whatever was not committed.
\*
\* What must never happen: a read through the index returns a document whose
\* value does not match, or misses one that does; the index holds an entry
\* the position record does not name; work is left in the log forever; a
\* drain aborts the explicit transaction; a drop truncates more than a page
\* of positions in one transaction.
\*
\* Code it models:
\*   crates/db/src/index/manager/deferred.rs  IndexManager::owe (the append),
\*                                            IndexManager::place (a create),
\*                                            IndexManager::replace (the walk's record read)
\*   crates/db/src/index/deferred/drain.rs    plan (snapshot), apply (position
\*                                            record, live value), truncate
\*   crates/db/src/index/manager/read.rs      IndexManager::read_index (owed docs,
\*                                            own appends included, records read
\*                                            unvalidated)
\*   crates/storage/src/index/overlay.rs      the stored keys of owed docs hidden,
\*                                            their live keys merged in order
\*   crates/db/src/definition/migration/reindex/page.rs  clear (entries out, records kept)
\*   crates/db/src/index/manager/rebuild.rs   IndexManager::reindex_document (the walk)
\*   crates/db/src/index/deferred/cleanup.rs  remove_page (the log truncated in pages)
EXTENDS Naturals, Integers, Sequences, FiniteSets

CONSTANTS
  Docs,        \* the documents
  Drains,      \* the drains that may run at once
  MaxSteps,    \* how many writes the check explores
  Overlay,     \* "On": reads check owed docs live | "Off": reads trust the stored entries
  OwnAppends,  \* "On": a transaction's own appends count as owed in its reads | "Off": they do not
  RecordRead,  \* "Validated": a drain's read of a position record aborts if another drain changed it | "Unvalidated"
  TxRecordRead, \* "Unvalidated": the transaction's read of a position record never aborts it | "Validated": a drain that changes it does
  Truncate,    \* "Planned": a drain truncates through its planned head | "Head": through the head at commit
  Reindex,     \* "On": a reindex may run | "Off": it never does
  WalkRecord,  \* "Delete": the walk removes the entry the record names | "Overwrite": it only writes its own
  WalkCheck,   \* "Validated": the walk's read of the record aborts if a drain changed it | "Unvalidated"
  Drop,        \* "On": the index may be dropped | "Off": it never is
  Cleanup,     \* "Paged": a drop page truncates up to Page entries from the first one left |
               \* "Whole": all that is left at once | "FromOne": up to Page past the first left, but from position 1
  Page         \* the most log entries one drop page takes

\* Each switch takes one of its values.
ASSUME Overlay \in {"On", "Off"}
ASSUME OwnAppends \in {"On", "Off"}
ASSUME RecordRead \in {"Validated", "Unvalidated"}
ASSUME TxRecordRead \in {"Validated", "Unvalidated"}
ASSUME Truncate \in {"Planned", "Head"}
ASSUME Reindex \in {"On", "Off"}
ASSUME WalkRecord \in {"Delete", "Overwrite"}
ASSUME WalkCheck \in {"Validated", "Unvalidated"}
ASSUME Drop \in {"On", "Off"}
ASSUME Cleanup \in {"Paged", "Whole", "FromOne"}
\* At least one write, or nothing happens.
ASSUME MaxSteps \in Nat \ {0}
\* A page takes at least one entry, or a drop never finishes.
ASSUME Page \in Nat \ {0}

\* No value: a deleted document, or a document with no entry.
None == -1

VARIABLES
  value,    \* each document's counter as stored
  alive,    \* whether each document exists (a delete sets it false)
  index,    \* the stored index entries: pairs of a document and a value
  rec,      \* each document's position record: the value its entry is at, or None
  log,      \* the pending log, every position ever given; position p holds log[p]
  trunc,    \* the log is truncated through this position
  dpc,      \* each drain's step: "Idle", "Planned" (has a plan) or "Read" (read its inputs)
  dP,       \* each drain's planned head
  dDocs,    \* each drain's planned documents
  dRec,     \* the position records each drain read
  dLive,    \* the live values each drain read (None for a deleted document)
  txOpen,   \* whether the explicit transaction is open
  txd,      \* the increments the open transaction made, per document
  txApp,    \* the documents the open transaction appended to the log
  sValue,   \* the counters as the transaction's snapshot saw them
  sAlive,   \* which documents existed in the transaction's snapshot
  sIndex,   \* the stored entries in the transaction's snapshot
  sOwed,    \* the owed documents in the transaction's snapshot
  sRec,     \* the position records in the transaction's snapshot
  txAborted, \* whether a drain ever aborted the transaction
  steps,    \* writes so far
  crashed,  \* whether the one crash happened
  rx,       \* the reindex: "None" (not begun), "Cleared" (entries gone, walk under way) or "Done"
  walked,   \* the documents the walk has written
  wpc,      \* the walk's step for each document: "Idle" or "Read" (read its inputs)
  wRec,     \* the record the walk read for each document
  wLive,    \* the value the walk read for each document (None for a deleted one)
  dropped,  \* whether the index was dropped
  biggest   \* the most log positions one drop page truncated

vars == <<value, alive, index, rec, log, trunc, dpc, dP, dDocs, dRec, dLive,
          txOpen, txd, txApp, sValue, sAlive, sIndex, sOwed, sRec, txAborted, steps, crashed,
          rx, walked, wpc, wRec, wLive, dropped, biggest>>

\* The values the check can reach.
Values == 0..MaxSteps

\* The larger and the smaller of two numbers.
Max(a, b) == IF a > b THEN a ELSE b
Min(a, b) == IF a < b THEN a ELSE b

\* The documents still in the log: appended, and not yet truncated.
Owed == {log[p] : p \in (trunc + 1)..Len(log)}

\* What a read through the index finds for the value q, at the current state.
Found(q) ==
  IF Overlay = "On"
  THEN {d \in Docs \ Owed : <<d, q>> \in index}                      \* stored entries of settled docs
       \cup {d \in Owed : alive[d] /\ value[d] = q}                  \* owed docs, checked live
  ELSE {d \in Docs : <<d, q>> \in index}                             \* the stored entries alone

\* What a read inside the open transaction finds for q: its snapshot, plus its own increments.
TxFound(q) ==
  LET owed == IF OwnAppends = "On"                                   \* the docs it treats as owed
              THEN sOwed \cup {txApp[i] : i \in 1..Len(txApp)}
              ELSE sOwed
  IN {d \in Docs : <<d, q>> \in sIndex                               \* stored entries in the snapshot,
                   /\ ~(d \in owed /\ sRec[d] = q)}                  \* less those the snapshot's records name for owed docs
     \cup {d \in owed : sAlive[d] /\ sValue[d] + txd[d] = q}         \* owed docs, checked live with own writes

\* Every variable stays in its range.
TypeOK ==
  /\ value \in [Docs -> Values]                       \* counters
  /\ alive \in [Docs -> BOOLEAN]                      \* existence
  /\ index \subseteq Docs \X Values                   \* entries
  /\ rec \in [Docs -> Values \cup {None}]             \* position records
  /\ trunc \in 0..Len(log)                            \* truncation stays inside the log
  /\ dpc \in [Drains -> {"Idle", "Planned", "Read"}]  \* drain steps
  /\ txOpen \in BOOLEAN                               \* the transaction is open or not
  /\ txAborted \in BOOLEAN                            \* a drain aborted it or not
  /\ steps \in 0..MaxSteps                            \* bounded exploration
  /\ rx \in {"None", "Cleared", "Done"}               \* reindex stages
  /\ walked \subseteq Docs                            \* walked documents
  /\ wpc \in [Docs -> {"Idle", "Read"}]               \* walk steps
  /\ dropped \in BOOLEAN                              \* the index is dropped or not
  /\ biggest \in Nat                                  \* a count of positions

\* Each document exists at 0 with its entry and record; nothing is owed.
Init ==
  /\ value = [d \in Docs |-> 0]                \* every counter starts at 0
  /\ alive = [d \in Docs |-> TRUE]             \* every document exists
  /\ index = {<<d, 0>> : d \in Docs}           \* each indexed at 0
  /\ rec = [d \in Docs |-> 0]                  \* each record names that entry
  /\ log = <<>>                                \* nothing appended
  /\ trunc = 0                                 \* nothing truncated
  /\ dpc = [r \in Drains |-> "Idle"]           \* no drain running
  /\ dP = [r \in Drains |-> 0]                 \* no plan
  /\ dDocs = [r \in Drains |-> {}]             \* no planned docs
  /\ dRec = [r \in Drains |-> [d \in Docs |-> None]]   \* nothing read
  /\ dLive = [r \in Drains |-> [d \in Docs |-> None]]  \* nothing read
  /\ txOpen = FALSE                            \* no transaction open
  /\ txd = [d \in Docs |-> 0]                  \* no own increments
  /\ txApp = <<>>                              \* no own appends
  /\ sValue = value /\ sAlive = alive          \* empty snapshot, unused while closed
  /\ sIndex = index /\ sOwed = {}              \* empty snapshot, unused while closed
  /\ sRec = rec                                \* empty snapshot, unused while closed
  /\ txAborted = FALSE                         \* no abort yet
  /\ steps = 0                                \* nothing written
  /\ crashed = FALSE                           \* no crash yet
  /\ rx = "None" /\ walked = {}                \* no reindex yet
  /\ wpc = [d \in Docs |-> "Idle"]             \* the walk has read nothing
  /\ wRec = [d \in Docs |-> None]              \* nothing read
  /\ wLive = [d \in Docs |-> None]             \* nothing read
  /\ dropped = FALSE                           \* the index exists
  /\ biggest = 0                               \* no drop page yet

\* The transaction state, left as is.
txVars == <<txOpen, txd, txApp, sValue, sAlive, sIndex, sOwed, sRec, txAborted>>
\* The drains' state, left as is.
drainVars == <<dpc, dP, dDocs, dRec, dLive>>
\* The reindex and drop state, left as is.
lifeVars == <<rx, walked, wpc, wRec, wLive, dropped, biggest>>

\* A plain increment: blind on the counter, blind append to the log.
Increment(d) ==
  /\ steps < MaxSteps                          \* still exploring
  /\ alive[d]                                  \* only a live document is incremented
  /\ value' = [value EXCEPT ![d] = @ + 1]      \* add without reading
  /\ log' = IF dropped THEN log ELSE Append(log, d)   \* owe the index an update, while it exists
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<alive, index, rec, trunc, crashed>> /\ UNCHANGED drainVars /\ UNCHANGED txVars
  /\ UNCHANGED lifeVars

\* A delete: the document goes, and the index is owed its removal.
Delete(d) ==
  /\ steps < MaxSteps                          \* still exploring
  /\ alive[d]                                  \* a document is deleted once
  /\ ~(txOpen /\ txd[d] > 0)                   \* a delete of a doc the open txn wrote would abort it; left out
  /\ alive' = [alive EXCEPT ![d] = FALSE]      \* the document is gone
  /\ log' = IF dropped THEN log ELSE Append(log, d)   \* owe the index the removal, while it exists
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<value, index, rec, trunc, crashed>> /\ UNCHANGED drainVars /\ UNCHANGED txVars
  /\ UNCHANGED lifeVars

\* The explicit transaction opens and takes its snapshot.
TxBegin ==
  /\ ~txOpen                                   \* one transaction at a time
  /\ steps < MaxSteps                          \* still exploring
  /\ ~dropped                                  \* a write to a dropped index conflicts with the drop; left out
  /\ rx /= "Cleared"                           \* reads in the middle of a reindex are not checked here
  /\ txOpen' = TRUE                            \* it is open
  /\ sValue' = value /\ sAlive' = alive        \* the snapshot of the documents
  /\ sIndex' = index /\ sOwed' = Owed          \* the snapshot of the index and the log
  /\ sRec' = rec                               \* the snapshot of the position records
  /\ UNCHANGED <<txd, txApp, txAborted, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED drainVars
  /\ UNCHANGED lifeVars

\* The transaction increments a document it sees.
TxIncrement(d) ==
  /\ txOpen                                    \* inside the transaction
  /\ steps < MaxSteps                          \* still exploring
  /\ sAlive[d] /\ alive[d]                     \* a live document
  /\ txd' = [txd EXCEPT ![d] = @ + 1]          \* its own increment
  /\ txApp' = Append(txApp, d)                 \* its own append, without a position yet
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<txOpen, sValue, sAlive, sIndex, sOwed, sRec, txAborted, value, alive, index, rec, log, trunc, crashed>>
  /\ UNCHANGED drainVars /\ UNCHANGED lifeVars

\* A drain moved a record the transaction's reads used since its snapshot.
RecordMoved == \E d \in sOwed : rec[d] /= sRec[d]

\* The transaction commits: its increments land and its appends take positions.
\* Had it validated the records it read, a drain that moved one aborts it instead.
TxCommit ==
  /\ txOpen                                    \* inside the transaction
  /\ IF TxRecordRead = "Validated" /\ RecordMoved
     THEN /\ txAborted' = TRUE                 \* the conflict goes back to the client
          /\ UNCHANGED <<value, log>>          \* nothing it wrote lands
     ELSE /\ value' = [d \in Docs |-> value[d] + txd[d]]   \* blind increments merge on top
          /\ log' = log \o txApp               \* its appends take the next positions
          /\ UNCHANGED txAborted
  /\ txOpen' = FALSE /\ txd' = [d \in Docs |-> 0] /\ txApp' = <<>>   \* closed
  /\ UNCHANGED <<sValue, sAlive, sIndex, sOwed, sRec, alive, index, rec, trunc, steps, crashed>>
  /\ UNCHANGED drainVars /\ UNCHANGED lifeVars

\* A drain plans on a snapshot: a head P (any, as a cap may stop short) and the docs up to it.
Plan(r) ==
  /\ dpc[r] = "Idle"                           \* not already draining
  /\ Owed /= {}                                \* something is owed (a request or the sweep)
  /\ \E p \in (trunc + 1)..Len(log) :          \* the planned head
       /\ dP' = [dP EXCEPT ![r] = p]
       /\ dDocs' = [dDocs EXCEPT ![r] = {log[i] : i \in (trunc + 1)..p}]
  /\ dpc' = [dpc EXCEPT ![r] = "Planned"]      \* the apply is next
  /\ UNCHANGED <<dRec, dLive, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED txVars
  /\ UNCHANGED lifeVars

\* The drain's apply transaction reads each planned doc's record and live value.
Read(r) ==
  /\ dpc[r] = "Planned"                        \* has a plan
  /\ dRec' = [dRec EXCEPT ![r] = rec]          \* the position records
  /\ dLive' = [dLive EXCEPT ![r] = [d \in Docs |-> IF alive[d] THEN value[d] ELSE None]]   \* live values, unvalidated
  /\ dpc' = [dpc EXCEPT ![r] = "Read"]         \* the commit is next
  /\ UNCHANGED <<dP, dDocs, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED txVars
  /\ UNCHANGED lifeVars

\* The drain's apply transaction commits, or aborts and runs again on a changed record.
Commit(r) ==
  /\ dpc[r] = "Read"                           \* has read its inputs
  /\ IF RecordRead = "Validated" /\ \E d \in dDocs[r] : rec[d] /= dRec[r][d]
     THEN /\ dpc' = [dpc EXCEPT ![r] = "Planned"]   \* another drain moved a record: run the apply again
          /\ UNCHANGED <<index, rec, trunc>>
     ELSE /\ IF dropped                        \* the index is gone: only the truncate is left
             THEN UNCHANGED <<index, rec>>
             ELSE /\ index' = (index \ {<<d, dRec[r][d]>> : d \in dDocs[r]})          \* old entries out
                              \cup {<<d, dLive[r][d]>> : d \in {e \in dDocs[r] : dLive[r][e] /= None}}   \* live entries in
                  /\ rec' = [d \in Docs |-> IF d \in dDocs[r] THEN dLive[r][d] ELSE rec[d]]   \* records follow
          /\ trunc' = LET upto == IF Truncate = "Planned" THEN dP[r] ELSE Len(log)    \* how far it truncates
                      IN Max(upto, trunc)
          /\ dpc' = [dpc EXCEPT ![r] = "Idle"]      \* done
  /\ UNCHANGED <<dP, dDocs, dRec, dLive, value, alive, log, steps, crashed>> /\ UNCHANGED txVars
  /\ UNCHANGED lifeVars

\* A crash: whatever was not committed is lost; the reopened node settles by draining.
Crash ==
  /\ ~crashed                                  \* one crash is enough to explore
  /\ crashed' = TRUE                           \* it happened
  /\ dpc' = [r \in Drains |-> "Idle"]          \* drains in flight are gone
  /\ txOpen' = FALSE /\ txd' = [d \in Docs |-> 0] /\ txApp' = <<>>   \* the open transaction is gone
  /\ wpc' = [d \in Docs |-> "Idle"]            \* a walk in flight is gone; the reindex resumes from what it wrote
  /\ UNCHANGED <<dP, dDocs, dRec, dLive, sValue, sAlive, sIndex, sOwed, sRec, txAborted, value, alive, index, rec, log, trunc, steps>>
  /\ UNCHANGED <<rx, walked, wRec, wLive, dropped, biggest>>

\* The reindex's clear: every entry goes, the records stay.
ReindexClear ==
  /\ Reindex = "On"                            \* a reindex may run
  /\ rx = "None" /\ ~dropped                   \* once, on a live index
  /\ ~txOpen                                   \* the transaction's snapshot is wholly before or after it
  /\ rx' = "Cleared"                           \* the walk is next
  /\ index' = {}                               \* the entries are gone
  /\ UNCHANGED <<value, alive, rec, log, trunc, steps, crashed, walked, wpc, wRec, wLive, dropped, biggest>>
  /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* The walk's transaction for d reads its record and its value.
WalkRead(d) ==
  /\ rx = "Cleared" /\ d \notin walked          \* the walk has not written d yet
  /\ wpc[d] = "Idle"                           \* not already read
  /\ wRec' = [wRec EXCEPT ![d] = rec[d]]       \* the record
  /\ wLive' = [wLive EXCEPT ![d] = IF alive[d] THEN value[d] ELSE None]   \* the value, if d lives
  /\ wpc' = [wpc EXCEPT ![d] = "Read"]         \* the commit is next
  /\ UNCHANGED <<value, alive, index, rec, log, trunc, steps, crashed, rx, walked, dropped, biggest>>
  /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* The walk's transaction for d commits, or runs again if a drain moved the record it read.
WalkCommit(d) ==
  /\ wpc[d] = "Read"                           \* has read its inputs
  /\ IF WalkCheck = "Validated" /\ rec[d] /= wRec[d]
     THEN /\ wpc' = [wpc EXCEPT ![d] = "Idle"]  \* a drain moved the record: read again
          /\ UNCHANGED <<index, rec, walked, rx>>
     ELSE /\ walked' = walked \cup {d}          \* d is written
          /\ rx' = IF walked \cup {d} = Docs THEN "Done" ELSE "Cleared"   \* the last one ends the reindex
          /\ wpc' = [wpc EXCEPT ![d] = "Idle"]  \* done with d
          /\ IF wLive[d] = None                 \* the walk skips a deleted document
             THEN UNCHANGED <<index, rec>>
             ELSE /\ index' = (IF WalkRecord = "Delete"   \* the entry the record names goes
                               THEN index \ {<<d, wRec[d]>>}
                               ELSE index)
                              \cup {<<d, wLive[d]>>}    \* and the walk's own comes in
                  /\ rec' = [rec EXCEPT ![d] = wLive[d]]   \* the record names it
  /\ UNCHANGED <<value, alive, log, trunc, steps, crashed, wRec, wLive, dropped, biggest>>
  /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* A drop: the definition goes, then its entries and records (their pages are not
\* modelled here, the log's are).
DropIndex ==
  /\ Drop = "On"                               \* the index may be dropped
  /\ ~dropped /\ ~txOpen /\ rx /= "Cleared"     \* once, with no writer or reindex in its way
  /\ dropped' = TRUE                           \* no write owes it any more
  /\ index' = {} /\ rec' = [d \in Docs |-> None]   \* entries and records are gone
  /\ UNCHANGED <<value, alive, log, trunc, steps, crashed, rx, walked, wpc, wRec, wLive, biggest>>
  /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* One drop page, its own transaction: it truncates from the first entry left.
CleanupPage ==
  /\ dropped /\ trunc < Len(log)               \* something is left of the log
  /\ LET from == IF Cleanup = "FromOne" THEN 1 ELSE trunc + 1           \* where its range starts
         upto == IF Cleanup = "Whole" THEN Len(log) ELSE Min(trunc + Page, Len(log))   \* where it ends
     IN /\ trunc' = upto                        \* the log is truncated through it
        /\ biggest' = Max(biggest, upto - from + 1)   \* how many positions this one transaction truncated
  /\ UNCHANGED <<value, alive, index, rec, log, steps, crashed, rx, walked, wpc, wRec, wLive, dropped>>
  /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* Any one step happens next.
Next ==
  \/ \E d \in Docs : Increment(d) \/ Delete(d) \/ TxIncrement(d)
  \/ TxBegin \/ TxCommit \/ Crash
  \/ \E r \in Drains : Plan(r) \/ Read(r) \/ Commit(r)
  \/ ReindexClear \/ DropIndex \/ CleanupPage
  \/ \E d \in Docs : WalkRead(d) \/ WalkCommit(d)

\* Drains, the transaction and the drop's cleanup keep making progress when they can.
Fairness ==
  /\ \A r \in Drains : WF_vars(Plan(r)) /\ WF_vars(Read(r)) /\ WF_vars(Commit(r))
  /\ WF_vars(TxCommit)
  /\ WF_vars(CleanupPage)

\* Start in Init, take any steps, and let drains run.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* A read through the index finds exactly the documents whose value matches,
\* while the index exists and no reindex is between its clear and its last walk.
INV_ReadExact ==
  (rx /= "Cleared" /\ ~dropped) => \A q \in Values : Found(q) = {d \in Docs : alive[d] /\ value[d] = q}

\* A read inside the transaction finds exactly what its snapshot and its own writes say.
INV_TxReadExact ==
  txOpen => \A q \in Values : TxFound(q) = {d \in Docs : sAlive[d] /\ sValue[d] + txd[d] = q}

\* Before any reindex, the index holds an entry for a document exactly where its position record says.
INV_RecordMatchesIndex ==
  rx = "None" => \A d \in Docs : {v \in Values : <<d, v>> \in index} = IF rec[d] = None THEN {} ELSE {rec[d]}

\* No entry is an orphan: each one is the one its document's record names.
INV_NoOrphanEntry ==
  \A d \in Docs : {v \in Values : <<d, v>> \in index} \subseteq IF rec[d] = None THEN {} ELSE {rec[d]}

\* A drain never aborts the explicit transaction that read through the index.
INV_TxNotAbortedByDrain == ~txAborted

\* Once nothing is owed, the stored entries are exactly the live values (on a live index, outside a reindex).
INV_SettledIndexExact ==
  (Owed = {} /\ rx /= "Cleared" /\ ~dropped) => index = {<<d, value[d]>> : d \in {e \in Docs : alive[e]}}

\* No drop page truncates more than a page of positions in its one transaction.
INV_CleanupBounded == biggest <= Page

\* Every owed document is eventually drained.
PROP_Drains == <>[](Owed = {})
====
