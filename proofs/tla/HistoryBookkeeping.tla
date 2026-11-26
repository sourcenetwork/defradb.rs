------------------------- MODULE HistoryBookkeeping -------------------------
\* The story. A node receives the newest block of a document's history and
\* walks down to the oldest unapplied ancestor, applying each block after its
\* parents. To remember where it is between turns it keeps bookkeeping keys on
\* disk: one stack frame per level of the walk, and one mark per node it has
\* reached. The history here is a small DAG: two branches share ancestors, one
\* ancestor (node 1) was applied before the walk began, and any of nodes 2, 3
\* and 5 may be skipped for good when processed (never applied).
\* What can happen: the walk pushes a parent, pops an ancestor the headstore
\* already records as applied, processes a node whose parents are done, and at
\* the end deletes what bookkeeping remains.
\* What must never happen: the root's progress miscounts its keys; a finished,
\* applied node keeps a mark (bookkeeping then grows with the whole history,
\* not with the open walk); a node is processed twice; bookkeeping outlives the
\* walk. No node count stops a valid walk: there is no cutoff to model.
\* Rust: crates/database/db/src/merge/merge_handler/history_walk.rs
\* (DbMergeHandler::history_page) and history.rs (Progress::keys, cleanup).
EXTENDS Naturals, Sequences, FiniteSets

\* Which guard a RED configuration removes; "none" keeps every guard.
CONSTANT Bug

\* The blocks of the history; 6 is the root that arrived.
Nodes == 1..6
\* The root the walk starts from.
Root == 6
\* Each block's parents, in the order the walk visits them.
Parents == [n \in Nodes |->
              CASE n = 6 -> <<4, 5>>
                [] n = 5 -> <<3, 2>>
                [] n = 4 -> <<3, 2>>
                [] n = 3 -> <<1>>
                [] n = 2 -> <<1>>
                [] n = 1 -> << >>]
\* The blocks processing may skip for good instead of applying.
MaySkip == {2, 3, 5}

\* stack: the open frames, each a node and the index of its next parent.
\* mark: each node's mark key, "none" when no key exists.
\* applied: what the headstore records as applied.
\* processed: how many times each node was processed.
\* keys: the bookkeeping count the root's progress stores.
\* stage: the walk, then its cleanup, then nothing left.
\* skipped: which blocks this run's processing skips, fixed at the start.
VARIABLES stack, mark, applied, processed, keys, stage, skipped
\* All the state in one tuple for UNCHANGED and fairness.
vars == <<stack, mark, applied, processed, keys, stage, skipped>>

\* Admission writes the root's frame and its open mark: two keys.
Init == /\ stack = << [n |-> Root, next |-> 1] >>
        /\ mark = [n \in Nodes |-> IF n = Root THEN "open" ELSE "none"]
        /\ applied = {1}
        /\ processed = [n \in Nodes |-> 0]
        /\ keys = 2
        /\ stage = "walk"
        /\ skipped \in SUBSET MaySkip

\* The frame on top of the stack.
Top == stack[Len(stack)]
\* The stack without its top frame.
Popped == SubSeq(stack, 1, Len(stack) - 1)
\* The stage after popping: the walk ends when the root's frame goes.
AfterPop == IF Len(stack) = 1 THEN "clean" ELSE "walk"

\* The top node is already applied: its frame and its mark are deleted, two
\* keys. With Bug = "keepmark" the mark is rewritten as done instead of
\* deleted, as the old walk did. With Bug = "noaccount" the deletes are not
\* subtracted from the count. With Bug = "noappliedcheck" this step never runs.
PopApplied == /\ stage = "walk"
              /\ Bug # "noappliedcheck"
              /\ Top.n \in applied
              /\ mark' = [mark EXCEPT ![Top.n] = IF Bug = "keepmark" THEN "done" ELSE "none"]
              /\ keys' = CASE Bug = "keepmark" -> keys - 1
                           [] Bug = "noaccount" -> keys
                           [] OTHER -> keys - 2
              /\ stack' = Popped
              /\ stage' = AfterPop
              /\ UNCHANGED <<applied, processed, skipped>>

\* The top node has a parent left to look at; a done parent is passed over.
SkipDone == /\ stage = "walk"
            /\ Top.n \notin applied \/ Bug = "noappliedcheck"
            /\ Top.next <= Len(Parents[Top.n])
            /\ mark[Parents[Top.n][Top.next]] = "done"
            /\ stack' = [stack EXCEPT ![Len(stack)].next = @ + 1]
            /\ UNCHANGED <<mark, applied, processed, keys, stage, skipped>>

\* A parent with no mark is pushed: a new frame and an open mark, two keys.
Push == /\ stage = "walk"
        /\ Top.n \notin applied \/ Bug = "noappliedcheck"
        /\ Top.next <= Len(Parents[Top.n])
        /\ LET p == Parents[Top.n][Top.next] IN
           /\ mark[p] = "none"
           /\ stack' = Append([stack EXCEPT ![Len(stack)].next = @ + 1],
                              [n |-> p, next |-> 1])
           /\ mark' = [mark EXCEPT ![p] = "open"]
        /\ keys' = keys + 2
        /\ UNCHANGED <<applied, processed, stage, skipped>>

\* Every parent is done: the node is processed and its frame deleted. An
\* applied node's mark is deleted too; a skipped node keeps a done mark so a
\* second branch passes it over. With Bug = "dropskipped" the skipped node's
\* mark is deleted as well, so the next branch reaches it again. With
\* Bug = "keepmark" every processed node keeps a done mark, as the old walk did.
Process == /\ stage = "walk"
           /\ Top.n \notin applied \/ Bug = "noappliedcheck"
           /\ Top.next > Len(Parents[Top.n])
           /\ LET n == Top.n
                  keep == \/ n \in skipped /\ Bug # "dropskipped"
                          \/ Bug = "keepmark" IN
              /\ processed' = [processed EXCEPT ![n] = @ + 1]
              /\ applied' = IF n \in skipped THEN applied ELSE applied \cup {n}
              /\ mark' = [mark EXCEPT ![n] = IF keep THEN "done" ELSE "none"]
              /\ keys' = keys - (IF keep THEN 1 ELSE 2)
           /\ stack' = Popped
           /\ stage' = AfterPop
           /\ UNCHANGED skipped

\* After the walk, cleanup deletes every remaining key and the count with them.
Cleanup == /\ stage = "clean"
           /\ mark' = [n \in Nodes |-> "none"]
           /\ keys' = 0
           /\ stage' = "gone"
           /\ UNCHANGED <<stack, applied, processed, skipped>>

\* One step of the walk or its cleanup.
Next == PopApplied \/ SkipDone \/ Push \/ Process \/ Cleanup

\* The walk keeps taking steps until its bookkeeping is gone.
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* The number of keys that exist: every frame and every mark.
Stored == Len(stack) + Cardinality({n \in Nodes : mark[n] # "none"})

\* The progress counts exactly the bookkeeping keys that exist.
INV_Accounted == keys = Stored
\* Only the open walk and the skipped nodes hold marks: an applied node keeps none.
INV_Retention == \A n \in Nodes : mark[n] = "done" => n \in skipped
\* No node is processed twice, applied or skipped.
INV_ProcessedOnce == \A n \in Nodes : processed[n] <= 1
\* Once the walk is over, every block that is not skipped is applied and no key is left.
INV_CleanEnd == stage = "gone" => /\ applied = Nodes \ skipped
                                  /\ keys = 0
                                  /\ Stored = 0
\* The walk always finishes and leaves no bookkeeping.
LIVE_Finishes == <>(stage = "gone")
=============================================================================
