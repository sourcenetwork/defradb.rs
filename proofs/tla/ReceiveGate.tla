----------------------------- MODULE ReceiveGate -----------------------------
\* The story. Peers push document heads to a node. The node may have a
\* replication filter and a retention floor, and it checks each pushed
\* document against them before the merge: first on what it already has when
\* the push arrives (the head block, any CAR sent with it, its own store),
\* before it stores the head or fetches anything; then on the CAR it fetches,
\* before it imports that CAR's blocks. When nobody serves the CAR, the node
\* would walk the document block by block; before that walk it fetches only
\* the document's height block and checks again on it. Documents at one
\* height share that block, and it is asked for once. The merge checks once
\* more, as a backstop, for documents no check could judge.
\*
\* Each document either should be refused or kept ("truth"); whether the
\* deciding value is at hand on arrival, whether a peer serves its CAR, and
\* whether a peer serves its height block are chosen by the world.
\*
\* What must never happen:
\*   - a document refused on arrival leaves any block behind;
\*   - a document refused after its CAR is fetched has that CAR's blocks
\*     imported;
\*   - a document refused on its height block is walked;
\*   - a height block is asked of the network twice;
\*   - a document that should be refused is merged;
\*   - a document that should be kept is refused.
\*
\* Rust: crates/p2p/src/sync/coordinator/receive_gate.rs
\* SyncCoordinator::gate_arrival (the first check, called from the gossip
\* and pushlog handlers before the manager stores the head) and
\* SyncCoordinator::gate_fetched (the second check, called from
\* event_handler/car.rs before a fetched CAR is imported);
\* crates/p2p/src/sync/coordinator/dag_fetcher.rs height_refusal (the height
\* block fetched alone, its owner taken in HeightGate::fetches, and the check
\* on it, before fetch_dag_attempt's walk); crates/db/src/receive_gate.rs
\* DbReceiveGate::judge and DbReceiveGate::height_link; the merge's own
\* filter and retention checks in crates/db/src/merge/merge_handler (the
\* backstop).
EXTENDS Naturals

\* Each Bug value removes exactly one guard, so its RED config must fail on
\* the invariant named next to it. "none" keeps every guard (GREEN).
\*   "storefirst" store the head before the first check -> INV_ArrivalRefusalStoresNothing
\*   "noafter"    import a fetched CAR unchecked         -> INV_FetchRefusalImportsNothing
\*   "noheight"   walk without checking the height block -> INV_HeightRefusalWalksNothing
\*   "refetch"    ask for a shared height block again    -> INV_HeightAskedOnce
\*   "nobackstop" merge without the merge-time check     -> INV_RefusedNeverMerged
\*   "eager"      refuse when the value is not at hand    -> INV_KeptIsKept
CONSTANT Bug

\* Two documents are enough: each can be refused or kept, with its deciding
\* value at hand on arrival or not, its CAR served or not, and its height
\* block served or not; and the two can share one height block.
Docs == {"a", "b"}

\* The height blocks: each document's own, or "h", the one they share.
Blocks == Docs \cup {"h"}

\* truth: per document, what a check that sees its deciding value says.
\* atHand: per document, whether that value is at hand on arrival (the head,
\*   a CAR sent with it, or the local store hold it).
\* served: per document, whether a peer serves its CAR when asked.
\* shared: whether the two documents share one height block, "h".
\* heightServed: per height block, whether a peer serves it when asked.
\* phase: per document, where it is: "pushed" (not arrived yet), "pending"
\*   (head stored, CAR to fetch), "height" (no CAR came, its height block is
\*   asked for or awaited), "judging" (the height block asked, the check on
\*   it next), "ready" (every block here, merge next), "merged", or
\*   "refused".
\* head, rest: per document, whether its head block, and the rest of its
\*   blocks, are stored.
\* held: per height block, whether it is stored.
\* asks: per height block, how many times it was asked of the network.
\* refusedAt: per document, which check refused it: "none", "arrival",
\*   "fetch", "height" or "merge".
VARIABLES truth, atHand, served, shared, heightServed, phase, head, rest,
          held, asks, refusedAt

\* Every variable, for UNCHANGED.
vars == <<truth, atHand, served, shared, heightServed, phase, head, rest,
          held, asks, refusedAt>>

\* The height block document d links: the shared one, or its own.
HeightOf(d) == IF shared THEN "h" ELSE d

\* The world picks each document's truth, what is at hand and who serves.
\* Documents sharing a height block are at one height, so one truth.
Init ==
  /\ shared \in BOOLEAN
  /\ truth \in [Docs -> {"keep", "refuse"}]
  /\ shared => truth["a"] = truth["b"]
  /\ atHand \in [Docs -> BOOLEAN]
  /\ served \in [Docs -> BOOLEAN]
  /\ heightServed \in [Blocks -> BOOLEAN]
  /\ phase = [d \in Docs |-> "pushed"]
  /\ head = [d \in Docs |-> FALSE]
  /\ rest = [d \in Docs |-> FALSE]
  /\ held = [b \in Blocks |-> FALSE]
  /\ asks = [b \in Blocks |-> 0]
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
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, rest, held, asks>>

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
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, held, asks>>

\* Nobody serves its CAR: the document now needs its height block.
NoCar(d) ==
  /\ phase[d] = "pending" /\ ~served[d]
  /\ phase' = [phase EXCEPT ![d] = "height"]
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, rest,
                 held, asks, refusedAt>>

\* The height block is not stored and nobody asked for it yet: d takes the
\* block's single-flight owner and asks the network once; it arrives if a
\* peer serves it. The "refetch" bug asks even when another document already
\* has, so a shared block is asked for twice.
AskHeight(d) ==
  LET b == HeightOf(d) IN
  /\ phase[d] = "height"
  /\ ~held[b]
  /\ asks[b] = 0 \/ Bug = "refetch"
  /\ asks' = [asks EXCEPT ![b] = @ + 1]
  /\ held' = [held EXCEPT ![b] = heightServed[b]]
  /\ phase' = [phase EXCEPT ![d] = "judging"]
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, rest,
                 refusedAt>>

\* The height block is stored already, or another document asked for it:
\* d waits on that ask and asks nothing itself.
AwaitHeight(d) ==
  LET b == HeightOf(d) IN
  /\ phase[d] = "height"
  /\ held[b] \/ (asks[b] > 0 /\ Bug # "refetch")
  /\ phase' = [phase EXCEPT ![d] = "judging"]
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, rest,
                 held, asks, refusedAt>>

\* The check on the height block, when it is stored: refused, d is never
\* walked; otherwise the walk brings its blocks (the "noheight" bug walks
\* without checking, leaving the refusal to the merge).
JudgeHeight(d) ==
  /\ phase[d] = "judging"
  /\ IF Bug # "noheight" /\ Says(d, held[HeightOf(d)]) = "refuse"
     THEN /\ phase' = [phase EXCEPT ![d] = "refused"]
          /\ refusedAt' = [refusedAt EXCEPT ![d] = "height"]
          /\ UNCHANGED rest
     ELSE /\ phase' = [phase EXCEPT ![d] = "ready"]
          /\ rest' = [rest EXCEPT ![d] = TRUE]
          /\ UNCHANGED refusedAt
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, held, asks>>

\* The merge checks the document as merged, seeing every value (the
\* "nobackstop" bug merges it unchecked).
Merge(d) ==
  /\ phase[d] = "ready"
  /\ IF Bug # "nobackstop" /\ truth[d] = "refuse"
     THEN /\ phase' = [phase EXCEPT ![d] = "refused"]
          /\ refusedAt' = [refusedAt EXCEPT ![d] = "merge"]
     ELSE /\ phase' = [phase EXCEPT ![d] = "merged"]
          /\ UNCHANGED refusedAt
  /\ UNCHANGED <<truth, atHand, served, shared, heightServed, head, rest,
                 held, asks>>

\* Every step the node can take.
Next == \E d \in Docs :
          \/ Arrive(d) \/ FetchCar(d) \/ NoCar(d) \/ AskHeight(d)
          \/ AwaitHeight(d) \/ JudgeHeight(d) \/ Merge(d)

\* A document refused on arrival left no block behind.
INV_ArrivalRefusalStoresNothing ==
  \A d \in Docs : refusedAt[d] = "arrival" => ~head[d] /\ ~rest[d]

\* A document that should be refused and whose CAR a peer serves never has
\* blocks imported: a check refuses it on arrival or on its fetched CAR.
INV_FetchRefusalImportsNothing ==
  \A d \in Docs : truth[d] = "refuse" /\ served[d] => ~rest[d]

\* A document that should be refused and whose height block a peer serves
\* is never walked: with no CAR, the check on its height block refuses it.
INV_HeightRefusalWalksNothing ==
  \A d \in Docs : truth[d] = "refuse" /\ heightServed[HeightOf(d)] => ~rest[d]

\* Every height block is asked of the network at most once, however many
\* documents link it.
INV_HeightAskedOnce == \A b \in Blocks : asks[b] <= 1

\* A document that should be refused is never merged.
INV_RefusedNeverMerged == \A d \in Docs : truth[d] = "refuse" => phase[d] # "merged"

\* A document that should be kept is never refused.
INV_KeptIsKept == \A d \in Docs : truth[d] = "keep" => phase[d] # "refused"

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
