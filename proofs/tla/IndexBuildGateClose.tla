---- MODULE IndexBuildGateClose ----
\* The story: an index is being built in the background, a batch of
\* documents at a time. Before each batch the build passes a gate, a hook a
\* test installs to hold the build half done. While the build waits at the
\* gate, someone closes the database, and later opens it again.
\*
\* Who acts: the build (waiting at the gate, running a batch, or giving up
\* because the database is closing), the test (opening the gate, but only
\* while the database is open), and the database (starting to close,
\* finishing the close once its worker pool has stopped, and opening again).
\* The pool stops only when no build task is still running: it waits for
\* the task, it never kills it.
\*
\* What must never happen:
\*   - a close that never finishes, because the build waits at a gate that
\*     nobody opens while the database is closing;
\*   - a build that is never finished, because the close marked it failed or
\*     the next open forgot to start it again.
\*
\* Code it models:
\*   crates/db/src/index/backfill.rs  DB::backfill_batches (the gate wait
\*       raced with DB::closing; a close ends it with DatabaseClosed),
\*       DB::backfill_index (DatabaseClosed leaves the record in progress),
\*       DB::resume_index_backfills (the next open resumes it)
\*   crates/db/src/database/mod.rs  DB::close (sets closing, then stops the
\*       pool, which waits for the build task)
\*   crates/db/src/index/build_gate.rs  IndexBuildGate::enter
\* Crashes and drops during a build are in IndexDropBackfill.tla; a close
\* here leaves the build in the same state a crash does.
EXTENDS Naturals

CONSTANTS
  Docs,        \* how many documents the build must index
  RaceMode,    \* "Race": the gate wait also ends when the database closes | "NoRace": it waits for the gate only
  StopMode,    \* "Leave": a build stopped by the close keeps its record in progress | "Fail": it marks it failed
  ResumeMode   \* "Resume": opening the database restarts an unfinished build | "Forget": it does not

\* There is at least one document, so the build has a batch to run.
ASSUME Docs \in Nat \ {0}
\* Each switch takes one of its two values.
ASSUME RaceMode \in {"Race", "NoRace"}
ASSUME StopMode \in {"Leave", "Fail"}
ASSUME ResumeMode \in {"Resume", "Forget"}

\* How many times the database can close: enough for one close while the
\* build waits and one more after it resumes.
MaxCloses == 2

VARIABLES
  db,      \* the database: "Open", "Closing" (pool stopping) or "Closed"
  gate,    \* the test's gate: "Shut" holds every build, "Open" lets them through
  bf,      \* the build task (see BfStates)
  left,    \* documents the build has not indexed yet
  record,  \* the build's durable record: "InProgress", "Errored" or "None" (built)
  closes   \* how many times the database has started to close

vars == <<db, gate, bf, left, record, closes>>

\* The build task is about to reach the gate ("Run"), waits at it
\* ("Gated"), runs a batch ("Batch"), gave up because the database closed
\* ("Stopped"), finished ("Done"), or does not exist because nothing
\* started it ("Idle").
BfStates == {"Run", "Gated", "Batch", "Stopped", "Done", "Idle"}

\* A build task that is still running, which the pool waits for.
Running == bf \in {"Run", "Gated", "Batch"}

\* Every variable stays in its range.
TypeOK ==
  /\ db \in {"Open", "Closing", "Closed"}          \* the three database states
  /\ gate \in {"Shut", "Open"}                     \* the gate is shut or open
  /\ bf \in BfStates                               \* a known build step
  /\ left \in 0..Docs                              \* work left is a count of documents
  /\ record \in {"InProgress", "Errored", "None"}  \* the three record states
  /\ closes \in 0..MaxCloses                       \* closes so far

\* The index was just created: its record says in progress, its build task
\* is about to reach the gate, the gate is shut, and the database is open.
Init ==
  /\ db = "Open"              \* the database is up
  /\ gate = "Shut"            \* the test installed a shut gate
  /\ bf = "Run"               \* the build is about to reach the gate
  /\ left = Docs              \* no document is indexed yet
  /\ record = "InProgress"    \* committed with the index definition
  /\ closes = 0               \* no close yet

\* ------------------------------------------------------------------ build

\* The build reaches the gate before its next batch and waits there.
BfEnter ==
  /\ bf = "Run"                                   \* between batches
  /\ bf' = "Gated"                                \* now waiting at the gate
  /\ UNCHANGED <<db, gate, left, record, closes>>

