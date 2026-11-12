---------------------------- MODULE HeldBlockIngest ----------------------------
\* The story. Two CAR responses arrive at once. Each carries its own root and
\* one shared block X. Storing a block that is not stored yet writes the block
\* and its to-merge mark (`ToMergeIndexKey`); storing a block already stored
\* writes nothing. So, before storing, an ingest takes an owner on every block
\* it will write: two ingests must not write the same mark at once. A merge
\* later deletes a block's mark, and takes no ingest owner. An orphan sweep
\* may delete an unmerged block.
\*
\* An ingest first drops the blocks it already holds, then takes owners only
\* on the blocks left, then stores them, then lets its owners go. A block it
\* dropped was stored, so storing it would have written nothing: it needs no
\* owner, and an owner on it must not make the ingest give up.
\*
\* What must never happen:
\*   - two writers touch one block's mark at the same time (two stores, or a
\*     store and a merge);
\*   - an ingest gives up because another ingest owns a block it already held;
\*   - an ingest finishes and one of its blocks is neither stored nor swept.
\*
\* Rust: crates/p2p/src/sync/coordinator/event_handler/car.rs
\* handle_car_fetch_response_inner and drop_held_blocks (drop held, read with
\* one has_many snapshot, then own, then put_many);
\* crates/p2p/src/sync/manager/process/mod.rs
\* try_acquire_car_storage_owners (all-or-nothing owners, StorageContended on
\* any taken one); crates/storage/src/stores/blockstore.rs BlockWritesAttempt
\* (a put of a stored block writes nothing; MarkMerged deletes the mark);
\* crates/db/src/merge/merge_handler/field_ops.rs
\* best_effort_finalize_linked_field_blocks (merge marks blocks merged without
\* an ingest owner).
EXTENDS Naturals, FiniteSets

\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "noowners" an ingest takes no owners at all      -> INV_OneMarkWriter
\*   "ownheld"  an ingest also owns blocks it holds    -> INV_HeldNeverContends
CONSTANTS Bug

\* The two ingests.
Ingests == {1, 2}

\* Every block: the two roots and the shared block.
Blocks == {"R1", "R2", "X"}

\* What each ingest's response carries.
Resp(i) == IF i = 1 THEN {"R1", "X"} ELSE {"R2", "X"}

\* stored: blocks in the store. marks: stored blocks not merged yet.
\* owner: which ingest owns each block (0 for none).
\* pc: where each ingest is: "filter", "own", "put", "release", "done", or
\* "contended" (it gave up and will be retried).
\* fresh: the blocks an ingest found missing, the ones it will store.
\* heldAt: the blocks an ingest found already stored.
\* toPut: the fresh blocks an ingest has not stored yet.
\* writing: the ingests storing each block right now.
\* merging: blocks a merge is marking merged right now.
\* swept: blocks the orphan sweep deleted.
\* heldContend: an ingest gave up over a block it already held.
VARIABLES stored, marks, owner, pc, fresh, heldAt, toPut, writing, merging,
          swept, heldContend

\* Every variable, for UNCHANGED.
vars == <<stored, marks, owner, pc, fresh, heldAt, toPut, writing, merging,
          swept, heldContend>>

\* The blocks an ingest asks owners for: the fresh ones, all of its blocks
\* with "ownheld", none with "noowners".
OwnSet(i) ==
  \* The planted bug: owners on held blocks too.
  CASE Bug = "ownheld"  -> Resp(i)
    \* The planted bug: no owners at all.
    [] Bug = "noowners" -> {}
    \* The real code: owners on the fresh blocks only.
    [] OTHER            -> fresh[i]

