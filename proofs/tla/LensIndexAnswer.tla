--------------------------- MODULE LensIndexAnswer ---------------------------
(***************************************************************************)
(* The story, in plain words.                                              *)
(*                                                                         *)
(* A collection has one document and one secondary index on one field. The *)
(* collection has two versions, "old" and "new", and one of them is        *)
(* active. A migration may add a lens between them that changes the        *)
(* indexed field's value. A query reads the document through that lens     *)
(* when the document is stored at the version that is not active, so what  *)
(* a query reads can differ from what is stored.                           *)
(*                                                                         *)
(* Who acts: a user registers the migration or switches the active         *)
(* version (a schema change); a local writer saves the document at the     *)
(* active version; a peer's merge saves it at whichever version its block  *)
(* names, indexing the value it stores; and a rebuild walks the collection, *)
(* migrates the document to the active version, indexes it again, then in  *)
(* a later transaction clears the "reindex owed" record it started from.   *)
(*                                                                         *)
(* What must never happen: an index answers a query while its entry says   *)
(* something other than what the query reads (the query would miss the    *)
(* document or return the wrong one). And what must happen: once the       *)
(* rebuild has settled and the entry matches, the index answers again      *)
(* (issue #2379: before the fix, a collection with any lens never used its *)
(* index at all).                                                          *)
(*                                                                         *)
(* How the code keeps the promise: every schema change and every merge     *)
(* that stores the document off the active version across a lens writes a *)
(* fresh owed record (a new token) in its own transaction; no index answers *)
(* while a record stands; a rebuild clears only the record whose token it  *)
(* started from.                                                           *)
(*                                                                         *)
(* A query plans and scans in two steps. It plans from what it can see at  *)
(* the time (the cached collection), picking the index when the index      *)
(* answers. Anything can commit before it scans: a merge can owe a reindex *)
(* in between. So the scan reads the owed record again in its own          *)
(* transaction, and when one stands it gives up the index and scans the    *)
(* collection instead, which always reads what queries read. What must     *)
(* never happen: a query that scans the index while the entry differs      *)
(* from what it reads, because it trusted a plan made before the merge.    *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/db/src/definition/migration/reindex/owed.rs owe_reindex,       *)
(*     clear_owed_reindex, settle_owed_reindex                             *)
(*   crates/db/src/definition/migration/reindex/page.rs (the walk)         *)
(*   crates/db/src/collection/mod.rs queryable (no index while owed)       *)
(*   crates/db/src/merge/merge_handler/stale_version.rs                    *)
(*     DbMergeHandler::owe_reindex_off_active_version, deciding on the     *)
(*     active version and definition that                                  *)
(*     Collection::validate_write_definition_read read in the merge's      *)
(*     transaction (Merge reads `active` and `lens` in its own step): a    *)
(*     lens into the active version is read from that definition, any      *)
(*     other from a validated scan of the versions, in the same            *)
(*     transaction);                                                       *)
(*     composite_persist.rs persist_merged_document compares it with the   *)
(*     version the payload stores the document at (Merge's `ver`); a       *)
(*     walk that merges several blocks in one transaction owes once        *)
(*     (block_reads.rs BlockReads), as one Merge step writes one token     *)
(*   crates/db/src/definition/migration/set.rs set_migration_as,           *)
(*   crates/db/src/collection/ops/version.rs set_active_collection_version *)
(*   crates/query/src/planner/builder/index_methods.rs try_select_index    *)
(*     and crates/query/src/runner/query/simple.rs execute_simple_select   *)
(*     (Plan: the index from the cached queryable indexes)                 *)
(*   crates/db/src/read/lensed/autocommit/scan.rs get_by_index_scan_inner, *)
(*     crates/db/src/read/lensed/autocommit/stream.rs                      *)
(*     stream_index_scan_impl (Scan: the owed record read in the scan's    *)
(*     own transaction, QueryError::IndexOwed when it stands), and         *)
(*     crates/query/src/runner/query/select.rs execute_select_internal     *)
(*     (that error re-plans the select without indexes: a collection scan) *)
(* Go reference: internal/db/lens.go setMigration, internal/db/merge.go    *)
(*   syncIndexedDoc (Go indexes the merged document as the active version  *)
(*   reads it).                                                            *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS
  \* Which rule decides whether an index answers: "owed" (the code: no
  \* index while a reindex is owed), "lens" (main before the fix: no index
  \* once the history has any lens), "none" (always answer).
  Gate,
  \* TRUE when a schema change writes an owed record.
  OweOnSchemaChange,
  \* TRUE when a merge off the active version across a lens writes one.
  OweOnMerge,
  \* TRUE when such a merge skips writing because a record already stands.
  SkipIfOwed,
  \* TRUE when a rebuild clears the record only if its token is unchanged.
  ClearChecksToken,
  \* TRUE when a query's index scan reads the owed record again, in its own
  \* transaction, and falls back to a collection scan when one stands.
  ScanRechecksOwed,
  \* How many steps a behaviour may take, to keep the state space finite.
  MaxSteps

\* The constants have the shapes they should.
ASSUME /\ Gate \in {"owed", "lens", "none"}
       /\ OweOnSchemaChange \in BOOLEAN /\ OweOnMerge \in BOOLEAN
       /\ SkipIfOwed \in BOOLEAN /\ ClearChecksToken \in BOOLEAN
       /\ ScanRechecksOwed \in BOOLEAN
       /\ MaxSteps \in Nat

\* The two versions of the collection.
Versions == {"old", "new"}

\* The values the indexed field can hold.
Vals == {0, 1}

\* The lens flips the value; it is its own inverse, so one function covers
\* reading forward and backward.
Flip(v) == 1 - v

\* The version that is not v.
Other(v) == IF v = "old" THEN "new" ELSE "old"

\* No rebuild walk is waiting to clear.
NoWalk == 0

VARIABLES
  \* Which version queries read at.
  active,
  \* Whether the migration's lens between the versions is registered.
  lens,
  \* The document as stored: the version it was saved at and its value.
  stored,
  \* The value the index entry holds for the document.
  entry,
  \* Whether a reindex record stands.
  owed,
  \* The token of the standing record; every new record takes a new one.
  token,
  \* The token a finished walk started from (NoWalk when none waits).
  walk,
  \* How many steps have been taken.
  steps,
  \* The one query's progress: "idle" before it plans, "index" or "scan"
  \* once it has planned which source to read, "done" after it scanned.
  plan,
  \* Whether the query's answer was right: "none" until it scans, then
  \* "right" when it saw what reads see, "wrong" when it did not.
  answer

vars == <<active, lens, stored, entry, owed, token, walk, steps, plan, answer>>

\* The variables the query's two steps leave alone.
world == <<active, lens, stored, entry, owed, token, walk, steps>>

\* What a query reads: the stored value, through the lens when the document
\* sits at the version that is not active.
Read == IF stored.ver = active \/ ~lens THEN stored.val ELSE Flip(stored.val)

\* Whether an index answers queries, by the gate in use.
IndexAnswers == CASE Gate = "owed" -> ~owed
                  [] Gate = "lens" -> ~lens
                  [] OTHER -> TRUE

\* Every variable holds a value of its kind.
TypeOK == /\ active \in Versions /\ lens \in BOOLEAN
          /\ stored \in [ver : Versions, val : Vals] /\ entry \in Vals
          /\ owed \in BOOLEAN /\ token \in Nat /\ walk \in Nat
          /\ steps \in 0..MaxSteps
          /\ plan \in {"idle", "index", "scan", "done"}
          /\ answer \in {"none", "right", "wrong"}

\* At the start: one document stored at "old" with value 0, indexed as 0,
\* "old" active, no lens, no record.
Init == /\ active = "old" /\ lens = FALSE
        /\ stored = [ver |-> "old", val |-> 0] /\ entry = 0
        /\ owed = FALSE /\ token = 1 /\ walk = NoWalk /\ steps = 0
        \* The query has not planned yet and has no answer.
        /\ plan = "idle" /\ answer = "none"

\* Write a fresh owed record (a new token), in the acting transaction.
Owe == owed' = TRUE /\ token' = token + 1

\* A schema change writes a record if the guard is on.
OweForSchemaChange == IF OweOnSchemaChange THEN Owe ELSE UNCHANGED <<owed, token>>

\* Count one step, and stop once the bound is reached.
Step == steps < MaxSteps /\ steps' = steps + 1

\* A user registers the migration's lens; the change owes a reindex.
SetMigration == /\ Step /\ ~lens /\ lens' = TRUE
                /\ OweForSchemaChange
                /\ UNCHANGED <<active, stored, entry, walk>>

\* A user switches the active version; the change owes a reindex.
SwitchActive == /\ Step /\ active' = Other(active)
                /\ OweForSchemaChange
                /\ UNCHANGED <<lens, stored, entry, walk>>

\* A local writer saves value v at the active version and indexes it.
LocalWrite(v) == /\ Step /\ stored' = [ver |-> active, val |-> v] /\ entry' = v
                 /\ UNCHANGED <<active, lens, owed, token, walk>>

\* A peer's merge saves value v at version ver and indexes the value as
\* stored. Off the active version across the lens, it owes a reindex,
\* unless the guard is off, or it skips because a record already stands.
Merge(ver, v) ==
  /\ Step /\ stored' = [ver |-> ver, val |-> v] /\ entry' = v
  \* Whether this merge stores the document where reads go through the lens.
  /\ LET acrossLens == ver # active /\ lens
     IN IF OweOnMerge /\ acrossLens /\ ~(SkipIfOwed /\ owed)
          THEN Owe ELSE UNCHANGED <<owed, token>>
  /\ UNCHANGED <<active, lens, walk>>

\* The rebuild's walk, while a record stands: migrate the document to the
\* active version, index what reads see, and remember the token it saw.
RebuildWalk == /\ Step /\ owed /\ walk = NoWalk
               /\ stored' = [ver |-> active, val |-> Read] /\ entry' = Read
               /\ walk' = token
               /\ UNCHANGED <<active, lens, owed, token>>

\* The rebuild's last page: clear the record, but only the one it started
\* from when the token check is on; a newer record stays.
RebuildClear == /\ Step /\ walk # NoWalk /\ walk' = NoWalk
                /\ IF ~ClearChecksToken \/ walk = token
                     THEN owed' = FALSE ELSE UNCHANGED owed
                /\ UNCHANGED <<active, lens, stored, entry, token>>

\* The query plans: it picks the index when the index answers right now
\* (the cached collection it plans from), and a collection scan otherwise.
\* Planning does not count as a step, so every world behaviour of MaxSteps
\* steps can still happen between the plan and the scan.
Plan == /\ plan = "idle"
        /\ plan' = IF IndexAnswers THEN "index" ELSE "scan"
        /\ UNCHANGED <<world, answer>>

\* The query scans what it planned. A collection scan reads what reads see,
\* so it is always right. An index scan first reads the owed record again,
\* when the guard is on, and scans the collection instead when one stands;
\* otherwise it answers from the entry, which is right only when the entry
\* holds what reads see.
Scan == /\ plan \in {"index", "scan"}
        /\ plan' = "done"
        \* Whether the scan gives up the index for a collection scan.
        /\ LET fallsBack == ScanRechecksOwed /\ owed
           IN answer' = IF plan = "scan" \/ fallsBack \/ entry = Read
                          THEN "right" ELSE "wrong"
        /\ UNCHANGED world

\* Any one of the actions above. The world's actions leave the query alone.
Next == \/ /\ \/ SetMigration \/ SwitchActive
              \/ \E v \in Vals : LocalWrite(v)
              \/ \E ver \in Versions, v \in Vals : Merge(ver, v)
              \/ RebuildWalk \/ RebuildClear
           /\ UNCHANGED <<plan, answer>>
        \/ Plan \/ Scan

\* The whole behaviour: start in Init, take Next steps (or stutter).
Spec == Init /\ [][Next]_vars

\* An index that answers holds what a query reads: no wrong answer.
INV_IndexMatchesReads == IndexAnswers => entry = Read

\* Once nothing is owed or waiting and the entry matches, the index answers
\* (nothing valid is refused: #2379).
INV_SettledIndexAnswers == (~owed /\ walk = NoWalk /\ entry = Read) => IndexAnswers

\* A query that planned before a merge and scanned after it still answers
\* what reads see: no plan is trusted past a reindex owed in between.
INV_ScanMatchesReads == answer # "wrong"
================================================================================
