---- MODULE KvImport ----
\* The story: an operator imports a KV snapshot into a running node. The
\* snapshot holds, for each document, its heads and the blocks it carries:
\* every version back to the first one when a Rust node wrote it, or only
\* the newest when a Go node did. The import takes the documents one at a
\* time: it stores the document's blocks, checks that the versions reach the
\* first one (the first version names the document's DocID), then merges the
\* heads. A document whose first version is missing is reported, not merged.
\* The import is a restore, so the replication filter and the retention
\* floor, which judge what peers send, do not judge it.
\*
\* Who acts: the import; a live writer, which updates documents the node
\* already holds while the import runs; and a crash, after which the
\* operator runs the whole import again.
\*
\* What must never happen:
\*   - a merged version whose block is not stored;
\*   - a document merged without its first version (it has no DocID);
\*   - a live write lost because the import wrote over it;
\*   - an import that finishes with a document it carries back to its first
\*     version not fully merged, or with a document it could not name not
\*     reported.
\*
\* Code it models (crates/db/src/kv_snapshot/import.rs):
\*   DB::import_kv_snapshot_as (one document at a time),
\*   DB::restore_document (store the blocks, then the genesis check, then
\*   the merge of each composite head), reaches_genesis, and the restore
\*   flag BlockMetadata::restore that composite_persist.rs and
\*   composite_fields.rs read to skip the floor and the filter.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  SnapMode,     \* "History": a Rust export, every version | "Shallow": a Go export, newest only
  Filtered,     \* documents the replication filter would refuse from a peer
  MaxRuns,      \* how many times the import may be started
  StoreMode,    \* "Before": blocks are stored before the merge | "After": after it
  GenesisMode,  \* "Check": no merge without the first version | "Skip": no check
  WriteMode,    \* "Join": the merge joins versions | "Overwrite": it replaces them
  JudgeMode,    \* "Restore": the import is not judged | "Judged": the filter judges it
  ReportMode    \* "Report": an unnamed document is reported | "Silent": it is dropped

\* The documents, in the order the snapshot lists them.
Order == <<"a", "b">>
\* Every document the snapshot names.
Docs == {Order[i] : i \in 1..Len(Order)}
\* Version 1 is a document's first version; 3 is a live write.
First == 1
Live == 3
\* The versions the snapshot carries for each document. A Rust export
\* carries a's two versions and b's one; a Go export carries a, never
\* updated, whole, and b, updated once, by its newest version only.
Snap ==
  IF SnapMode = "History"
    THEN [d \in Docs |-> IF d = "a" THEN {1, 2} ELSE {1}]
    ELSE [d \in Docs |-> IF d = "a" THEN {1} ELSE {2}]

\* Each switch takes one of its values.
ASSUME SnapMode \in {"History", "Shallow"}
ASSUME Filtered \subseteq Docs
ASSUME MaxRuns \in 1..3
ASSUME StoreMode \in {"Before", "After"}
ASSUME GenesisMode \in {"Check", "Skip"}
ASSUME WriteMode \in {"Join", "Overwrite"}
ASSUME JudgeMode \in {"Restore", "Judged"}
ASSUME ReportMode \in {"Report", "Silent"}

VARIABLES
  pos,       \* the position in Order of the document the import is on
  step,      \* what the import does next for it: "store", "check" or "merge"
  stored,    \* the blocks on the node, as <<document, version>> pairs
  merged,    \* for each document, the versions the node has merged
  written,   \* the live writes that committed, as <<document, version>>
  reported,  \* the documents the import reported it could not name
  runs,      \* how many times the import has been started
  done       \* TRUE once a run reached the end of the snapshot

vars == <<pos, step, stored, merged, written, reported, runs, done>>

\* The document the import is on.
Current == Order[pos]
\* The blocks the snapshot carries for document d.
BlocksOf(d) == {<<d, v>> : v \in Snap[d]}

\* The node starts empty and the import at its first document.
Init ==
  /\ pos = 1                              \* the first document
  /\ step = IF StoreMode = "Before" THEN "store" ELSE "check"
                                          \* storing first, unless the bug skips it
  /\ stored = {}                          \* no blocks yet
  /\ merged = [d \in Docs |-> {}]         \* nothing merged
  /\ written = {}                         \* no live writes
  /\ reported = {}                        \* nothing reported
  /\ runs = 1                             \* the first run
  /\ done = FALSE                         \* not finished

\* Move on to the next document, with its first step.
Advance ==
  /\ pos' = pos + 1                       \* the next document
  /\ step' = IF StoreMode = "Before" THEN "store" ELSE "check"
                                          \* its first step

\* Store the current document's blocks: blind puts, so a block already
\* stored is stored again unchanged.
Store ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "store"                       \* and stores next
  /\ stored' = stored \cup BlocksOf(Current)  \* every block it carries
  /\ step' = "check"                      \* then the check (or, after the bug's
                                          \* late store, the next document's check)
  /\ IF StoreMode = "Before"
       THEN UNCHANGED pos                 \* same document
       ELSE pos' = pos + 1                \* the bug stored last: move on
  /\ UNCHANGED <<merged, written, reported, runs, done>>

\* The first version is on the node: it came in this snapshot or was there.
HasFirst(d) == <<d, First>> \in stored

\* Check the current document reaches its first version.
Check ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "check"                       \* and checks next
  /\ IF GenesisMode = "Check" /\ StoreMode = "Before" /\ ~HasFirst(Current)
       THEN /\ reported' = IF ReportMode = "Report"
                             THEN reported \cup {Current}  \* reported
                             ELSE reported                  \* (bug) dropped silently
            /\ Advance                    \* and not merged
            /\ UNCHANGED <<stored, merged, written, runs, done>>
       ELSE /\ step' = "merge"            \* it can be named: merge it
            /\ UNCHANGED <<pos, stored, merged, written, reported, runs, done>>

\* Merge the current document's heads.
Merge ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "merge"                       \* and merges next
  /\ merged' = [merged EXCEPT ![Current] =
       IF JudgeMode = "Judged" /\ Current \in Filtered
         THEN @                           \* (bug) the filter refuses the restore
         ELSE IF WriteMode = "Join"
           THEN @ \cup Snap[Current]      \* the merge joins its versions in
           ELSE Snap[Current]]            \* (bug) a raw write replaces them
  /\ IF StoreMode = "Before"
       THEN Advance                       \* blocks were stored: next document
       ELSE /\ step' = "store"            \* (bug) store the blocks after
            /\ UNCHANGED pos
  /\ UNCHANGED <<stored, written, reported, runs, done>>

