---- MODULE IndexDropBackfill ----
\* The story: someone creates a secondary index on a collection that already
\* holds documents. The node writes the index into the collection's
\* definition together with a "backfill in progress" record, then a backfill
\* walks the old documents a page at a time and writes an index entry for
\* each. While that backfill runs, someone drops the index.
\*
\* Who acts: the backfill (reading the definition, committing a page,
\* failing for some unrelated reason, or finishing), the drop (taking the
\* index out of the definition, then deleting its entries a page at a time,
\* or failing on a page), someone who creates a new index once the drop has
\* committed, and the process itself, which can crash and restart, at most
\* MaxStarts times. A failed drop page is a passing storage error: the next
\* start retries it.
\*
\* What must never happen:
\*   - the dropped index keeps a backfill record, so the action list shows a
\*     failed (or forever running) backfill for an index that is gone;
\*   - the drop says it is done while some of the index's entries are still
\*     on disk;
\*   - a drop that committed never finishes deleting its entries, even
\*     across a restart;
\*   - the drop deletes an entry of the new index, because the new index
\*     got the dropped one's id and so shares its keys.
\*
\* Every transaction reads the definition key as a validated read: if the
\* definition changes between the read and the commit, the commit conflicts
\* and the attempt runs again. That is regolith's rule for an Ordinary key,
\* and the model leans on it rather than adding any lock.
\*
\* Code it models (crates/db/src/index):
\*   drop.rs      DB::drop_index, DropIndexAttempt (definition, backfill record,
\*                drop record and the retired id in one commit), DropPage
\*                (entries in pages), DB::remove_dropped_entries (a failed page
\*                keeps the record), DB::resume_index_drops (restart)
\*   manager/mod.rs  IndexManager::next_index_id and index_id_floor (a new
\*                index never gets a retired id)
\*   backfill.rs  BackfillPage::index (validated definition read per page),
\*                DB::backfill_index, FailBackfillAttempt (a failure is recorded
\*                only while the index is still defined),
\*                DB::resume_index_backfills (restart)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Docs,        \* the documents that existed before the index, which the backfill must index
  RecordMode,  \* "Clear": the drop deletes the backfill record in its commit | "Keep": it leaves it
  FailMode,    \* "Checked": a failure is written only if the index is still defined | "Blind": always
  EntryMode,   \* "AfterDefinition": entries go in pages after the definition commits | "SameTxn": in the drop's own commit
  ResumeMode,  \* "Resume": a restart continues an unfinished drop | "Forget": it does not
  IdMode       \* "Retire": the drop retires its index's id | "Reuse": a new index can get it again

\* There is at least one old document, or there is nothing to backfill.
ASSUME Docs # {}
\* Each switch takes one of its two values.
ASSUME RecordMode \in {"Clear", "Keep"}
ASSUME FailMode \in {"Checked", "Blind"}
ASSUME EntryMode \in {"AfterDefinition", "SameTxn"}
ASSUME ResumeMode \in {"Resume", "Forget"}
ASSUME IdMode \in {"Retire", "Reuse"}

\* How many times the process can restart: enough for a crash and a retry
\* after a failed page.
MaxStarts == 2

VARIABLES
  defined,    \* the index is in the collection's definition
  version,    \* how many times the definition changed: what a validated read compares
  entries,    \* the old documents that have an index entry on disk
  record,     \* the backfill's durable record: "None", "InProgress" or "Errored"
  bfPc,       \* where the backfill is (see BfStates)
  bfLeft,     \* old documents the backfill has not indexed yet
  bfVersion,  \* the definition version the backfill's open transaction read
  bfDefined,  \* whether that read found the index
  dropPc,     \* where the drop is (see DropStates)
  dropSeen,   \* the entries the drop's open transaction saw, in "SameTxn" mode
  dropRec,    \* the drop's durable "drop in progress" record
  starts,     \* how many times the process has restarted
  newId,      \* the id of the index created after the drop: "None", "Same" or "Fresh"
  newEntries  \* the old documents that have an entry in that new index

vars == <<defined, version, entries, record, bfPc, bfLeft, bfVersion,
          bfDefined, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* The variables the new index adds, which the backfill and drop steps that
\* do not touch it leave alone.
newVars == <<newId, newEntries>>

\* The backfill runs ("Run": about to read), holds an open page ("Page"),
\* is settling a failure ("Fail": about to read, "FailCommit": holding the
\* read), or has ended ("Done": finished, "Stopped": gave up).
BfStates == {"Run", "Page", "Fail", "FailCommit", "Done", "Stopped"}