\* The gate is open, so the wait ends and the batch runs.
BfPass ==
  /\ bf = "Gated"                                 \* waiting at the gate
  /\ gate = "Open"                                \* which lets it through
  /\ bf' = "Batch"                                \* on to the batch
  /\ UNCHANGED <<db, gate, left, record, closes>>

\* "Race": the database started to close, so the wait ends without the
\* gate. "Leave" keeps the record in progress, as a crash would; "Fail"
\* writes the close down as a failed build.
BfStop ==
  /\ RaceMode = "Race"                                          \* the wait watches the close
  /\ bf = "Gated"                                               \* waiting at the gate
  /\ db = "Closing"                                             \* and the database is closing
  /\ bf' = "Stopped"                                            \* the build task ends
  /\ record' = IF StopMode = "Leave" THEN record ELSE "Errored" \* keep or fail the record
  /\ UNCHANGED <<db, gate, left, closes>>

\* A batch indexes one more document, or, with none left, finishes the
\* build and deletes its record.
BfBatch ==
  /\ bf = "Batch"                                 \* running a batch
  /\ IF left = 0                                  \* nothing left to index
     THEN /\ bf' = "Done"                         \* the index is built
          /\ record' = "None"                     \* complete_action deletes the record
          /\ UNCHANGED left                       \* no more work
     ELSE /\ left' = left - 1                     \* one more document is indexed
          /\ bf' = "Run"                          \* on to the gate before the next batch
          /\ UNCHANGED record                     \* still in progress
  /\ UNCHANGED <<db, gate, closes>>

\* ------------------------------------------------------------------- test

\* The test opens the gate, but only while the database is open: the
\* scenario that closes the database releases the gate after it reopens.
Release ==
  /\ db = "Open"                                  \* the node is up
  /\ gate = "Shut"                                \* the gate is still shut
  /\ gate' = "Open"                               \* every build may pass now
  /\ UNCHANGED <<db, bf, left, record, closes>>

\* --------------------------------------------------------------- database

\* The database starts to close: DB::close sets closing first.
Close ==
  /\ db = "Open"                                  \* it is up
  /\ closes < MaxCloses                           \* it closes a bounded number of times
  /\ db' = "Closing"                              \* closing is now set
  /\ closes' = closes + 1                         \* one more close
  /\ UNCHANGED <<gate, bf, left, record>>

\* The worker pool stops once no build task runs, and the store closes.
PoolStop ==
  /\ db = "Closing"                               \* the close is under way
  /\ ~Running                                     \* the pool has waited for every task
  /\ db' = "Closed"                               \* the close is done
  /\ UNCHANGED <<gate, bf, left, record, closes>>

\* The database opens again. "Resume" starts a build whose record is still
\* in progress, as DB::resume_index_backfills does; "Forget" starts none.
Reopen ==
  /\ db = "Closed"                                                \* it was closed
  /\ db' = "Open"                                                 \* now it is up
  /\ bf' = IF ResumeMode = "Resume" /\ record = "InProgress"      \* an unfinished build, resumed
           THEN "Run"                                             \* starts again from its watermark
           ELSE IF bf = "Done" THEN "Done" ELSE "Idle"            \* otherwise nothing runs
  /\ UNCHANGED <<gate, left, record, closes>>

\* Any one step happens next.
Next ==
  \/ BfEnter \/ BfPass \/ BfStop \/ BfBatch
  \/ Release
  \/ Close \/ PoolStop \/ Reopen

\* Every step that can keep going eventually does. A close may happen but
\* never has to.
Fairness ==
  /\ WF_vars(BfEnter) /\ WF_vars(BfPass) /\ WF_vars(BfStop) /\ WF_vars(BfBatch)
  /\ WF_vars(Release) /\ WF_vars(PoolStop) /\ WF_vars(Reopen)

\* Start in Init and take any steps, fairly.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* The record is deleted only once every document is indexed: a close never
\* passes a half-built index off as built.
INV_BuiltOnlyWhenIndexed == record = "None" => left = 0

\* A close that starts always finishes, even with the build held at a shut
\* gate.
LIVE_CloseFinishes == (db = "Closing") ~> (db = "Closed")

\* The build always finishes, across a close while it waits at the gate.
LIVE_BuildFinishes == <>(record = "None")
====
