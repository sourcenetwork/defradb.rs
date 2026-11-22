---------------------- MODULE LiveMarkerGeneration ----------------------
\* Story. One document is replicated to one peer. Two live writes each commit
\* a new head of it and register the document's retry marker; at the same time
\* the existing-documents replay of a fresh install registers a marker for the
\* same document. Each registration takes a generation from one shared
\* counter outside its transaction, so a later allocation can commit first,
\* and the write with the newer head can hold the older generation. A marker
\* is written only when no newer generation holds the scope. A live write
\* whose generation a newer marker supersedes is owed by that marker: its push
\* still goes out, at its own older generation, and nothing reports it
\* undurable. An acknowledgement clears the marker only at its own generation
\* (the generation fence), and only if, when its push landed, its head was
\* still the document's whole frontier (the head fence). Without the head
\* fence, the writer with the older head and the newer generation clears the
\* marker its newer head still needs.
\* The replay reads the heads before it allocates and, once its push lands,
\* clears its marker only if those heads are still current. The retry sweep
\* sends the current heads of any marked scope and clears it.
\* Never: a live write that committed reported as undurable (#2902, #2791),
\* or a live head neither delivered nor held by a marker.
\* Heads are numbered by commit: a later head descends from every earlier one,
\* so delivering head n delivers every head before it too.
\* Rust: crates/engine/storage/src/stores/peerstore.rs register_scope_marker
\* (the live path behind observe_push_head_with_generation, which maps
\* ReplayMarkerWrite::Superseded to its own generation) and
\* RegisterScopeMarkerAttempt; crates/database/db/src/merge/push_docs.rs
\* push_existing_docs_with_config_and_allowlist (observe_push_head_for_incarnation,
\* then complete_retry_scope after checking the heads it sent are current);
\* crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job (the
\* live ack, fenced by is_whole_frontier on both the current and the retired
\* branch) and report_push_ack; crates/engine/storage/src/stores/peerstore.rs
\* Attempt::CompleteMarker (the clear, fenced by the job's marker generation).
EXTENDS Naturals
\* LiveMode: "covered" treats a superseded live write as owed by the newer
\* marker (the design); "fail" reports it undurable (main before #2902).
\* Fence: a live acknowledgement clears the marker only at its own generation.
\* HeadFence: a live acknowledgement is sent only if the head it delivered was
\* still the whole frontier when its push landed (FALSE is the retired
\* branch on main, which acked on the generation alone).
CONSTANTS LiveMode, Fence, HeadFence
\* The two live writers.
W == {1, 2}
\* next: the shared marker-generation allocator.
\* heads: the newest committed head, 0 before any live write commits.
\* delivered: the newest head the peer holds, 0 for none.
\* marker: the document's marker generation, 0 when none.
\* live[w]: writer w's step; ver[w] the head it committed; liveGen[w] its
\*   generation; front[w] whether its head was the whole frontier when its
\*   push landed.
\* replay: the replay's step; rHeads the heads it read; rGen its generation.
VARIABLES next, heads, delivered, marker, live, ver, liveGen, front, replay, rHeads, rGen
\* Every variable, for stuttering steps.
vars == <<next, heads, delivered, marker, live, ver, liveGen, front, replay, rHeads, rGen>>
\* The variables only the replay changes.
replayVars == <<replay, rHeads, rGen>>

\* The larger of two numbers: delivering an older head after a newer one
\* leaves the peer at the newer.
Max(a, b) == IF a > b THEN a ELSE b

\* Nothing committed, nothing marked, every writer and the replay idle.
Init == /\ next = 0 /\ heads = 0 /\ delivered = 0 /\ marker = 0
        /\ live = [w \in W |-> "idle"] /\ ver = [w \in W |-> 0]
        /\ liveGen = [w \in W |-> 0]
        /\ front = [w \in W |-> FALSE]
        /\ replay = "idle" /\ rHeads = 0 /\ rGen = 0

\* Writer w commits the next head.
LiveCommit(w) ==
  /\ live[w] = "idle"
  /\ heads' = heads + 1
  /\ ver' = [ver EXCEPT ![w] = heads + 1]
  /\ live' = [live EXCEPT ![w] = "alloc"]
  /\ UNCHANGED <<next, delivered, marker, liveGen, front>> /\ UNCHANGED replayVars

\* Writer w's registration allocates its generation outside its transaction.
LiveAlloc(w) ==
  /\ live[w] = "alloc"
  /\ next' = next + 1
  /\ liveGen' = [liveGen EXCEPT ![w] = next + 1]
  /\ live' = [live EXCEPT ![w] = "write"]
  /\ UNCHANGED <<heads, delivered, marker, ver, front>> /\ UNCHANGED replayVars

\* Writer w's registration transaction: it writes its marker unless a newer
\* generation holds the scope. Superseded, the design counts it as owed by
\* that marker and pushes anyway at its own generation; main failed it.
LiveWrite(w) ==
  /\ live[w] = "write"
  /\ IF marker > liveGen[w]
     THEN /\ live' = [live EXCEPT ![w] = IF LiveMode = "covered" THEN "pushing" ELSE "failed"]
          /\ UNCHANGED marker
     \* Not superseded: the marker is this writer's, and its push goes out.
     ELSE /\ marker' = liveGen[w]
          /\ live' = [live EXCEPT ![w] = "pushing"]
  /\ UNCHANGED <<next, heads, delivered, ver, liveGen, front>> /\ UNCHANGED replayVars

\* Writer w's push lands with its head, and the worker reads the document's
\* heads (is_whole_frontier) outside any transaction.
LiveDeliver(w) ==
  /\ live[w] = "pushing"
  /\ delivered' = Max(delivered, ver[w])
  /\ front' = [front EXCEPT ![w] = (heads = ver[w])]
  /\ live' = [live EXCEPT ![w] = "landed"]
  /\ UNCHANGED <<next, heads, marker, ver, liveGen>> /\ UNCHANGED replayVars

\* Writer w's ack, in a later transaction. With the head fence it is sent only
\* if its head was the whole frontier; with the generation fence it clears the
\* marker only at its own generation, and without it whatever marker is there.
LiveAck(w) ==
  /\ live[w] = "landed"
  /\ live' = [live EXCEPT ![w] = "acked"]
  /\ marker' = IF (front[w] \/ ~HeadFence) /\ (marker = liveGen[w] \/ ~Fence)
               THEN 0 ELSE marker
  /\ UNCHANGED <<next, heads, delivered, ver, liveGen, front>> /\ UNCHANGED replayVars

\* Writer w's push is lost; only a marker can bring its head back.
LiveLost(w) ==
  /\ live[w] = "pushing"
  /\ live' = [live EXCEPT ![w] = "lost"]
  /\ UNCHANGED <<next, heads, delivered, marker, ver, liveGen, front>> /\ UNCHANGED replayVars

\* The replay reads the document's heads, then allocates its generation.
ReplayRead == /\ replay = "idle"
              /\ rHeads' = heads /\ replay' = "alloc"
              /\ UNCHANGED <<next, heads, delivered, marker, live, ver, liveGen, front, rGen>>

\* The replay's generation, allocated outside its transaction.
ReplayAlloc == /\ replay = "alloc"
               /\ next' = next + 1 /\ rGen' = next + 1 /\ replay' = "write"
               /\ UNCHANGED <<heads, delivered, marker, live, ver, liveGen, front, rHeads>>

\* A newer marker already owes the document, so the replay moves on.
ReplayWrite == /\ replay = "write"
               /\ IF marker > rGen
                  THEN /\ replay' = "done" /\ UNCHANGED marker
                  ELSE /\ marker' = rGen /\ replay' = "pushing"
               /\ UNCHANGED <<next, heads, delivered, live, ver, liveGen, front, rHeads, rGen>>

\* The replay's push lands with the heads it read.
ReplayPush == /\ replay = "pushing"
              /\ delivered' = Max(delivered, rHeads) /\ replay' = "verify"
              /\ UNCHANGED <<next, heads, marker, live, ver, liveGen, front, rHeads, rGen>>

\* The replay checks the heads it sent are still current ...
ReplayVerify == /\ replay = "verify"
                /\ replay' = IF heads = rHeads THEN "complete" ELSE "done"
                /\ UNCHANGED <<next, heads, delivered, marker, live, ver, liveGen, front, rHeads, rGen>>

\* ... then, in a later transaction, clears the marker at its own generation.
ReplayComplete == /\ replay = "complete"
                  /\ marker' = (IF marker = rGen THEN 0 ELSE marker) /\ replay' = "done"
                  /\ UNCHANGED <<next, heads, delivered, live, ver, liveGen, front, rHeads, rGen>>

\* The retry sweep sends a marked scope's current heads and clears it.
Sweep == /\ marker # 0
         /\ delivered' = Max(delivered, heads) /\ marker' = 0
         /\ UNCHANGED <<next, heads, live, ver, liveGen, front>> /\ UNCHANGED replayVars

\* Any writer's step, any replay step, or the sweep, in any order.
Next == \/ \E w \in W : LiveCommit(w) \/ LiveAlloc(w) \/ LiveWrite(w) \/ LiveDeliver(w) \/ LiveAck(w) \/ LiveLost(w)
        \/ ReplayRead \/ ReplayAlloc \/ ReplayWrite \/ ReplayPush \/ ReplayVerify \/ ReplayComplete
        \/ Sweep

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A live write that committed is never reported as undurable.
INV_NoFalseFailure == \A w \in W : live[w] # "failed"
\* Once a live registration returns, its head is delivered or a marker holds it.
INV_LiveOwed ==
  \A w \in W : live[w] \in {"pushing", "landed", "lost", "acked"} /\ delivered < ver[w] => marker # 0
=============================================================================