\* The drop has not started ("Idle"), holds its open transaction
\* ("Commit"), is deleting entries ("Removing"), stopped on a failed page
\* with its record kept for the next start ("Failed"), lost its work in a
\* crash ("Crashed"), finished ("Done"), or never committed before a crash
\* ("Abandoned": the caller saw an error and nothing changed).
DropStates == {"Idle", "Commit", "Removing", "Failed", "Crashed", "Done", "Abandoned"}

\* The drop's commit landed, whatever happened to it after.
DropCommitted == dropPc \in {"Removing", "Failed", "Crashed", "Done"}

\* The keys the drop deletes: the dropped index's entries, and the new
\* index's too when it got the same id, since a key names only the id.
DropKeys == entries \cup (IF newId = "Same" THEN newEntries ELSE {})

\* Every variable stays in its range.
TypeOK ==
  /\ defined \in BOOLEAN                                \* in the definition or not
  /\ version \in Nat                                    \* a change counter
  /\ entries \subseteq Docs                             \* entries are for old documents
  /\ record \in {"None", "InProgress", "Errored"}       \* the three record states
  /\ bfPc \in BfStates                                  \* a known backfill step
  /\ bfLeft \subseteq Docs                              \* work left is old documents
  /\ bfVersion \in Nat                                  \* a version that was read
  /\ bfDefined \in BOOLEAN                              \* what that read found
  /\ dropPc \in DropStates                              \* a known drop step
  /\ dropSeen \subseteq Docs                            \* entries a drop read saw
  /\ dropRec \in BOOLEAN                                \* the drop record exists or not
  /\ starts \in 0..MaxStarts                           \* restarts so far
  /\ newId \in {"None", "Same", "Fresh"}                \* no new index, or which id it got
  /\ newEntries \subseteq Docs                          \* its entries are for old documents

\* The index was just created: it is defined, its backfill record says
\* "in progress", no old document has an entry yet, and no drop has started.
Init ==
  /\ defined = TRUE              \* the create committed the definition
  /\ version = 0                 \* nothing has changed it since
  /\ entries = {}                \* no old document is indexed yet
  /\ record = "InProgress"       \* committed with the definition
  /\ bfPc = "Run"                \* the backfill is about to read its first page
  /\ bfLeft = Docs               \* every old document still needs an entry
  /\ bfVersion = 0               \* no read yet
  /\ bfDefined = TRUE            \* no read yet
  /\ dropPc = "Idle"             \* nobody has asked to drop
  /\ dropSeen = {}               \* no drop read yet
  /\ dropRec = FALSE             \* no drop record
  /\ starts = 0                  \* the process has not restarted
  /\ newId = "None"              \* no new index yet
  /\ newEntries = {}             \* so no entry for it

\* ---------------------------------------------------------------- backfill

\* The backfill opens a page transaction and reads the definition (a
\* validated read) to see whether its index is still there.
BfRead ==
  /\ bfPc = "Run"                                    \* between pages
  /\ bfVersion' = version                            \* remember what it read
  /\ bfDefined' = defined                            \* and whether the index was there
  /\ bfPc' = "Page"                                  \* now holding the open page
  /\ UNCHANGED <<defined, version, entries, record, bfLeft, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* The page tries to commit. A definition that changed since the read is a
\* conflict, and the page runs again. An index that was already gone stops
\* the backfill through the failure path. With nothing left, the backfill
\* finishes and deletes its record. Otherwise it indexes one more document.
BfCommit ==
  /\ bfPc = "Page"                                                  \* holding a page
  /\ IF bfVersion # version                                         \* the definition moved
     THEN /\ bfPc' = "Run"                                          \* conflict: read again
          /\ UNCHANGED <<entries, record, bfLeft>>                  \* nothing committed
     ELSE IF ~bfDefined                                             \* the read found no index
     THEN /\ bfPc' = "Fail"                                         \* stop through the failure path
          /\ UNCHANGED <<entries, record, bfLeft>>                  \* nothing committed
     ELSE IF bfLeft = {}                                            \* every old document is indexed
     THEN /\ bfPc' = "Done"                                         \* the index is built
          /\ record' = "None"                                       \* complete_action deletes the record
          /\ UNCHANGED <<entries, bfLeft>>                          \* no new entry
     ELSE \E d \in bfLeft :                                         \* pick the next old document
          /\ entries' = entries \cup {d}                            \* its entry commits
          /\ bfLeft' = bfLeft \ {d}                                 \* and it is done
          /\ bfPc' = "Run"                                          \* on to the next page
          /\ UNCHANGED record                                       \* the record still says in progress
  /\ UNCHANGED <<defined, version, bfVersion, bfDefined, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* A page fails for a reason that has nothing to do with the drop (a
\* storage error, a lens transform error). The backfill goes to the
\* failure path.
BfError ==
  /\ bfPc = "Run"                                    \* between pages
  /\ bfPc' = "Fail"                                  \* settle the failure
  /\ UNCHANGED <<defined, version, entries, record, bfLeft, bfVersion, bfDefined, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* "Checked": the failure is settled in a transaction of its own that first