\* The world picks whether X is already stored, merged or not.
Init ==
  \* X is stored or not; if stored, its mark is still there or merged away.
  /\ \E x \in {{}, {"X"}}, m \in BOOLEAN :
       \* The store holds X, or nothing.
       /\ stored = x
       \* X's mark is there only if X is stored and not merged yet.
       /\ marks = IF m THEN x ELSE {}
  \* Nobody owns anything yet.
  /\ owner = [b \in Blocks |-> 0]
  \* Both ingests start by checking what they hold.
  /\ pc = [i \in Ingests |-> "filter"]
  \* No ingest has sorted its blocks yet.
  /\ fresh = [i \in Ingests |-> {}]
  \* Nor found any it holds.
  /\ heldAt = [i \in Ingests |-> {}]
  \* Nor has anything left to store.
  /\ toPut = [i \in Ingests |-> {}]
  \* Nobody is storing anything.
  /\ writing = [b \in Blocks |-> {}]
  \* No merge is in flight.
  /\ merging = {}
  \* The sweep has deleted nothing.
  /\ swept = {}
  \* No ingest has given up.
  /\ heldContend = FALSE

\* An ingest reads which of its blocks are stored and keeps the rest.
Filter(i) ==
  \* Only an ingest that has not filtered yet.
  /\ pc[i] = "filter"
  \* The blocks it lacks are the ones it will store.
  /\ fresh' = [fresh EXCEPT ![i] = Resp(i) \ stored]
  \* The blocks it already holds are dropped, and remembered.
  /\ heldAt' = [heldAt EXCEPT ![i] = Resp(i) \cap stored]
  \* Every fresh block is still to be stored.
  /\ toPut' = [toPut EXCEPT ![i] = Resp(i) \ stored]
  \* Next it takes its owners.
  /\ pc' = [pc EXCEPT ![i] = "own"]
  \* Filtering only reads: nothing else moves.
  /\ UNCHANGED <<stored, marks, owner, writing, merging, swept, heldContend>>

\* An ingest takes all its owners at once, or none: if another ingest owns
\* one of them, it gives up and remembers whether that block was one it held.
Own(i) ==
  \* Only an ingest about to take its owners.
  /\ pc[i] = "own"
  \* taken: the blocks it wants that another ingest owns.
  /\ LET taken == {b \in OwnSet(i) : owner[b] \notin {0, i}}
     \* One of them is taken: it gives up and takes none.
     IN IF taken # {}
          \* It gives up, to be retried.
          THEN /\ pc' = [pc EXCEPT ![i] = "contended"]
               \* Giving up over a block it already held is the bug to catch.
               /\ heldContend' = (heldContend \/ taken \cap heldAt[i] # {})
               \* It takes no owner.
               /\ UNCHANGED owner
          \* None is taken: it owns all of them at once.
          ELSE /\ owner' = [b \in Blocks |-> IF b \in OwnSet(i) THEN i ELSE owner[b]]
               \* Next it stores its fresh blocks.
               /\ pc' = [pc EXCEPT ![i] = "put"]
               \* It did not give up.
               /\ UNCHANGED heldContend
  \* Taking owners writes nothing to the store.
  /\ UNCHANGED <<stored, marks, fresh, heldAt, toPut, writing, merging, swept>>

\* An ingest starts storing one of its fresh blocks. If the block is stored
\* by now, the store writes nothing; otherwise it starts writing the block
\* and its mark.
PutBegin(i) ==
  \* Only an ingest that holds its owners.
  /\ pc[i] = "put"
  \* It picks one fresh block it has not stored yet.
  /\ \E b \in toPut[i] :
       \* That block is no longer left to store.
       /\ toPut' = [toPut EXCEPT ![i] = @ \ {b}]
       \* Stored meanwhile: the put writes nothing, so no writer is added.
       /\ writing' = IF b \in stored THEN writing
                     \* Not stored: this ingest is now writing the block and
                     \* its mark.
                     ELSE [writing EXCEPT ![b] = @ \cup {i}]
  \* Starting a write changes nothing else yet.
  /\ UNCHANGED <<stored, marks, owner, pc, fresh, heldAt, merging, swept,
                 heldContend>>

\* A store in flight finishes: the block and its mark are written.
PutEnd(i) ==
  \* Some block this ingest is writing.
  /\ \E b \in Blocks :
       \* This ingest is writing it.
       /\ i \in writing[b]
       \* The block is stored.
       /\ stored' = stored \cup {b}
       \* Its to-merge mark is written with it.
       /\ marks' = marks \cup {b}
       \* This ingest is no longer writing it.
       /\ writing' = [writing EXCEPT ![b] = @ \ {i}]
  \* Finishing a write moves nothing else.
  /\ UNCHANGED <<owner, pc, fresh, heldAt, toPut, merging, swept, heldContend>>

