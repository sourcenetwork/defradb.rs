---- MODULE KvImport ----
\* The story: an operator imports a KV snapshot an old Go Shinzo node
\* exported into a running node. The snapshot holds, for each document, its
\* heads and the blocks they link: only the newest version, so the first
\* version is there only for a document never updated. The import takes the
\* documents one at a
\* time: it reads the document's blocks, stores them, checks that the
\* versions reach the first one (the first version names the document's
\* DocID), then merges the heads. A document whose first version (its create
\* block) is missing cannot be named: it is reported, not merged.
\* The import is a restore, so the replication filter and the retention
\* floor, which judge what peers send, do not judge it.
\*
\* A signing Go node signs every block, and its export leaves the signature
\* blocks out, so every block in the snapshot names a signature block the
\* node never gets. A restore takes such a block as unsigned; a block a peer
\* pushes with its signature block missing is still refused. An encrypted
\* document comes without its encryption block or its key, so its values
\* cannot be read: the import reports it and stores none of its blocks.
\*
\* Who acts: the import; a live writer, which updates documents the node
\* already holds while the import runs; a peer, which pushes a block whose
\* signature block is missing; and a crash, after which the operator runs
\* the whole import again.
\*
\* What must never happen:
\*   - a merged version whose block is not stored;
\*   - a document merged without its first version (it has no DocID);
\*   - a live write lost because the import wrote over it;
\*   - a pushed block merged without its signature block;
\*   - a block of a document the node cannot read stored;
\*   - an import that finishes with a document it carries back to its first
\*     version not fully merged, or with a document it could not name or
\*     read not reported.
\*
\* Code it models (crates/db/src/kv_snapshot/import.rs):
\*   DB::import_kv_snapshot_as (one document at a time),
\*   DB::restore_document (the encryption check, then store the blocks,
\*   then the genesis check, then the merge of each composite head),
\*   unreadable, reaches_genesis, the restore flag
\*   BlockMetadata::restore that composite_persist.rs and
\*   composite_fields.rs read to skip the floor and the filter, and that
\*   batch.rs passes to DbMergeHandler::verify_block_signature
\*   (crates/db/src/merge/merge_handler/signature.rs), the one place a
\*   missing signature block is taken or refused.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Filtered,     \* documents the replication filter would refuse from a peer
  MaxRuns,      \* how many times the import may be started
  StoreMode,    \* "Before": blocks are stored before the merge | "After": after it
  GenesisMode,  \* "Check": no merge without the first version | "Skip": no check
  WriteMode,    \* "Join": the merge joins versions | "Overwrite": it replaces them
  JudgeMode,    \* "Restore": the import is not judged | "Judged": the filter judges it
  ReportMode,   \* "Report": an unnamed document is reported | "Silent": it is dropped
  SigMode,      \* "RestoreOnly": only a restore takes a block without its
                \* signature block | "Always": every merge does | "Never": none does
  EncMode       \* "Check": an unreadable document is not stored | "Skip": no check

\* The documents, in the order the snapshot lists them.
Order == <<"a", "b", "c">>
\* Every document the snapshot names.
Docs == {Order[i] : i \in 1..Len(Order)}
\* Version 1 is a document's first version; 3 is a live write; 4 is a block
\* a peer pushes without its signature block.
First == 1
Live == 3
Pushed == 4
\* The versions the snapshot carries for each document: a, never updated,
\* whole; b, updated once, by its newest version only; c, never updated,
\* whole, but encrypted.
Snap == [d \in Docs |-> IF d = "b" THEN {2} ELSE {1}]
\* The documents whose encryption blocks and keys the snapshot lacks.
Encrypted == {"c"}

\* Each switch takes one of its values.
ASSUME Filtered \subseteq Docs
ASSUME MaxRuns \in 1..3
ASSUME StoreMode \in {"Before", "After"}
ASSUME GenesisMode \in {"Check", "Skip"}
ASSUME WriteMode \in {"Join", "Overwrite"}
ASSUME JudgeMode \in {"Restore", "Judged"}
ASSUME ReportMode \in {"Report", "Silent"}
ASSUME SigMode \in {"RestoreOnly", "Always", "Never"}
ASSUME EncMode \in {"Check", "Skip"}

VARIABLES
  pos,       \* the position in Order of the document the import is on
  step,      \* what the import does next for it: "read", "store", "check" or "merge"
  stored,    \* the blocks on the node, as <<document, version>> pairs
  merged,    \* for each document, the versions the node has merged
  written,   \* the live writes that committed, as <<document, version>>
  reported,  \* the documents the import reported it could not name or read
  runs,      \* how many times the import has been started
  done       \* TRUE once a run reached the end of the snapshot

vars == <<pos, step, stored, merged, written, reported, runs, done>>