\* reads the definition. "Blind": the failure is written at once.
BfFailRead ==
  /\ bfPc = "Fail"                                                  \* a failure to settle
  /\ IF FailMode = "Checked"
     THEN /\ bfVersion' = version                                   \* read the definition
          /\ bfDefined' = defined                                   \* and see if the index is there
          /\ bfPc' = "FailCommit"                                   \* hold the read
          /\ UNCHANGED record                                       \* nothing written yet
     ELSE /\ record' = "Errored"                                    \* write the failure, no questions
          /\ bfPc' = "Stopped"                                      \* the backfill is over
          /\ UNCHANGED <<bfVersion, bfDefined>>                     \* no read
  /\ UNCHANGED <<defined, version, entries, bfLeft, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* The checked failure commits. A definition that changed is a conflict and
\* the failure is read again. An index that is still defined gets the
\* failure on its record; a dropped one gets nothing, because its record
\* went with the drop.
BfFailCommit ==
  /\ bfPc = "FailCommit"                                            \* holding the read
  /\ IF bfVersion # version                                         \* the definition moved
     THEN /\ bfPc' = "Fail"                                         \* conflict: read again
          /\ UNCHANGED record                                       \* nothing committed
     ELSE /\ record' = IF bfDefined THEN "Errored" ELSE record      \* only a live index fails
          /\ bfPc' = "Stopped"                                      \* the backfill is over
  /\ UNCHANGED <<defined, version, entries, bfLeft, bfVersion, bfDefined, dropPc, dropSeen, dropRec, starts, newId, newEntries>>

\* -------------------------------------------------------------------- drop

\* The drop opens its transaction and reads the definition. In "SameTxn"
\* mode it also lists the index's entries, with a plain scan that nothing
\* validates.
DropRead ==
  /\ dropPc = "Idle"                                 \* the drop request arrives
  /\ defined                                         \* the index is there to drop
  /\ dropSeen' = entries                             \* what a scan sees now
  /\ dropPc' = "Commit"                              \* hold the open transaction
  /\ UNCHANGED <<defined, version, entries, record, bfPc, bfLeft, bfVersion, bfDefined, dropRec, starts, newId, newEntries>>

\* The drop commits: the index leaves the definition, which bumps its
\* version. "Clear" deletes the backfill record in the same commit.
\* "AfterDefinition" writes the drop record and deletes entries later;
\* "SameTxn" deletes the entries its scan saw and is done.
DropCommit ==
  /\ dropPc = "Commit"                                              \* holding the transaction
  /\ defined' = FALSE                                               \* the index is gone
  /\ version' = version + 1                                         \* the definition changed
  /\ record' = IF RecordMode = "Clear" THEN "None" ELSE record      \* the backfill record goes, or stays
  /\ IF EntryMode = "SameTxn"
     THEN /\ entries' = entries \ dropSeen                          \* only what the scan saw
          /\ dropPc' = "Done"                                       \* the drop reports success
          /\ UNCHANGED dropRec                                      \* no drop record
     ELSE /\ dropRec' = TRUE                                        \* remember the unfinished drop
          /\ dropPc' = "Removing"                                   \* delete entries next
          /\ UNCHANGED entries                                      \* none deleted yet
  /\ UNCHANGED <<bfPc, bfLeft, bfVersion, bfDefined, dropSeen, starts, newId, newEntries>>

\* One page of the drop deletes a key under the dropped index's id. Nothing
\* can add an entry for the dropped index any more: every writer and every
\* backfill page read the old definition, and the drop's commit changed it,
\* so they conflict and see the index gone. A new index with the same id
\* shares those keys, so the page deletes its entries too.
DropRemove ==
  /\ dropPc = "Removing"                             \* deleting entries
  /\ \E d \in DropKeys :                             \* one key under the id goes
       /\ entries' = entries \ {d}                   \* the dropped index's entry
       /\ newEntries' = IF newId = "Same"            \* and, sharing the key,
                        THEN newEntries \ {d}        \* the new index's entry
                        ELSE newEntries              \* or nothing of it
  /\ UNCHANGED <<defined, version, record, bfPc, bfLeft, bfVersion, bfDefined, dropPc, dropSeen, dropRec, starts, newId>>