\* An ingest whose stores are all done lets its owners go.
Release(i) ==
  \* Only an ingest in its storing step.
  /\ pc[i] = "put"
  \* With nothing left to start storing.
  /\ toPut[i] = {}
  \* And no write of its own still in flight.
  /\ \A b \in Blocks : i \notin writing[b]
  \* Every owner it held goes back to nobody.
  /\ owner' = [b \in Blocks |-> IF owner[b] = i THEN 0 ELSE owner[b]]
  \* It is done.
  /\ pc' = [pc EXCEPT ![i] = "done"]
  \* Letting owners go writes nothing.
  /\ UNCHANGED <<stored, marks, fresh, heldAt, toPut, writing, merging, swept,
                 heldContend>>

\* A merge starts marking a stored, unmerged block merged. It takes no
\* ingest owner.
MergeBegin ==
  \* Some stored block that still has its mark.
  /\ \E b \in marks \cap stored :
       \* Not already being merged.
       /\ b \notin merging
       \* A merge is now touching its mark.
       /\ merging' = merging \cup {b}
  \* Starting a merge changes nothing else yet.
  /\ UNCHANGED <<stored, marks, owner, pc, fresh, heldAt, toPut, writing,
                 swept, heldContend>>

\* A merge in flight deletes the mark.
MergeEnd ==
  \* Some block a merge is touching.
  /\ \E b \in merging :
       \* The merge is over.
       /\ merging' = merging \ {b}
       \* Its mark is gone: the block is merged.
       /\ marks' = marks \ {b}
  \* Finishing a merge moves nothing else.
  /\ UNCHANGED <<stored, owner, pc, fresh, heldAt, toPut, writing, swept,
                 heldContend>>

\* The orphan sweep deletes an unmerged block nobody is writing.
Sweep ==
  \* Some stored block that still has its mark.
  /\ \E b \in marks \cap stored :
       \* Nobody is storing it right now.
       /\ writing[b] = {}
       \* And no merge is touching it.
       /\ b \notin merging
       \* The block is deleted.
       /\ stored' = stored \ {b}
       \* With its mark.
       /\ marks' = marks \ {b}
       \* And remembered as swept.
       /\ swept' = swept \cup {b}
  \* The sweep touches no ingest's state.
  /\ UNCHANGED <<owner, pc, fresh, heldAt, toPut, writing, merging, heldContend>>

\* Once nothing can move, it stays that way.
Idle ==
  \* Both ingests have finished or given up.
  /\ \A i \in Ingests : pc[i] \in {"done", "contended"}
  \* Nothing changes.
  /\ UNCHANGED vars

\* One step of either ingest, the merge or the sweep.
Next ==
  \* Either ingest takes one of its steps.
  \/ \E i \in Ingests : Filter(i) \/ Own(i) \/ PutBegin(i) \/ PutEnd(i) \/ Release(i)
  \* Or the merge, the sweep, or nothing once all is over.
  \/ MergeBegin \/ MergeEnd \/ Sweep \/ Idle

\* Everything that can happen.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  \* The store holds only known blocks.
  /\ stored \subseteq Blocks
  \* Marks are only on known blocks.
  /\ marks \subseteq Blocks
  \* Each block is owned by nobody or by one ingest.
  /\ owner \in [Blocks -> {0} \cup Ingests]
  \* Merges touch only known blocks.
  /\ merging \subseteq Blocks

\* At most one writer touches a block's mark at a time.
INV_OneMarkWriter ==
  \* Storing ingests plus a merge, counted per block, are never more than one.
  \A b \in Blocks : Cardinality(writing[b]) + (IF b \in merging THEN 1 ELSE 0) <= 1

\* No ingest gives up over a block it already held.
INV_HeldNeverContends == ~heldContend

\* A finished ingest left each of its blocks stored, or the sweep took it, in
\* which case the merge's missing-link check asks for it again.
INV_Lands ==
  \* Every block of a finished ingest is stored or was swept.
  \A i \in Ingests : pc[i] = "done" => Resp(i) \subseteq stored \cup swept
=============================================================================
