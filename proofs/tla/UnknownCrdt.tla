---- MODULE UnknownCrdt ----
\* The story: a field can use a custom CRDT type that a node registers on its
\* builder. Node A has the type (a max register: the field's value is the
\* greatest any write gave it). Node B does not, until it is built again with
\* the type registered. A writes documents; each write is one block that may
\* set the custom field and always adds one to a native counter field beside
\* it. A pushes every block B has not acknowledged, in any order.
\*
\* B without the type cannot know how the custom field merges, so it does not
\* merge that part: it merges the counter, keeps the custom block, writes a
\* record naming it (parks it), and acknowledges the block. A block without a
\* custom part merges as usual. When B is built with the type, it merges each
\* parked block's custom part, removing its record in the same step.
\*
\* The operator may also build B once more without the type after it had it.
\* B refuses to start that way once it stores a value of the type: without
\* the type, a rewrite of the document would drop that value for good.
\*
\* Who acts: A (writing blocks), the network (delivering a block to B), the
\* operator (building B with or without the type), B (merging a parked block,
\* rewriting a document whole).
\*
\* What must never happen: B, without the type, merges a custom part; a
\* custom part merges twice; B's counter counts a block twice or misses one;
\* once B has the type and has merged everything it holds, B differs from A.
\*
\* Code it models:
\*   crates/crdt/src/field/registry.rs          Crdts::delta (a custom block names its type; FieldCrdtError::Unregistered), Crdts::field, Crdts::part
\*   crates/db/src/merge/merge_handler/field.rs merge_field_delta_in_txn (Unregistered -> a FieldMergeResult with `parked`)
\*   crates/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks (the other fields merge; the parked block's head waits)
\*   crates/db/src/merge/merge_handler/parked.rs park (the record, keyed by collection, document, field and block), replay_parked and replay_in_txn (merge, then remove the record, in one transaction)
\*   crates/db/src/merge/merge_handler/parked.rs DB::start_parked_replay (a background task started when a node is built with its types, paging through the records)
\*   crates/db/src/database/stored_crdts.rs     record (the stored-type record) and DB::check_stored_crdts (the refusal to open without a stored type)
\*   crates/defra-node/tests/custom_crdt/replication.rs (the same story between real nodes)
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxBlocks, \* how many blocks A writes at most
  MaxVal,    \* the greatest value a write gives the custom field
  Mode       \* how B handles a custom block without the type:
             \*   "Park"         keeps a record, merges it once with the type (the code)
             \*   "DropDeferral" acknowledges the block but keeps no record
             \*   "DoubleReplay" merges a parked block without removing its record
             \*   "Lww"          takes the custom part as a last-write-wins register
             \*   "RemoveStored" lets B start without the type while it stores a value of it

\* A writes at least one block, and a write can give the field a value.
ASSUME MaxBlocks \in Nat \ {0}
ASSUME MaxVal \in Nat \ {0}
\* The mode is one of the five above.
ASSUME Mode \in {"Park", "DropDeferral", "DoubleReplay", "Lww", "RemoveStored"}

\* A block's ID is its place in A's write order.
Ids == 1..MaxBlocks

\* What a block's custom part may be: 0 means it leaves the field alone.
Vals == 0..MaxVal

\* The greater of two numbers: how a max register merges.
Max(x, y) == IF x >= y THEN x ELSE y

VARIABLES
  written,  \* how many blocks A has written
  cval,     \* cval[i]: the custom value block i writes (0: it leaves the field alone)
  cA,       \* A's custom field
  oA,       \* A's counter field
  hasB,     \* B has the type registered
  appliedB, \* blocks whose counter B has merged
  ackB,     \* blocks B has acknowledged, which A stops offering
  parked,   \* blocks B keeps a record of, their custom part not merged yet
  merges,   \* merges[i]: how many times B merged block i's custom part
  cB,       \* B's custom field
  oB,       \* B's counter field
  blind,    \* ghost: B merged a custom part without the type
  removed   \* the operator has built B without the type after B had it

\* Every variable, for stuttering steps.
vars == <<written, cval, cA, oA, hasB, appliedB, ackB, parked, merges, cB, oB, blind, removed>>

\* Every variable holds a value of its kind.
TypeOK ==
  /\ written \in 0..MaxBlocks      \* A writes at most MaxBlocks blocks
  /\ cval \in [Ids -> Vals]        \* each block's custom value is in range
  /\ cA \in Vals                   \* A's field holds one of those values
  /\ oA \in 0..MaxBlocks           \* A's counter counts A's blocks
  /\ hasB \in BOOLEAN              \* B has the type or not
  /\ appliedB \subseteq Ids        \* B merged the counters of some blocks
  /\ ackB \subseteq Ids            \* B acknowledged some blocks
  /\ parked \subseteq Ids          \* B parked some blocks
  /\ merges \in [Ids -> Nat]       \* a count per block
  /\ cB \in Vals                   \* B's field holds a value in range
  /\ oB \in Nat                    \* B's counter is a number
  /\ blind \in BOOLEAN             \* the ghost is a flag
  /\ removed \in BOOLEAN           \* the removal happened or not

\* Nothing written yet; B starts without the type.
Init ==
  /\ written = 0                   \* no block yet
  /\ cval = [i \in Ids |-> 0]      \* no custom value yet
  /\ cA = 0                        \* A's field starts empty
  /\ oA = 0                        \* A's counter starts at zero
  /\ hasB = FALSE                  \* B lacks the type
  /\ appliedB = {}                 \* B merged nothing
  /\ ackB = {}                     \* B acknowledged nothing
  /\ parked = {}                   \* B parked nothing
  /\ merges = [i \in Ids |-> 0]    \* no custom part merged
  /\ cB = 0                        \* B's field starts empty
  /\ oB = 0                        \* B's counter starts at zero
  /\ blind = FALSE                 \* no blind merge yet
  /\ removed = FALSE               \* B was never built without a type it had

\* A writes block i, giving the custom field v (or leaving it alone, v = 0) and
\* adding one to the counter. A has the type, so it applies its own write.
Write(v) ==
  /\ written < MaxBlocks                         \* A has blocks left to write
  /\ written' = written + 1                      \* the next block
  /\ cval' = [cval EXCEPT ![written + 1] = v]    \* carries v
  /\ cA' = Max(cA, v)                            \* A's register keeps the greatest
  /\ oA' = oA + 1                                \* A's counter goes up
  /\ UNCHANGED <<hasB, appliedB, ackB, parked, merges, cB, oB, blind, removed>>

\* B merges block i's counter, once, and acknowledges the block.
MergeCounter(i) ==
  /\ oB' = IF i \in appliedB THEN oB ELSE oB + 1 \* a block already counted counts nothing
  /\ appliedB' = appliedB \cup {i}               \* the block's counter is merged
  /\ ackB' = ackB \cup {i}                       \* A stops offering it

\* A pushes block i, which B has not acknowledged, and B handles it.
Deliver(i) ==
  /\ i \in 1..written                            \* A wrote it
  /\ i \notin ackB                               \* B has not acknowledged it
  /\ MergeCounter(i)                             \* the counter always merges
  /\ IF cval[i] = 0
     THEN \* No custom part: an ordinary merge.
          UNCHANGED <<parked, merges, cB, blind>>
     ELSE IF hasB
     THEN \* B has the type: the custom part merges by its rule.
          /\ cB' = Max(cB, cval[i])
          /\ merges' = [merges EXCEPT ![i] = @ + 1]
          /\ UNCHANGED <<parked, blind>>
     ELSE CASE Mode \in {"Park", "DoubleReplay", "RemoveStored"} ->
               \* The code: the custom block is kept and a record names it.
               /\ parked' = parked \cup {i}
               /\ UNCHANGED <<merges, cB, blind>>
          [] Mode = "DropDeferral" ->
               \* Bug: no record, so nothing ever merges the custom part.
               UNCHANGED <<parked, merges, cB, blind>>
          [] Mode = "Lww" ->
               \* Bug: the unknown part is taken as a register whose last
               \* arrival wins.
               /\ cB' = cval[i]
               /\ merges' = [merges EXCEPT ![i] = @ + 1]
               /\ blind' = TRUE
               /\ UNCHANGED parked
  /\ UNCHANGED <<written, cval, cA, oA, hasB, removed>>

\* B is built again with the type registered.
Register ==
  /\ ~hasB                                       \* B lacked it
  /\ hasB' = TRUE                                \* and now has it
  /\ UNCHANGED <<written, cval, cA, oA, appliedB, ackB, parked, merges, cB, oB, blind, removed>>

\* B stores a value of the type: some custom part merged, so its record exists.
Stored == \E i \in Ids : merges[i] > 0

\* The operator builds B once more without the type. The code refuses the
\* build while B stores a value of the type; the RED lets it start.
Unregister ==
  /\ hasB                                        \* B has the type now
  /\ ~removed                                    \* the operator does this once
  /\ Mode = "RemoveStored" \/ ~Stored             \* the refusal, unless the RED drops it
  /\ hasB' = FALSE                               \* B runs without the type
  /\ removed' = TRUE                             \* and it happened
  /\ UNCHANGED <<written, cval, cA, oA, appliedB, ackB, parked, merges, cB, oB, blind>>

\* B, without the type, rewrites a document whole (a migration, a structural
\* put): a part of a type it lacks is not kept, so the custom field is lost.
Rewrite ==
  /\ ~hasB                                       \* only without the type
  /\ cB # 0                                      \* there is a value to lose
  /\ cB' = 0                                     \* and it is gone
  /\ UNCHANGED <<written, cval, cA, oA, hasB, appliedB, ackB, parked, merges, oB, blind, removed>>

\* B, with the type, merges the parked block i's custom part. In the code the
\* merge and the removal of the record commit together.
Replay(i) ==
  /\ hasB                                        \* only with the type
  /\ i \in parked                                \* a block B parked
  /\ cB' = Max(cB, cval[i])                      \* merges by the type's rule
  /\ merges' = [merges EXCEPT ![i] = @ + 1]      \* one more merge of it
  /\ parked' = IF Mode = "DoubleReplay" THEN parked ELSE parked \ {i}
  /\ UNCHANGED <<written, cval, cA, oA, hasB, appliedB, ackB, oB, blind, removed>>

\* Any one of these steps happens next.
Next ==
  \/ \E v \in Vals : Write(v)
  \/ \E i \in Ids : Deliver(i)
  \/ Register
  \/ Unregister
  \/ Rewrite
  \/ \E i \in Ids : Replay(i)

\* A keeps offering what B has not acknowledged, B is eventually built with
\* the type, and B merges what it parked.
Fairness ==
  /\ \A i \in Ids : WF_vars(Deliver(i))
  /\ WF_vars(Register)
  /\ \A i \in Ids : WF_vars(Replay(i))

\* Every behaviour from the start, step by step.
Spec == Init /\ [][Next]_vars

\* Those behaviours where nothing waits forever.
FairSpec == Spec /\ Fairness

\* B never merges a custom part it has no type for.
INV_NoBlindMerge == ~blind

\* Each block's custom part merges at most once.
INV_MergeOnce == \A i \in Ids : merges[i] <= 1

\* B's counter counts exactly the blocks it merged, each once.
INV_OtherFieldsExact == oB = Cardinality(appliedB)

\* Once B has the type, has acknowledged every block and has merged everything
\* it parked, it equals A.
INV_ConvergeWhenDone ==
  (hasB /\ 1..written \subseteq ackB /\ parked = {}) => (cB = cA /\ oB = oA)

\* Once A stops writing, B ends up equal to A, and stays so.
LIVE_Converge == <>[](cB = cA /\ oB = oA)
====
