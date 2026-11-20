---------------------- MODULE LiveMarkerGeneration ----------------------
\* Story. One document is replicated to one peer. A live write commits a new
\* head and registers the document's retry marker; at the same time the
\* existing-documents replay of a fresh install registers a marker for the
\* same document. Each registration takes a generation from one shared
\* counter outside its transaction, so the replay can allocate after the live
\* write and commit first. A marker is written only when no newer generation
\* holds the scope; an acknowledgement clears it only at its own generation.
\* The replay reads the heads before it allocates and, once its push lands,
\* clears its marker only if those heads are still current. The retry sweep
\* sends the current heads of any marked scope and clears it.
\* Never: a live write that committed reported as undurable (#2902, #2791),
\* or the live head neither delivered nor held by a marker.
\* Rust: crates/engine/storage/src/stores/peerstore.rs register_scope_marker_at
\* (the live path behind observe_push_head_with_generation) and
\* RegisterScopeMarkerAttempt; crates/database/db/src/merge/push_docs.rs
\* push_existing_docs_with_config_and_allowlist (observe_push_head_for_incarnation,
\* then complete_retry_scope after checking the heads it sent are current);
\* crates/database/p2p/src/sync/coordinator/push_worker.rs (the live ack,
\* fenced by the job's marker generation).
EXTENDS Naturals
\* LiveMode: "rerun" reruns a superseded live write with a fresh generation
\* (the fix); "fail" reports a superseded live write as undurable (main
\* before #2902); "covered" treats a superseded live write as owed by the
\* newer marker and sends nothing of its own.
\* MaxReruns: the live path's rerun budget (LIVE_MARKER_RERUNS, fixed at 16
\* in the code and independent of max_txn_retries).
CONSTANTS LiveMode, MaxReruns
\* next: the shared marker-generation allocator.
\* heads: "old" until the live write commits, then "new".
\* delivered: the peer holds the "new" head.
\* marker: the document's marker generation, 0 when none.
\* live: the live write's step; liveGen its current generation; reruns its
\*   reruns so far.
\* replay: the replay's step; rHeads the heads it read; rGen its generation.
VARIABLES next, heads, delivered, marker, live, liveGen, reruns, replay, rHeads, rGen
vars == <<next, heads, delivered, marker, live, liveGen, reruns, replay, rHeads, rGen>>

Init == /\ next = 0 /\ heads = "old" /\ delivered = FALSE /\ marker = 0
        /\ live = "idle" /\ liveGen = 0 /\ reruns = 0
        /\ replay = "idle" /\ rHeads = "old" /\ rGen = 0

\* The live write commits its head.
LiveCommit == /\ live = "idle"
              /\ heads' = "new" /\ live' = "alloc"
              /\ UNCHANGED <<next, delivered, marker, liveGen, reruns, replay, rHeads, rGen>>

\* The live registration allocates its generation outside its transaction.
LiveAlloc == /\ live = "alloc"
             /\ next' = next + 1 /\ liveGen' = next + 1 /\ live' = "write"
             /\ UNCHANGED <<heads, delivered, marker, reruns, replay, rHeads, rGen>>

\* The live registration's transaction: it writes its marker unless a newer
\* generation holds the scope. Superseded, the fix reruns with a fresh
\* generation while its budget lasts.
LiveWrite ==
  /\ live = "write"
  /\ IF marker > liveGen
     THEN IF LiveMode = "rerun" /\ reruns < MaxReruns
          THEN /\ next' = next + 1 /\ liveGen' = next + 1 /\ reruns' = reruns + 1
               /\ UNCHANGED <<live, marker>>
          ELSE /\ live' = IF LiveMode = "covered" THEN "covered" ELSE "failed"
               /\ UNCHANGED <<next, liveGen, reruns, marker>>
     ELSE /\ marker' = liveGen /\ live' = "pushing"
          /\ UNCHANGED <<next, liveGen, reruns>>
  /\ UNCHANGED <<heads, delivered, replay, rHeads, rGen>>

\* The live push lands; its ack clears the marker only at its own generation.
LiveAck == /\ live = "pushing"
           /\ delivered' = TRUE /\ live' = "acked"
           /\ marker' = IF marker = liveGen THEN 0 ELSE marker
           /\ UNCHANGED <<next, heads, liveGen, reruns, replay, rHeads, rGen>>

\* The live push is lost; only a marker can bring the head back.
LiveLost == /\ live = "pushing"
            /\ live' = "lost"
            /\ UNCHANGED <<next, heads, delivered, marker, liveGen, reruns, replay, rHeads, rGen>>

\* The replay reads the document's heads, then allocates its generation.
ReplayRead == /\ replay = "idle"
              /\ rHeads' = heads /\ replay' = "alloc"
              /\ UNCHANGED <<next, heads, delivered, marker, live, liveGen, reruns, rGen>>

ReplayAlloc == /\ replay = "alloc"
               /\ next' = next + 1 /\ rGen' = next + 1 /\ replay' = "write"
               /\ UNCHANGED <<heads, delivered, marker, live, liveGen, reruns, rHeads>>

\* A newer marker already owes the document, so the replay moves on.
ReplayWrite == /\ replay = "write"
               /\ IF marker > rGen
                  THEN /\ replay' = "done" /\ UNCHANGED marker
                  ELSE /\ marker' = rGen /\ replay' = "pushing"
               /\ UNCHANGED <<next, heads, delivered, live, liveGen, reruns, rHeads, rGen>>

\* The replay's push lands with the heads it read.
ReplayPush == /\ replay = "pushing"
              /\ delivered' = (delivered \/ rHeads = "new") /\ replay' = "verify"
              /\ UNCHANGED <<next, heads, marker, live, liveGen, reruns, rHeads, rGen>>

\* The replay checks the heads it sent are still current ...
ReplayVerify == /\ replay = "verify"
                /\ replay' = IF heads = rHeads THEN "complete" ELSE "done"
                /\ UNCHANGED <<next, heads, delivered, marker, live, liveGen, reruns, rHeads, rGen>>

\* ... then, in a later transaction, clears the marker at its own generation.
ReplayComplete == /\ replay = "complete"
                  /\ marker' = (IF marker = rGen THEN 0 ELSE marker) /\ replay' = "done"
                  /\ UNCHANGED <<next, heads, delivered, live, liveGen, reruns, rHeads, rGen>>

\* The retry sweep sends a marked scope's current heads and clears it.
Sweep == /\ marker # 0
         /\ delivered' = (delivered \/ heads = "new") /\ marker' = 0
         /\ UNCHANGED <<next, heads, live, liveGen, reruns, replay, rHeads, rGen>>

Next == \/ LiveCommit \/ LiveAlloc \/ LiveWrite \/ LiveAck \/ LiveLost
        \/ ReplayRead \/ ReplayAlloc \/ ReplayWrite \/ ReplayPush \/ ReplayVerify \/ ReplayComplete
        \/ Sweep

Spec == Init /\ [][Next]_vars

\* A live write that committed is never reported as undurable.
INV_NoFalseFailure == live # "failed"
\* Once the live registration returns, its head is delivered or a marker holds it.
INV_LiveOwed == live \in {"pushing", "lost", "covered", "acked"} /\ ~delivered => marker # 0
=============================================================================
