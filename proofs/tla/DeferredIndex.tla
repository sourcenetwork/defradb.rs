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
\* documents it appended itself count as owed too.
\*
\* Who acts: plain increments and deletes (one transaction each), one
\* explicit transaction that increments and reads, two drains, and a crash
\* that loses whatever was not committed.
\*
\* What must never happen: a read through the index returns a document whose
\* value does not match, or misses one that does; the index holds an entry
\* the position record does not name; work is left in the log forever.
\*
\* Code it models (crates/db/src/index/deferred):
\*   owe.rs     append to the pending log on every write of an indexed field
\*   drain.rs   plan (snapshot), apply (position record, live value), truncate
\*   overlay.rs the read through the index: stored entries minus owed docs,
\*              merged with the owed docs' live keys
EXTENDS Naturals, Integers, Sequences, FiniteSets

CONSTANTS
  Docs,        \* the documents
  Drains,      \* the drains that may run at once
  MaxSteps,    \* how many writes the check explores
  Overlay,     \* "On": reads check owed docs live | "Off": reads trust the stored entries
  OwnAppends,  \* "On": a transaction's own appends count as owed in its reads | "Off": they do not
  RecordRead,  \* "Validated": a drain's read of a position record aborts if another drain changed it | "Unvalidated"
  Truncate     \* "Planned": a drain truncates through its planned head | "Head": through the head at commit

\* Each switch takes one of its values.
ASSUME Overlay \in {"On", "Off"}
ASSUME OwnAppends \in {"On", "Off"}
ASSUME RecordRead \in {"Validated", "Unvalidated"}
ASSUME Truncate \in {"Planned", "Head"}
\* At least one write, or nothing happens.
ASSUME MaxSteps \in Nat \ {0}

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
  steps,    \* writes so far
  crashed   \* whether the one crash happened

vars == <<value, alive, index, rec, log, trunc, dpc, dP, dDocs, dRec, dLive,
          txOpen, txd, txApp, sValue, sAlive, sIndex, sOwed, steps, crashed>>

\* The values the check can reach.
Values == 0..MaxSteps

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
  IN {d \in Docs \ owed : <<d, q>> \in sIndex}                       \* stored entries of settled docs
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
  /\ steps \in 0..MaxSteps                            \* bounded exploration

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
  /\ steps = 0                                 \* nothing written
  /\ crashed = FALSE                           \* no crash yet

\* The transaction state, left as is.
txVars == <<txOpen, txd, txApp, sValue, sAlive, sIndex, sOwed>>
\* The drains' state, left as is.
drainVars == <<dpc, dP, dDocs, dRec, dLive>>

\* A plain increment: blind on the counter, blind append to the log.
Increment(d) ==
  /\ steps < MaxSteps                          \* still exploring
  /\ alive[d]                                  \* only a live document is incremented
  /\ value' = [value EXCEPT ![d] = @ + 1]      \* add without reading
  /\ log' = Append(log, d)                     \* owe the index an update
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<alive, index, rec, trunc, crashed>> /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* A delete: the document goes, and the index is owed its removal.
Delete(d) ==
  /\ steps < MaxSteps                          \* still exploring
  /\ alive[d]                                  \* a document is deleted once
  /\ ~(txOpen /\ txd[d] > 0)                   \* a delete of a doc the open txn wrote would abort it; left out
  /\ alive' = [alive EXCEPT ![d] = FALSE]      \* the document is gone
  /\ log' = Append(log, d)                     \* owe the index the removal
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<value, index, rec, trunc, crashed>> /\ UNCHANGED drainVars /\ UNCHANGED txVars

\* The explicit transaction opens and takes its snapshot.
TxBegin ==
  /\ ~txOpen                                   \* one transaction at a time
  /\ steps < MaxSteps                          \* still exploring
  /\ txOpen' = TRUE                            \* it is open
  /\ sValue' = value /\ sAlive' = alive        \* the snapshot of the documents
  /\ sIndex' = index /\ sOwed' = Owed          \* the snapshot of the index and the log
  /\ UNCHANGED <<txd, txApp, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED drainVars

\* The transaction increments a document it sees.
TxIncrement(d) ==
  /\ txOpen                                    \* inside the transaction
  /\ steps < MaxSteps                          \* still exploring
  /\ sAlive[d] /\ alive[d]                     \* a live document
  /\ txd' = [txd EXCEPT ![d] = @ + 1]          \* its own increment
  /\ txApp' = Append(txApp, d)                 \* its own append, without a position yet
  /\ steps' = steps + 1                        \* one more write
  /\ UNCHANGED <<txOpen, sValue, sAlive, sIndex, sOwed, value, alive, index, rec, log, trunc, crashed>>
  /\ UNCHANGED drainVars

\* The transaction commits: its increments land and its appends take positions.
TxCommit ==
  /\ txOpen                                    \* inside the transaction
  /\ value' = [d \in Docs |-> value[d] + txd[d]]   \* blind increments merge on top
  /\ log' = log \o txApp                       \* its appends take the next positions
  /\ txOpen' = FALSE /\ txd' = [d \in Docs |-> 0] /\ txApp' = <<>>   \* closed
  /\ UNCHANGED <<sValue, sAlive, sIndex, sOwed, alive, index, rec, trunc, steps, crashed>>
  /\ UNCHANGED drainVars

