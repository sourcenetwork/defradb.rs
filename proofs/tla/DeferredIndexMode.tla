---------------------------- MODULE DeferredIndexMode ----------------------------
\* The story: an index on a counter is kept one of two ways, and the CRDT types
\* registered when the node starts decide which. Inline, a write moves the entry
\* itself. Deferred, a write only adds the document to a log, and a drain later
\* moves the entry to where the document's position record says it is now.
\*
\* The way can change between two starts: a custom type registered on one start
\* and not the next, or an upgrade from a build that kept such an index inline.
\* The stored state then belongs to the other way: inline entries with no
\* records, or records and a log an inline index never reads.
\*
\* So at every open, before the node takes writes, a check compares the stored
\* state with the way this start keeps the index. A deferred index with entries
\* and no record, or an inline one that still has records or a log, is owed a
\* rebuild. The rebuild clears the entries (and, for an inline index, the records
\* and the log), then walks the documents and writes each one's entry again (and,
\* for a deferred index, its record). It runs in pages and a crash can stop it, so
\* it resumes from its saved progress at the next open, which may be under the
\* other way again: the progress names the way it was written under, and a
\* rebuild resumed under the other way starts over.
\*
\* Who acts: the node's starts (each one picks a way), writes and drains while it
\* is open, the open-time check and the rebuild's pages.
\*
\* What must never happen: an open node whose index holds an entry the way it is
\* kept does not account for, so that a later write or drain leaves it behind.
\*
\* Code it models:
\*   crates/db/src/definition/migration/format.rs  DB::migrate_index_format (the check at open)
\*   crates/db/src/index/deferred/mode.rs          kept_in_other_mode (what the check looks at)
\*   crates/db/src/definition/migration/reindex/page.rs  clear (an inline index's records and log go too)
\*   crates/db/src/definition/migration/reindex/progress.rs  read (progress under another way starts over)
\*   crates/db/src/index/manager/rebuild.rs        IndexManager::reindex_document (the walk)
EXTENDS Integers, FiniteSets

CONSTANTS
  Docs,          \* the documents
  MaxValue,      \* how high a counter may go
  MaxRestarts,   \* how many starts after the first the check explores
  StartModes,    \* the ways the store may have been kept before the first start
  Check,         \* "Both": the open check looks both ways | "ToDeferred": only for a deferred index | "Off"
  ClearRecords,  \* "On": a rebuild of an inline index removes its records and log | "Off": it keeps them
  ProgressReset  \* "On": progress written under the other way starts over | "Off": it resumes as is

\* Each switch takes one of its values.
ASSUME StartModes \subseteq {"Inline", "Deferred"} /\ StartModes /= {}
ASSUME Check \in {"Both", "ToDeferred", "Off"}
ASSUME ClearRecords \in {"On", "Off"}
ASSUME ProgressReset \in {"On", "Off"}
\* A counter can grow at least once, or nothing happens.
ASSUME MaxValue \in Nat \ {0}
\* A count of starts.
ASSUME MaxRestarts \in Nat

\* No record: a document whose index keeps none.
None == -1

\* The two ways an index is kept.
Modes == {"Inline", "Deferred"}

\* The values a counter can take.
Values == 0..MaxValue

VARIABLES
  mode,      \* the way this start keeps the index
  phase,     \* "Check" (just started), "Rebuild" (opening, a rebuild owed) or "Open" (taking writes)
  value,     \* each document's counter
  index,     \* the stored entries: pairs of a document and a value
  rec,       \* each document's position record: the value its entry is at, or None
  owed,      \* the documents in the log, waiting for a drain
  rebuild,   \* whether a rebuild is owed (its record is durable)
  progress,  \* the rebuild's saved stage: "Start" (clear next) or "Cleared" (walking)
  walked,    \* the documents the saved progress says were walked
  progMode,  \* the way the saved progress was written under
  restarts   \* starts so far after the first

vars == <<mode, phase, value, index, rec, owed, rebuild, progress, walked, progMode, restarts>>

\* The values a document's entries are at.
Entries(d) == {v \in Values : <<d, v>> \in index}

\* Whether the stored state belongs to the other way, as the check at open sees it.
OtherWay ==
  IF mode = "Deferred"
  THEN index /= {} /\ \A d \in Docs : rec[d] = None          \* entries, and not one record
  ELSE (\E d \in Docs : rec[d] /= None) \/ owed /= {}         \* records or a log an inline index never reads

\* Whether this start's check looks for the other way at all.
Looks == \/ Check = "Both"
         \/ Check = "ToDeferred" /\ mode = "Deferred"

\* Every variable stays in its range.
TypeOK ==
  /\ mode \in Modes                                  \* a way
  /\ phase \in {"Check", "Rebuild", "Open"}           \* a phase
  /\ value \in [Docs -> Values]                      \* counters
  /\ index \subseteq Docs \X Values                  \* entries
  /\ rec \in [Docs -> Values \cup {None}]            \* records
  /\ owed \subseteq Docs                             \* the log
  /\ rebuild \in BOOLEAN                             \* owed or not
  /\ progress \in {"Start", "Cleared"}               \* stages
  /\ walked \subseteq Docs                           \* walked documents
  /\ progMode \in Modes                              \* a way
  /\ restarts \in 0..MaxRestarts                     \* bounded starts

\* The store as the way before the first start left it, settled, and the node just started.
Init ==
  /\ mode \in Modes                                  \* the first start picks either way
  /\ phase = "Check"                                 \* it checks before it opens
  /\ value = [d \in Docs |-> 0]                      \* every counter at 0
  /\ index = {<<d, 0>> : d \in Docs}                 \* each document indexed at 0
  /\ \E before \in StartModes :                      \* the way the store was kept so far
       /\ rec = [d \in Docs |-> IF before = "Deferred" THEN 0 ELSE None]   \* records only if kept deferred
       /\ progMode = before                          \* unused until a rebuild saves progress
  /\ owed = {}                                       \* nothing owed
  /\ rebuild = FALSE                                 \* no rebuild owed
  /\ progress = "Start" /\ walked = {}               \* no progress saved
  /\ restarts = 0                                    \* the first start

\* The check at open: a rebuild already owed stands, and one more is owed if the
\* state belongs to the other way. With nothing owed the node opens.
CheckAtOpen ==
  /\ phase = "Check"                                 \* just started
  /\ LET owe == rebuild \/ (Looks /\ OtherWay)       \* what is owed after the check
     IN /\ rebuild' = owe                            \* the owed record is written
        /\ phase' = IF owe THEN "Rebuild" ELSE "Open"   \* rebuild first, or open
  /\ UNCHANGED <<mode, value, index, rec, owed, progress, walked, progMode, restarts>>

\* The progress a page reads: what was saved, or the start if it was saved under the other way.
Stale == ProgressReset = "On" /\ progMode /= mode

\* A clear page: every entry goes; an inline index's records and log go too.
RebuildClear ==
  /\ phase = "Rebuild"                               \* opening, a rebuild owed
  /\ progress = "Start" \/ Stale                     \* the clear is next
  /\ index' = {}                                     \* the entries are gone
  /\ rec' = IF mode = "Inline" /\ ClearRecords = "On" THEN [d \in Docs |-> None] ELSE rec   \* inline: records go
  /\ owed' = IF mode = "Inline" /\ ClearRecords = "On" THEN {} ELSE owed   \* inline: the log goes
  /\ progress' = "Cleared" /\ walked' = {}           \* the walk is next
  /\ progMode' = mode                                \* saved under this start's way
  /\ UNCHANGED <<mode, phase, value, rebuild, restarts>>

\* A walk page for d: its entry again, and its record if the index is deferred.
\* The page that walks the last document clears the owed rebuild, and the node opens.
RebuildWalk(d) ==
  /\ phase = "Rebuild"                               \* opening, a rebuild owed
  /\ progress = "Cleared" /\ ~Stale                  \* walking, under this start's way
  /\ d \notin walked                                 \* not written yet
  /\ index' = index \cup {<<d, value[d]>>}           \* its entry at its value
  /\ rec' = IF mode = "Deferred" THEN [rec EXCEPT ![d] = value[d]] ELSE rec   \* deferred: the record names it
  /\ IF walked \cup {d} = Docs                       \* the last document
     THEN /\ rebuild' = FALSE /\ phase' = "Open"     \* the rebuild is done; the node opens
          /\ progress' = "Start" /\ walked' = {}     \* the progress goes with the owed record
     ELSE /\ walked' = walked \cup {d}               \* saved with the page
          /\ UNCHANGED <<rebuild, phase, progress>>
  /\ UNCHANGED <<mode, value, owed, progMode, restarts>>

\* A write while open: inline, it moves the entry from the old value to the new one;
\* deferred, it only adds the document to the log.
Write(d) ==
  /\ phase = "Open"                                  \* the node takes writes
  /\ value[d] < MaxValue                             \* bounded exploration
  /\ value' = [value EXCEPT ![d] = @ + 1]            \* the counter grows
  /\ IF mode = "Inline"
     THEN /\ index' = (index \ {<<d, value[d]>>}) \cup {<<d, value[d] + 1>>}   \* the entry moves
          /\ UNCHANGED owed
     ELSE /\ owed' = owed \cup {d}                   \* the index is owed the move
          /\ UNCHANGED index
  /\ UNCHANGED <<mode, phase, rec, rebuild, progress, walked, progMode, restarts>>

\* A drain while open, of a deferred index: the entry the record names goes and one
\* at the live value comes in.
Drain(d) ==
  /\ phase = "Open" /\ mode = "Deferred"             \* only a deferred index drains
  /\ d \in owed                                      \* something is owed for d
  /\ index' = (index \ {<<d, rec[d]>>}) \cup {<<d, value[d]>>}   \* old entry out, live one in
  /\ rec' = [rec EXCEPT ![d] = value[d]]             \* the record follows
  /\ owed' = owed \ {d}                              \* the log is truncated for d
  /\ UNCHANGED <<mode, phase, value, rebuild, progress, walked, progMode, restarts>>

\* A crash or a stop, then a start that may keep the index the other way. What
\* was durable stays: the store, the owed rebuild and its saved progress.
Restart(m) ==
  /\ restarts < MaxRestarts                          \* bounded starts
  /\ restarts' = restarts + 1                        \* one more start
  /\ mode' = m                                       \* this start's way
  /\ phase' = "Check"                                \* it checks before it opens
  /\ UNCHANGED <<value, index, rec, owed, rebuild, progress, walked, progMode>>

\* Any one step happens next.
Next ==
  \/ CheckAtOpen \/ RebuildClear
  \/ \E d \in Docs : RebuildWalk(d) \/ Write(d) \/ Drain(d)
  \/ \E m \in Modes : Restart(m)

\* Start in Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* An open node's index is what the way it is kept accounts for: inline, each
\* document has one entry, at its value; deferred, each document has one entry,
\* the one its record names, at its value unless the document is owed a drain.
INV_OpenMatchesMode ==
  phase = "Open" =>
    \A d \in Docs :
      IF mode = "Inline"
      THEN Entries(d) = {value[d]}
      ELSE /\ rec[d] /= None /\ Entries(d) = {rec[d]}
           /\ (d \notin owed => rec[d] = value[d])
====