\* The import reaches the end of the snapshot.
Finish ==
  /\ ~done /\ pos > Len(Order)            \* every document handled
  /\ done' = TRUE                         \* the run finished
  /\ UNCHANGED <<pos, step, stored, merged, written, reported, runs>>

\* A live writer updates a document the node holds: its own transaction,
\* storing its block and merging its version.
LiveWrite(d) ==
  /\ merged[d] # {}                       \* the node holds the document
  /\ <<d, Live>> \notin written           \* one live write per document
  /\ stored' = stored \cup {<<d, Live>>}  \* its block
  /\ merged' = [merged EXCEPT ![d] = @ \cup {Live}]  \* its version
  /\ written' = written \cup {<<d, Live>>}  \* it committed
  /\ UNCHANGED <<pos, step, reported, runs, done>>

\* The node crashes, or the operator stops the import; it runs again from
\* the start. What committed stays.
Rerun ==
  /\ runs < MaxRuns                       \* another run is allowed
  /\ runs' = runs + 1                     \* counted
  /\ pos' = 1                             \* from the first document
  /\ step' = IF StoreMode = "Before" THEN "store" ELSE "check"
  /\ done' = FALSE                        \* not finished again yet
  /\ reported' = {}                       \* a new run reports afresh
  /\ UNCHANGED <<stored, merged, written>>

Next ==
  \/ Store \/ Check \/ Merge \/ Finish \/ Rerun
  \/ \E d \in Docs : LiveWrite(d)

Spec == Init /\ [][Next]_vars

\* Promise: every merged version's block is on the node.
INV_MergedHasBlocks ==
  \A d \in Docs : \A v \in merged[d] : <<d, v>> \in stored

\* Promise: a document is merged only with its first version, which names it.
INV_NoDocWithoutFirst ==
  \A d \in Docs : merged[d] # {} => First \in merged[d]

\* Promise: a live write that committed is never lost.
INV_NoLostLiveWrite ==
  \A w \in written : w[2] \in merged[w[1]]

\* Promise: a finished run merged every document it carries back to its
\* first version and reported every other one.
INV_Complete ==
  done =>
    \A d \in Docs :
      IF First \in Snap[d]
        THEN Snap[d] \subseteq merged[d]
        ELSE d \in reported \/ First \in merged[d]
====
