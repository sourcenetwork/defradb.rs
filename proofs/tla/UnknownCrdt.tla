---- MODULE UnknownCrdt ----
\* The story: a field can use a custom CRDT type that a node registers on its
\* builder. Node A has the type (a max register: the field's value is the
\* greatest any write gave it). Node B does not, until it is built again with
\* the type registered. A writes one document; each write is one block that
\* may set the custom field, may add one to a native counter field beside it,
\* and does at least one of the two. A pushes every block B has not
\* acknowledged, in any order.
\*
\* B without the type cannot know how the custom field merges, so it does not
\* merge that part: it merges the counter, keeps the custom block, writes a
\* record naming it (parks it), and acknowledges the block. Either way B now
\* holds the document, even when the block carried nothing B could merge. A
\* block without a custom part merges as usual. When B is built with the
\* type, it merges each parked block's custom part into the document,
\* removing its record in the same step. A merge that cannot finish for now
\* (a store error, a decryption key B does not hold) leaves the block parked
\* for a later try.
\*
\* The operator may also build B once more without the type after it had it.
\* B refuses to start that way once it stores a value of the type: without
\* the type, a rewrite of the document would drop that value for good.
\*
\* Who acts: A (writing blocks), the network (delivering a block to B), the
\* operator (building B with or without the type), B (merging a parked block,
\* failing to merge one for now, rewriting a document whole).
\*
\* What must never happen: B, without the type, merges a custom part; a
\* custom part merges twice; B's counter counts a block twice or misses one;
\* once B has the type and has merged everything it holds, B differs from A,
\* the document missing on B included.
\*
\* Code it models:
\*   crates/crdt/src/field/registry.rs          Crdts::delta (a custom block names its type; FieldCrdtError::Unregistered), Crdts::field, Crdts::part
\*   crates/db/src/merge/merge_handler/field.rs merge_field_delta_in_txn (Unregistered -> a FieldMergeResult with `parked`)
\*   crates/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks (the other fields merge; the parked block's head waits; CompositeMergeState::parked_fields)
\*   crates/db/src/merge/merge_handler/composite_persist.rs persist_merged_document (a composite whose fields were all parked still makes the document: docB)
\*   crates/db/src/merge/merge_handler/parked.rs park (the record, keyed by collection, document, field and block), replay_parked and replay_in_txn (merge into the stored document, then remove the record, in one transaction), error.rs MergeError::fails_block and MergeError::needs_key (a store error or a missing key leaves the block parked: Stall)
\*   crates/db/src/merge/merge_handler/merge_keys.rs with_local_history_keys (a replay decrypts its block's history with this node's own keys, fetched before its transaction; a key held elsewhere is a Stall, never a peer request)
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
             \*   "NoDocument"   makes no document for a block whose only part B parked
             \*   "FailStall"    sets a block aside for good when its merge fails for now

\* A writes at least one block, and a write can give the field a value.
ASSUME MaxBlocks \in Nat \ {0}
ASSUME MaxVal \in Nat \ {0}
\* The mode is one of the five above.
ASSUME Mode \in {"Park", "DropDeferral", "DoubleReplay", "Lww", "RemoveStored",
                 "NoDocument", "FailStall"}

\* A block's ID is its place in A's write order.
Ids == 1..MaxBlocks

\* What a block's custom part may be: 0 means it leaves the field alone.
Vals == 0..MaxVal

\* The greater of two numbers: how a max register merges.
Max(x, y) == IF x >= y THEN x ELSE y

VARIABLES
  written,  \* how many blocks A has written
  cval,     \* cval[i]: the custom value block i writes (0: it leaves the field alone)
  native,   \* native[i]: block i adds one to the counter
  cA,       \* A's custom field
  oA,       \* A's counter field
  hasB,     \* B has the type registered
  appliedB, \* blocks whose counter B has merged
  ackB,     \* blocks B has acknowledged, which A stops offering
  parked,   \* blocks B keeps a record of, their custom part not merged yet
  merges,   \* merges[i]: how many times B merged block i's custom part
  cB,       \* B's custom field
  oB,       \* B's counter field
  docB,     \* B holds the document
  setAside, \* blocks B set aside as unable to merge, never merged
  blind,    \* ghost: B merged a custom part without the type
  removed   \* the operator has built B without the type after B had it

\* Every variable, for stuttering steps.
vars == <<written, cval, native, cA, oA, hasB, appliedB, ackB, parked, merges, cB, oB,
          docB, setAside, blind, removed>>

\* Every variable holds a value of its kind.
TypeOK ==
  /\ written \in 0..MaxBlocks      \* A writes at most MaxBlocks blocks
  /\ cval \in [Ids -> Vals]        \* each block's custom value is in range
  /\ native \in [Ids -> BOOLEAN]   \* each block counts or not
  /\ cA \in Vals                   \* A's field holds one of those values
  /\ oA \in 0..MaxBlocks           \* A's counter counts A's blocks
  /\ hasB \in BOOLEAN              \* B has the type or not
  /\ appliedB \subseteq Ids        \* B merged the counters of some blocks
  /\ ackB \subseteq Ids            \* B acknowledged some blocks
  /\ parked \subseteq Ids          \* B parked some blocks
  /\ merges \in [Ids -> Nat]       \* a count per block
  /\ cB \in Vals                   \* B's field holds a value in range
  /\ oB \in Nat                    \* B's counter is a number
  /\ docB \in BOOLEAN              \* the document is on B or not
  /\ setAside \subseteq Ids        \* B set some blocks aside
  /\ blind \in BOOLEAN             \* the ghost is a flag
  /\ removed \in BOOLEAN           \* the removal happened or not

\* Nothing written yet; B starts without the type.
Init ==
  /\ written = 0                   \* no block yet
  /\ cval = [i \in Ids |-> 0]      \* no custom value yet
  /\ native = [i \in Ids |-> FALSE] \* no counting block yet
  /\ cA = 0                        \* A's field starts empty
  /\ oA = 0                        \* A's counter starts at zero
  /\ hasB = FALSE                  \* B lacks the type
  /\ appliedB = {}                 \* B merged nothing
  /\ ackB = {}                     \* B acknowledged nothing
  /\ parked = {}                   \* B parked nothing
  /\ merges = [i \in Ids |-> 0]    \* no custom part merged
  /\ cB = 0                        \* B's field starts empty
  /\ oB = 0                        \* B's counter starts at zero
  /\ docB = FALSE                  \* B has no document yet
  /\ setAside = {}                 \* nothing set aside
  /\ blind = FALSE                 \* no blind merge yet
  /\ removed = FALSE               \* B was never built without a type it had

\* A writes block i, giving the custom field v (or leaving it alone, v = 0) and
\* adding one to the counter when n holds. A has the type, so it applies its
\* own write.
Write(v, n) ==
  /\ written < MaxBlocks                         \* A has blocks left to write
  /\ v # 0 \/ n                                 \* a block changes something
  /\ written' = written + 1                      \* the next block
  /\ cval' = [cval EXCEPT ![written + 1] = v]    \* carries v
  /\ native' = [native EXCEPT ![written + 1] = n] \* and counts when n holds
  /\ cA' = Max(cA, v)                            \* A's register keeps the greatest
  /\ oA' = IF n THEN oA + 1 ELSE oA              \* A's counter goes up when n holds
  /\ UNCHANGED <<hasB, appliedB, ackB, parked, merges, cB, oB, docB, setAside, blind, removed>>

\* B merges block i's counter, once, and acknowledges the block.
MergeCounter(i) ==
  /\ oB' = IF i \in appliedB \/ ~native[i]      \* a block already counted, or with
          THEN oB ELSE oB + 1                    \* no counter part, counts nothing
  /\ appliedB' = appliedB \cup {i}               \* the block's counter is merged
  /\ ackB' = ackB \cup {i}                       \* A stops offering it

\* A pushes block i, which B has not acknowledged, and B handles it.
Deliver(i) ==
  /\ i \in 1..written                            \* A wrote it
  /\ i \notin ackB                               \* B has not acknowledged it
  /\ MergeCounter(i)                             \* the counter always merges
  \* B holds the document now, unless the only part was parked and the RED
  \* makes no document for it.
  /\ docB' = (docB \/ native[i] \/ hasB \/ cval[i] = 0 \/ Mode # "NoDocument")
  /\ IF cval[i] = 0
     THEN \* No custom part: an ordinary merge.
          UNCHANGED <<parked, merges, cB, blind>>
     ELSE IF hasB
     THEN \* B has the type: the custom part merges by its rule.
          /\ cB' = Max(cB, cval[i])
          /\ merges' = [merges EXCEPT ![i] = @ + 1]
          /\ UNCHANGED <<parked, blind>>
     ELSE CASE Mode \in {"Park", "DoubleReplay", "RemoveStored", "NoDocument", "FailStall"} ->
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
  /\ UNCHANGED <<written, cval, native, cA, oA, hasB, setAside, removed>>

\* B is built again with the type registered.
Register ==
  /\ ~hasB                                       \* B lacked it
  /\ hasB' = TRUE                                \* and now has it
  /\ UNCHANGED <<written, cval, native, cA, oA, appliedB, ackB, parked, merges, cB, oB,
                 docB, setAside, blind, removed>>

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
  /\ UNCHANGED <<written, cval, native, cA, oA, appliedB, ackB, parked, merges, cB, oB,
                 docB, setAside, blind>>

\* B, without the type, rewrites a document whole (a migration, a structural
\* put): a part of a type it lacks is not kept, so the custom field is lost.
Rewrite ==
  /\ ~hasB                                       \* only without the type
  /\ cB # 0                                      \* there is a value to lose
  /\ cB' = 0                                     \* and it is gone
  /\ UNCHANGED <<written, cval, native, cA, oA, hasB, appliedB, ackB, parked, merges, oB,
                 docB, setAside, blind, removed>>

\* B, with the type, merges the parked block i's custom part into the document
\* it holds; with no document there is nothing to write it into. In the code
\* the merge and the removal of the record commit together.
Replay(i) ==
  /\ hasB                                        \* only with the type
  /\ i \in parked                                \* a block B parked
  /\ cB' = IF docB THEN Max(cB, cval[i]) ELSE cB \* merges by the type's rule
  /\ merges' = [merges EXCEPT ![i] = @ + 1]      \* one more merge of it
  /\ parked' = IF Mode = "DoubleReplay" THEN parked ELSE parked \ {i}
  /\ UNCHANGED <<written, cval, native, cA, oA, hasB, appliedB, ackB, oB, docB, setAside,
                 blind, removed>>

\* B, with the type, cannot finish merging the parked block i for now: the
\* store failed, or the block's history needs a key B does not hold. The
\* code leaves the block parked for a later try; the RED sets it aside.
Stall(i) ==
  /\ hasB                                        \* only a replay meets this
  /\ i \in parked                                \* a block B parked
  /\ Mode = "FailStall"                          \* the code changes nothing here
  /\ parked' = parked \ {i}                      \* the RED drops its record
  /\ setAside' = setAside \cup {i}               \* and never merges it
  /\ UNCHANGED <<written, cval, native, cA, oA, hasB, appliedB, ackB, merges, cB, oB, docB,
                 blind, removed>>

\* Any one of these steps happens next.
Next ==
  \/ \E v \in Vals, n \in BOOLEAN : Write(v, n)
  \/ \E i \in Ids : Deliver(i)
  \/ Register
  \/ Unregister
  \/ Rewrite
  \/ \E i \in Ids : Replay(i)
  \/ \E i \in Ids : Stall(i)

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

\* B's counter counts exactly the counting blocks it merged, each once.
INV_OtherFieldsExact == oB = Cardinality({i \in appliedB : native[i]})

\* Once B has the type, has acknowledged every block and has merged everything
\* it parked, it equals A, and holds the document once A has written one.
INV_ConvergeWhenDone ==
  (hasB /\ 1..written \subseteq ackB /\ parked = {})
    => (cB = cA /\ oB = oA /\ docB = (written > 0))

\* Once A stops writing, B ends up equal to A, and stays so.
LIVE_Converge == <>[](cB = cA /\ oB = oA /\ docB = (written > 0))
====
