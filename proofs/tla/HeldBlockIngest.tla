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
\* handle_car_fetch_response_inner and drop_held_blocks (drop held, then own,
\* then put_many); crates/p2p/src/sync/manager/process/mod.rs
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
  CASE Bug = "ownheld"  -> Resp(i)
    [] Bug = "noowners" -> {}
    [] OTHER            -> fresh[i]

\* The world picks whether X is already stored, merged or not.
Init ==
  /\ \E x \in {{}, {"X"}}, m \in BOOLEAN :
       /\ stored = x
       /\ marks = IF m THEN x ELSE {}
  /\ owner = [b \in Blocks |-> 0]
  /\ pc = [i \in Ingests |-> "filter"]
  /\ fresh = [i \in Ingests |-> {}]
  /\ heldAt = [i \in Ingests |-> {}]
  /\ toPut = [i \in Ingests |-> {}]
  /\ writing = [b \in Blocks |-> {}]
  /\ merging = {}
  /\ swept = {}
  /\ heldContend = FALSE

\* An ingest reads which of its blocks are stored and keeps the rest.
Filter(i) ==
  /\ pc[i] = "filter"
  /\ fresh' = [fresh EXCEPT ![i] = Resp(i) \ stored]
  /\ heldAt' = [heldAt EXCEPT ![i] = Resp(i) \cap stored]
  /\ toPut' = [toPut EXCEPT ![i] = Resp(i) \ stored]
  /\ pc' = [pc EXCEPT ![i] = "own"]
  /\ UNCHANGED <<stored, marks, owner, writing, merging, swept, heldContend>>

\* An ingest takes all its owners at once, or none: if another ingest owns
\* one of them, it gives up and remembers whether that block was one it held.
Own(i) ==
  /\ pc[i] = "own"
  /\ LET taken == {b \in OwnSet(i) : owner[b] \notin {0, i}}
     IN IF taken # {}
          THEN /\ pc' = [pc EXCEPT ![i] = "contended"]
               /\ heldContend' = (heldContend \/ taken \cap heldAt[i] # {})
               /\ UNCHANGED owner
          ELSE /\ owner' = [b \in Blocks |-> IF b \in OwnSet(i) THEN i ELSE owner[b]]
               /\ pc' = [pc EXCEPT ![i] = "put"]
               /\ UNCHANGED heldContend
  /\ UNCHANGED <<stored, marks, fresh, heldAt, toPut, writing, merging, swept>>

\* An ingest starts storing one of its fresh blocks. If the block is stored
\* by now, the store writes nothing; otherwise it starts writing the block
\* and its mark.
PutBegin(i) ==
  /\ pc[i] = "put"
  /\ \E b \in toPut[i] :
       /\ toPut' = [toPut EXCEPT ![i] = @ \ {b}]
       /\ writing' = IF b \in stored THEN writing
                     ELSE [writing EXCEPT ![b] = @ \cup {i}]
  /\ UNCHANGED <<stored, marks, owner, pc, fresh, heldAt, merging, swept,
                 heldContend>>

\* A store in flight finishes: the block and its mark are written.
PutEnd(i) ==
  /\ \E b \in Blocks :
       /\ i \in writing[b]
       /\ stored' = stored \cup {b}
       /\ marks' = marks \cup {b}
       /\ writing' = [writing EXCEPT ![b] = @ \ {i}]
  /\ UNCHANGED <<owner, pc, fresh, heldAt, toPut, merging, swept, heldContend>>

\* An ingest whose stores are all done lets its owners go.
Release(i) ==
  /\ pc[i] = "put"
  /\ toPut[i] = {}
  /\ \A b \in Blocks : i \notin writing[b]
  /\ owner' = [b \in Blocks |-> IF owner[b] = i THEN 0 ELSE owner[b]]
  /\ pc' = [pc EXCEPT ![i] = "done"]
  /\ UNCHANGED <<stored, marks, fresh, heldAt, toPut, writing, merging, swept,
                 heldContend>>

\* A merge starts marking a stored, unmerged block merged. It takes no
\* ingest owner.
MergeBegin ==
  /\ \E b \in marks \cap stored :
       /\ b \notin merging
       /\ merging' = merging \cup {b}
  /\ UNCHANGED <<stored, marks, owner, pc, fresh, heldAt, toPut, writing,
                 swept, heldContend>>

\* A merge in flight deletes the mark.
MergeEnd ==
  /\ \E b \in merging :
       /\ merging' = merging \ {b}
       /\ marks' = marks \ {b}
  /\ UNCHANGED <<stored, owner, pc, fresh, heldAt, toPut, writing, swept,
                 heldContend>>

\* The orphan sweep deletes an unmerged block nobody is writing.
Sweep ==
  /\ \E b \in marks \cap stored :
       /\ writing[b] = {}
       /\ b \notin merging
       /\ stored' = stored \ {b}
       /\ marks' = marks \ {b}
       /\ swept' = swept \cup {b}
  /\ UNCHANGED <<owner, pc, fresh, heldAt, toPut, writing, merging, heldContend>>

\* Once nothing can move, it stays that way.
Idle ==
  /\ \A i \in Ingests : pc[i] \in {"done", "contended"}
  /\ UNCHANGED vars

\* One step of either ingest, the merge or the sweep.
Next ==
  \/ \E i \in Ingests : Filter(i) \/ Own(i) \/ PutBegin(i) \/ PutEnd(i) \/ Release(i)
  \/ MergeBegin \/ MergeEnd \/ Sweep \/ Idle

\* Everything that can happen.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  /\ stored \subseteq Blocks
  /\ marks \subseteq Blocks
  /\ owner \in [Blocks -> {0} \cup Ingests]
  /\ merging \subseteq Blocks

\* At most one writer touches a block's mark at a time.
INV_OneMarkWriter ==
  \A b \in Blocks : Cardinality(writing[b]) + (IF b \in merging THEN 1 ELSE 0) <= 1

\* No ingest gives up over a block it already held.
INV_HeldNeverContends == ~heldContend

\* A finished ingest left each of its blocks stored, or the sweep took it, in
\* which case the merge's missing-link check asks for it again.
INV_Lands ==
  \A i \in Ingests : pc[i] = "done" => Resp(i) \subseteq stored \cup swept
=============================================================================
