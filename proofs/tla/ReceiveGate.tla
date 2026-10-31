----------------------------- MODULE ReceiveGate -----------------------------
\* The story. Peers push document heads to a node. The node may have a
\* replication filter, and it checks each pushed document against it twice
\* before the merge: first on what it already
\* has when the push arrives (the head block, any CAR sent with it, its own
\* store), before it stores the head or fetches anything; and again on the
\* CAR it fetches, before it imports that CAR's blocks. The merge checks once
\* more, as a backstop, for documents neither check could judge (no CAR came
\* and nobody served one, so the blocks came from the slow DAG walk).
\*
\* Each document either should be refused or kept ("truth"), and whether the
\* deciding value is at hand when it arrives, or only in its CAR, is chosen
\* by the world.
\*
\* What must never happen:
\*   - a document refused on arrival leaves any block behind;
\*   - a document refused after its CAR is fetched has that CAR's blocks
\*     imported;
\*   - a document that should be refused is merged;
\*   - a document that should be kept is refused.
\*
\* Rust: crates/p2p/src/sync/coordinator/receive_gate.rs
\* SyncCoordinator::gate_arrival (the first check, called from the gossip
\* and pushlog handlers before the manager stores the head) and
\* SyncCoordinator::gate_fetched (the second check, called from
\* event_handler/car.rs before a fetched CAR is imported);
\* crates/db/src/receive_gate.rs DbReceiveGate::allows (both checks); the
\* merge's own filter check in
\* crates/db/src/merge/merge_handler/composite_fields.rs (the backstop).
EXTENDS Naturals

\* Each Bug value removes exactly one guard, so its RED config must fail on
\* the invariant named next to it. "none" keeps every guard (GREEN).
\*   "storefirst" store the head before the first check -> INV_ArrivalRefusalStoresNothing
\*   "noafter"    import a fetched CAR unchecked         -> INV_FetchRefusalImportsNothing
\*   "nobackstop" merge without the merge-time check     -> INV_RefusedNeverMerged
\*   "eager"      refuse when the value is not at hand    -> INV_KeptIsKept
CONSTANT Bug

\* Two documents are enough: each can be refused or kept, with its deciding
\* value at hand on arrival or not, and its CAR served or not.
Docs == {"a", "b"}

\* truth: per document, what a check that sees its deciding value says.
\* atHand: per document, whether that value is at hand on arrival (the head,
\*   a CAR sent with it, or the local store hold it).
\* served: per document, whether a peer serves its CAR when asked.
\* phase: per document, where it is: "pushed" (not arrived yet), "pending"
\*   (head stored, CAR to fetch), "ready" (every block here, merge next),
\*   "merged", or "refused".
\* head, rest: per document, whether its head block, and the rest of its
\*   blocks, are stored.
\* refusedAt: per document, which check refused it: "none", "arrival",
\*   "fetch" or "merge".
VARIABLES truth, atHand, served, phase, head, rest, refusedAt

\* Every variable, for UNCHANGED.
vars == <<truth, atHand, served, phase, head, rest, refusedAt>>

\* The world picks each document's truth, what is at hand and who serves.
Init ==
  /\ truth \in [Docs -> {"keep", "refuse"}]
  /\ atHand \in [Docs -> BOOLEAN]
  /\ served \in [Docs -> BOOLEAN]
  /\ phase = [d \in Docs |-> "pushed"]
  /\ head = [d \in Docs |-> FALSE]
  /\ rest = [d \in Docs |-> FALSE]
  /\ refusedAt = [d \in Docs |-> "none"]

\* What a check says when it can see the deciding value: the truth. When it
\* cannot, it lets the document go on (the "eager" bug refuses it instead).
Says(d, sees) ==
  IF sees THEN truth[d]
  ELSE IF Bug = "eager" THEN "refuse" ELSE "keep"

\* Document d arrives. The first check runs before anything is stored (the
\* "storefirst" bug stores the head first). Refused, it is done; kept, its
\* head is stored and its CAR is to be fetched.
Arrive(d) ==
  /\ phase[d] = "pushed"
  /\ IF Says(d, atHand[d]) = "refuse"
     THEN /\ phase' = [phase EXCEPT ![d] = "refused"]
          /\ refusedAt' = [refusedAt EXCEPT ![d] = "arrival"]
          /\ head' = [head EXCEPT ![d] = (Bug = "storefirst")]
     ELSE /\ phase' = [phase EXCEPT ![d] = "pending"]
          /\ head' = [head EXCEPT ![d] = TRUE]
          /\ UNCHANGED refusedAt
  /\ UNCHANGED <<truth, atHand, served, rest>>

\* Its CAR is fetched. The CAR holds the deciding value, so the second check
\* sees it; refused, nothing of the CAR is imported (the "noafter" bug skips
\* the check and imports it). Kept, its blocks are imported.
FetchCar(d) ==
  /\ phase[d] = "pending" /\ served[d]
  /\ IF Bug # "noafter" /\ Says(d, TRUE) = "refuse"
     THEN /\ phase' = [phase EXCEPT ![d] = "refused"]
          /\ refusedAt' = [refusedAt EXCEPT ![d] = "fetch"]
          /\ UNCHANGED rest
     ELSE /\ phase' = [phase EXCEPT ![d] = "ready"]
          /\ rest' = [rest EXCEPT ![d] = TRUE]
          /\ UNCHANGED refusedAt
  /\ UNCHANGED <<truth, atHand, served, head>>

\* Nobody serves its CAR: the slow DAG walk brings its blocks, unchecked.
Walk(d) ==
  /\ phase[d] = "pending" /\ ~served[d]
  /\ phase' = [phase EXCEPT ![d] = "ready"]
  /\ rest' = [rest EXCEPT ![d] = TRUE]
  /\ UNCHANGED <<truth, atHand, served, head, refusedAt>>

\* The merge checks the document as merged, seeing every value (the
\* "nobackstop" bug merges it unchecked).
Merge(d) ==
  /\ phase[d] = "ready"
  /\ IF Bug # "nobackstop" /\ truth[d] = "refuse"
     THEN /\ phase' = [phase EXCEPT ![d] = "refused"]
          /\ refusedAt' = [refusedAt EXCEPT ![d] = "merge"]
     ELSE /\ phase' = [phase EXCEPT ![d] = "merged"]
          /\ UNCHANGED refusedAt
  /\ UNCHANGED <<truth, atHand, served, head, rest>>

\* Every step the node can take.
Next == \E d \in Docs : Arrive(d) \/ FetchCar(d) \/ Walk(d) \/ Merge(d)

\* A document refused on arrival left no block behind.
INV_ArrivalRefusalStoresNothing ==
  \A d \in Docs : refusedAt[d] = "arrival" => ~head[d] /\ ~rest[d]

\* A document that should be refused and whose CAR a peer serves never has
\* blocks imported: a check refuses it on arrival or on its fetched CAR.
INV_FetchRefusalImportsNothing ==
  \A d \in Docs : truth[d] = "refuse" /\ served[d] => ~rest[d]

\* A document that should be refused is never merged.
INV_RefusedNeverMerged == \A d \in Docs : truth[d] = "refuse" => phase[d] # "merged"

\* A document that should be kept is never refused.
INV_KeptIsKept == \A d \in Docs : truth[d] = "keep" => phase[d] # "refused"

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