\* A drain plans on a snapshot: a head P (any, as a cap may stop short) and the docs up to it.
Plan(r) ==
  /\ dpc[r] = "Idle"                           \* not already draining
  /\ Owed /= {}                                \* something is owed (a request or the sweep)
  /\ \E p \in (trunc + 1)..Len(log) :          \* the planned head
       /\ dP' = [dP EXCEPT ![r] = p]
       /\ dDocs' = [dDocs EXCEPT ![r] = {log[i] : i \in (trunc + 1)..p}]
  /\ dpc' = [dpc EXCEPT ![r] = "Planned"]      \* the apply is next
  /\ UNCHANGED <<dRec, dLive, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED txVars

\* The drain's apply transaction reads each planned doc's record and live value.
Read(r) ==
  /\ dpc[r] = "Planned"                        \* has a plan
  /\ dRec' = [dRec EXCEPT ![r] = rec]          \* the position records
  /\ dLive' = [dLive EXCEPT ![r] = [d \in Docs |-> IF alive[d] THEN value[d] ELSE None]]   \* live values, unvalidated
  /\ dpc' = [dpc EXCEPT ![r] = "Read"]         \* the commit is next
  /\ UNCHANGED <<dP, dDocs, value, alive, index, rec, log, trunc, steps, crashed>> /\ UNCHANGED txVars

\* The drain's apply transaction commits, or aborts and runs again on a changed record.
Commit(r) ==
  /\ dpc[r] = "Read"                           \* has read its inputs
  /\ IF RecordRead = "Validated" /\ \E d \in dDocs[r] : rec[d] /= dRec[r][d]
     THEN /\ dpc' = [dpc EXCEPT ![r] = "Planned"]   \* another drain moved a record: run the apply again
          /\ UNCHANGED <<index, rec, trunc>>
     ELSE /\ index' = (index \ {<<d, dRec[r][d]>> : d \in dDocs[r]})          \* old entries out
                      \cup {<<d, dLive[r][d]>> : d \in {e \in dDocs[r] : dLive[r][e] /= None}}   \* live entries in
          /\ rec' = [d \in Docs |-> IF d \in dDocs[r] THEN dLive[r][d] ELSE rec[d]]   \* records follow
          /\ trunc' = LET upto == IF Truncate = "Planned" THEN dP[r] ELSE Len(log)    \* how far it truncates
                      IN IF upto > trunc THEN upto ELSE trunc
          /\ dpc' = [dpc EXCEPT ![r] = "Idle"]      \* done
  /\ UNCHANGED <<dP, dDocs, dRec, dLive, value, alive, log, steps, crashed>> /\ UNCHANGED txVars

\* A crash: whatever was not committed is lost; the reopened node settles by draining.
Crash ==
  /\ ~crashed                                  \* one crash is enough to explore
  /\ crashed' = TRUE
  /\ dpc' = [r \in Drains |-> "Idle"]          \* drains in flight are gone
  /\ txOpen' = FALSE /\ txd' = [d \in Docs |-> 0] /\ txApp' = <<>>   \* the open transaction is gone
  /\ UNCHANGED <<dP, dDocs, dRec, dLive, sValue, sAlive, sIndex, sOwed, value, alive, index, rec, log, trunc, steps>>

\* Any one step happens next.
Next ==
  \/ \E d \in Docs : Increment(d) \/ Delete(d) \/ TxIncrement(d)
  \/ TxBegin \/ TxCommit \/ Crash
  \/ \E r \in Drains : Plan(r) \/ Read(r) \/ Commit(r)

\* Drains and the transaction keep making progress when they can.
Fairness ==
  /\ \A r \in Drains : WF_vars(Plan(r)) /\ WF_vars(Read(r)) /\ WF_vars(Commit(r))
  /\ WF_vars(TxCommit)

\* Start in Init, take any steps, and let drains run.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* A read through the index finds exactly the documents whose value matches.
INV_ReadExact == \A q \in Values : Found(q) = {d \in Docs : alive[d] /\ value[d] = q}

\* A read inside the transaction finds exactly what its snapshot and its own writes say.
INV_TxReadExact ==
  txOpen => \A q \in Values : TxFound(q) = {d \in Docs : sAlive[d] /\ sValue[d] + txd[d] = q}

\* The index holds an entry for a document only where its position record says.
INV_RecordMatchesIndex ==
  \A d \in Docs : {v \in Values : <<d, v>> \in index} = IF rec[d] = None THEN {} ELSE {rec[d]}

\* Once nothing is owed, the stored entries are exactly the live values.
INV_SettledIndexExact ==
  Owed = {} => index = {<<d, value[d]>> : d \in {e \in Docs : alive[e]}}

\* Every owed document is eventually drained.
PROP_Drains == <>[](Owed = {})
====