\* A page fails (a storage error). The drop stops and writes the failure on
\* its record, keeping the record and the entries left, so the next start
\* picks it up again. The error passes: it only happens while a start is left.
DropFail ==
  /\ dropPc = "Removing"                             \* deleting entries
  /\ starts < MaxStarts                              \* a later start can retry it
  /\ dropPc' = "Failed"                              \* the drop stops for now
  /\ UNCHANGED <<defined, version, entries, record, bfPc, bfLeft, bfVersion, bfDefined, dropSeen, dropRec, starts, newId, newEntries>>

\* No key is left under the id: the drop deletes its record and is done.
DropFinish ==
  /\ dropPc = "Removing"                             \* deleting entries
  /\ DropKeys = {}                                   \* none left
  /\ dropRec' = FALSE                                \* complete_action deletes the drop record
  /\ dropPc' = "Done"                                \* the drop is over
  /\ UNCHANGED <<defined, version, entries, record, bfPc, bfLeft, bfVersion, bfDefined, dropSeen, starts, newId, newEntries>>

\* --------------------------------------------------------------- new index

\* Once the drop has committed, someone creates a new index on the same
\* collection and its backfill indexes every old document. "Retire": the drop
\* retired the old id, so the new index gets a fresh one. "Reuse": the new
\* index gets the dropped one's id again.
ReCreate ==
  /\ DropCommitted                                   \* the old index is gone
  /\ newId = "None"                                  \* only one new index
  /\ newId' = IF IdMode = "Retire" THEN "Fresh" ELSE "Same"   \* which id it gets
  /\ newEntries' = Docs                              \* its backfill indexed every old document
  /\ UNCHANGED <<defined, version, entries, record, bfPc, bfLeft, bfVersion, bfDefined, dropPc, dropSeen, dropRec, starts>>

\* ------------------------------------------------------------------- crash

\* The process crashes or is restarted. Open transactions are lost; only
\* what committed survives. On restart, a backfill record in progress
\* resumes its backfill if the index is defined and is deleted if not; an
\* errored record is deleted if its index is gone. A drop record, failed or
\* not, resumes the drop under "Resume" and is ignored under "Forget".
Restart ==
  /\ starts < MaxStarts                                             \* a bounded number of restarts
  /\ starts' = starts + 1                                           \* one more now
  /\ bfPc' = CASE bfPc \in {"Done", "Stopped"} -> bfPc              \* an ended backfill stays ended
               [] record = "InProgress" /\ defined -> "Run"         \* resumed from its record
               [] OTHER -> "Stopped"                                \* nothing to resume
  /\ record' = IF ~defined THEN "None" ELSE record                  \* restart clears records of gone indexes
  /\ dropPc' = CASE dropPc \in {"Idle", "Done"} -> dropPc           \* untouched
                [] dropPc = "Commit" -> "Abandoned"                 \* never committed
                [] dropRec /\ ResumeMode = "Resume" -> "Removing"   \* resumed from its record
                [] OTHER -> "Crashed"                               \* left unfinished
  /\ UNCHANGED <<defined, version, entries, bfLeft, bfVersion, bfDefined, dropSeen, dropRec, newId, newEntries>>

\* Any one step happens next.
Next ==
  \/ BfRead \/ BfCommit \/ BfError \/ BfFailRead \/ BfFailCommit
  \/ DropRead \/ DropCommit \/ DropRemove \/ DropFail \/ DropFinish
  \/ ReCreate
  \/ Restart

\* Every step that can keep going eventually does: a backfill and a drop
\* are never left waiting forever while they could move. A failure and a
\* crash may happen but never have to. A node whose drop failed is
\* eventually started again.
Fairness ==
  /\ WF_vars(BfRead) /\ WF_vars(BfCommit) /\ WF_vars(BfFailRead) /\ WF_vars(BfFailCommit)
  /\ WF_vars(DropCommit) /\ WF_vars(DropRemove) /\ WF_vars(DropFinish)
  /\ WF_vars(Restart /\ dropPc = "Failed")

\* Start in Init and take any steps, fairly.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* An index that is not defined has no backfill record: the action list never
\* shows a backfill for a dropped index.
INV_NoRecordForGoneIndex == ~defined => record = "None"

\* A drop that says it is done left no entry of the index on disk.
INV_NoOrphanEntries == dropPc = "Done" => entries = {}

\* A drop never takes an entry from the index created after it: once built,
\* the new index keeps an entry for every old document.
INV_NewIndexIntact == newId # "None" => newEntries = Docs

\* A drop that committed always finishes, across failed pages and restarts.
LIVE_DropFinishes == dropRec ~> (dropPc = "Done")
====