\* The document the import is on.
Current == Order[pos]
\* The blocks the snapshot carries for document d.
BlocksOf(d) == {<<d, v>> : v \in Snap[d]}

\* Whether a merge takes a block whose signature block is missing: the
\* import's restore (restore = TRUE) or a peer's push (restore = FALSE).
Accepts(restore) ==
  \/ SigMode = "Always"                   \* (bug) every merge takes it
  \/ SigMode = "RestoreOnly" /\ restore   \* only a restore does

\* The node starts empty and the import at its first document.
Init ==
  /\ pos = 1                              \* the first document
  /\ step = "read"                        \* its blocks are read first
  /\ stored = {}                          \* no blocks yet
  /\ merged = [d \in Docs |-> {}]         \* nothing merged
  /\ written = {}                         \* no live writes
  /\ reported = {}                        \* nothing reported
  /\ runs = 1                             \* the first run
  /\ done = FALSE                         \* not finished

\* Move on to the next document, with its first step.
Advance ==
  /\ pos' = pos + 1                       \* the next document
  /\ step' = "read"                       \* its first step

\* Read the current document's blocks: one the node cannot decrypt is
\* reported and left out before any of its blocks is stored.
Read ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "read"                        \* and reads it next
  /\ IF EncMode = "Check" /\ Current \in Encrypted
       THEN /\ reported' = reported \cup {Current}  \* reported
            /\ Advance                    \* and none of it stored
       ELSE /\ step' = IF StoreMode = "Before" THEN "store" ELSE "check"
                                          \* storing next, unless the bug stores last
            /\ UNCHANGED <<pos, reported>>
  /\ UNCHANGED <<stored, merged, written, runs, done>>

\* Store the current document's blocks: blind puts, so a block already
\* stored is stored again unchanged.
Store ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "store"                       \* and stores next
  /\ stored' = stored \cup BlocksOf(Current)  \* every block it carries
  /\ IF StoreMode = "Before"
       THEN /\ step' = "check"            \* then the check
            /\ UNCHANGED pos              \* same document
       ELSE Advance                       \* (bug) stored last: next document
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

\* Merge the current document's heads. Every block names a signature block
\* the snapshot lacks, so the restore merges it only if it takes that gap.
Merge ==
  /\ ~done /\ pos <= Len(Order)           \* the import is on a document
  /\ step = "merge"                       \* and merges next
  /\ merged' = [merged EXCEPT ![Current] =
       IF JudgeMode = "Judged" /\ Current \in Filtered
         THEN @                           \* (bug) the filter refuses the restore
         ELSE IF ~Accepts(TRUE)
           THEN @                         \* (bug) the missing signature refuses it
           ELSE IF WriteMode = "Join"
             THEN @ \cup Snap[Current]    \* the merge joins its versions in
             ELSE Snap[Current]]          \* (bug) a raw write replaces them
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

\* A peer pushes an update to a document the node holds, whose signature
\* block it does not send: a normal merge, not a restore. It is stored and
\* merged only if that merge takes the gap; otherwise nothing changes.
Push(d) ==
  /\ merged[d] # {}                       \* the node holds the document
  /\ Pushed \notin merged[d]              \* not merged yet
  /\ Accepts(FALSE)                       \* the merge takes the gap
  /\ stored' = stored \cup {<<d, Pushed>>}  \* its block
  /\ merged' = [merged EXCEPT ![d] = @ \cup {Pushed}]  \* its version
  /\ UNCHANGED <<pos, step, written, reported, runs, done>>

\* The node crashes, or the operator stops the import; it runs again from
\* the start. What committed stays.
Rerun ==
  /\ runs < MaxRuns                       \* another run is allowed
  /\ runs' = runs + 1                     \* counted
  /\ pos' = 1                             \* from the first document
  /\ step' = "read"                       \* its first step
  /\ done' = FALSE                        \* not finished again yet
  /\ reported' = {}                       \* a new run reports afresh
  /\ UNCHANGED <<stored, merged, written>>

Next ==
  \/ Read \/ Store \/ Check \/ Merge \/ Finish \/ Rerun
  \/ \E d \in Docs : LiveWrite(d) \/ Push(d)

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

\* Promise: a block a peer pushed without its signature block is never merged.
INV_NoUnsignedPush ==
  \A d \in Docs : Pushed \notin merged[d]

\* Promise: no block of a document the node cannot read is stored.
INV_NoUnreadableStored ==
  \A b \in stored : b[1] \notin Encrypted

\* Promise: a finished run merged every readable document it carries back
\* to its first version and reported every other one.
INV_Complete ==
  done =>
    \A d \in Docs :
      IF d \in Encrypted
        THEN d \in reported
        ELSE IF First \in Snap[d]
          THEN Snap[d] \subseteq merged[d]
          ELSE d \in reported \/ First \in merged[d]
====
