---------------------------- MODULE ExactDagFetch ----------------------------
\* The story. A document's history is a DAG. Its spine is composite blocks,
\* each linking the composites before it (its heads); each composite also
\* carries a field block, and each field block links the field block before
\* it. A provider holds the whole DAG. A receiver holds the shared history
\* and an update of its own the provider never saw, and is told of the
\* provider's new head M, which sits on two branches the receiver lacks.
\*
\* The receiver names a few composites it holds (its haves): a head and its
\* parent. The provider walks the spine from the wanted blocks, highest
\* priority first. A held composite passes "held" to the composites it links;
\* a wanted one is served with its field block and passes "wanted" on. The
\* walk stops once nothing wanted is left. The receiver checks every block it
\* is sent (its CID), stores the new ones, and merges the head only once
\* every block the head reaches is held; otherwise it asks again for what
\* is still missing.
\*
\* In the second world ("hole") the receiver lost one shared composite and
\* its field block. The provider still thinks the receiver holds them, so the
\* receiver must ask for them by name, and a block asked for by name is always
\* served.
\*
\* A provider may also lie: send any blocks, forged ones too, and claim the
\* DAG is done.
\*
\* Only a composite spine is walked. When the provider holds no have, or a
\* wanted block is not a composite (a branchable collection's block), it
\* sends the wanted blocks' closure instead, as it did before haves; that
\* path is CarMissingRegion.lean's fallback_complete, not this walk.
\*
\* What must never happen:
\*   - the receiver merges a head while a block it reaches is missing;
\*   - a forged block is stored;
\*   - an honest provider sends a block the receiver already holds;
\*   - an honest fetch needs more than one request, or two with a hole.
\*
\* Rust: crates/p2p/src/sync/car/region.rs collect_missing_region and Walk
\* (the walk: mark_held, mark_wanted, serve_attachments, the explicit-want
\* rule); crates/p2p/src/sync/coordinator/haves.rs haves_from_heads (a head
\* and its ancestors) and dag_fetcher.rs root_haves (a held root's heads); crates/p2p/src/sync/coordinator/dag_fetcher.rs
\* fetch_dag_attempt (ask again from the missing frontier, merge only when
\* find_all_missing_links is empty); crates/p2p/src/sync/coordinator/
\* event_handler/car.rs handle_car_fetch_response_inner (verify_block_cid on
\* every block, drop_held_blocks before storing).
EXTENDS Naturals, FiniteSets

\* Liar: whether the provider may lie.
\* MaxRounds: how many requests a fetch may make before it gives up.
\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "nohaves"       the provider ignores the haves       -> INV_HeldNotResent
\*   "fifo"          the walk pops out of priority order  -> INV_HeldNotResent
\*   "flipwant"      held overrides a block asked by name -> INV_RoundsBounded
\*   "trustprovider" the receiver merges on the claim     -> INV_MergeComplete
\*   "skipcid"       the receiver skips the CID check     -> INV_NoForged
CONSTANTS Liar, MaxRounds, Bug

\* The composites: C1..C3 shared, B1 and A1 the provider's branches, M the
\* provider's new head over both, L1 the receiver's own update.
Spine == {"C1", "C2", "C3", "B1", "A1", "M", "L1"}

\* The field blocks, one per composite.
Fields == {"f1", "f2", "f3", "fB", "fA", "fM", "fL"}

\* "X" is a forged block: bytes that do not match the CID they claim.
Blocks == Spine \cup Fields \cup {"X"}

\* The new head the receiver was told of.
Root == "M"

\* The composites each composite links: its heads.
Heads(b) ==
  CASE b = "C2" -> {"C1"}
    [] b = "C3" -> {"C2"}
    [] b = "B1" -> {"C2"}
    [] b = "A1" -> {"C3"}
    [] b = "M"  -> {"A1", "B1"}
    [] b = "L1" -> {"C3"}
    [] OTHER    -> {}

\* The field block each composite carries: its attachment.
Att(b) ==
  CASE b = "C1" -> {"f1"}
    [] b = "C2" -> {"f2"}
    [] b = "C3" -> {"f3"}
    [] b = "B1" -> {"fB"}
    [] b = "A1" -> {"fA"}
    [] b = "M"  -> {"fM"}
    [] b = "L1" -> {"fL"}
    [] OTHER    -> {}

\* The field block each field block links: the one before it on its branch.
FieldHeads(b) ==
  CASE b = "f2" -> {"f1"}
    [] b = "f3" -> {"f2"}
    [] b = "fB" -> {"f2"}
    [] b = "fA" -> {"f3"}
    [] b = "fM" -> {"fA", "fB"}
    [] b = "fL" -> {"f3"}
    [] OTHER    -> {}

\* Every link a block has.
Links(b) == Heads(b) \cup Att(b) \cup FieldHeads(b)

\* A block's priority: one above the highest block it links on its chain.
\* B1 and C3 tie, and so do A1 and L1: the walk may pop either first.
Prio(b) ==
  CASE b \in {"C1", "f1"} -> 1
    [] b \in {"C2", "f2"} -> 2
    [] b \in {"C3", "B1", "f3", "fB"} -> 3
    [] b \in {"A1", "L1", "fA", "fL"} -> 4
    [] b \in {"M", "fM"} -> 5
    [] OTHER -> 0

\* Every block a set of blocks reaches, the set included. The DAG is five
\* links deep, so eight rounds of following links reach all of it.
RECURSIVE Reach(_, _)
Reach(S, n) == IF n = 0 THEN S ELSE Reach(S \cup UNION {Links(b) : b \in S}, n - 1)
Closure(S) == Reach(S, 8)

\* The provider holds everything but the receiver's own update and forgeries.
PHas == Blocks \ {"L1", "fL", "X"}

\* The haves the receiver names: its head and that head's parent.
Haves == {"L1", "C3"}

\* What the receiver holds at the start, in each world.
Start(world) ==
  LET shared == {"C1", "C2", "C3", "f1", "f2", "f3", "L1", "fL"}
  IN IF world = "steady" THEN shared ELSE shared \ {"C2", "f2"}

\* rhas: what the receiver holds. round: the request it is on.
\* phase: "walk" (the provider walks), "deliver" (the response is on its way),
\* "done". wants: what this request asks for. col: each block's colour in
\* the walk ("H" held, "W" wanted, "-" not reached). heldBelow: a block asked
\* for by name that a held block also reaches. popped: blocks the walk has
\* taken. served: what the walk will send. merged: the receiver merged M.
\* sentHeld: blocks an honest provider sent that the receiver already held.
\* world: which receiver this is.
VARIABLES rhas, round, phase, wants, col, heldBelow, popped, served, merged,
          sentHeld, world

\* Every variable, for UNCHANGED.
vars == <<rhas, round, phase, wants, col, heldBelow, popped, served, merged,
          sentHeld, world>>

\* The haves the provider holds; the "nohaves" bug ignores them.
HeldSeeds == IF Bug = "nohaves" THEN {} ELSE Haves \cap PHas

\* The colours a walk starts with: the wanted blocks the provider holds are
\* wanted, the haves it holds are held.
StartCol(w) ==
  [b \in Blocks |-> IF b \in w \cap PHas THEN "W"
                    ELSE IF b \in HeldSeeds THEN "H" ELSE "-"]

\* A block asked for by name that is also a have: served, but held below.
StartHeldBelow(w) == [b \in Blocks |-> b \in w /\ b \in HeldSeeds]

\* The blocks still to be served: wanted and not yet taken.
Pending == {b \in Blocks : col[b] = "W" /\ b \notin popped}

\* The blocks the walk may take next: coloured and not yet taken.
Live == {b \in Blocks : col[b] # "-" /\ b \notin popped}

\* The highest priority live blocks; "fifo" takes any live block.
NextPops ==
  IF Bug = "fifo" THEN Live
  ELSE {b \in Live : \A c \in Live : Prio(c) <= Prio(b)}

\* Mark the heads `hs` held. A wanted head not yet taken turns held, unless
\* it was asked for by name: then it stays wanted and is marked held below
\* ("flipwant" turns it held all the same).
MarkHeldCol(hs) ==
  [c \in Blocks |->
     IF c \in hs /\ c \in PHas
        /\ (col[c] = "-"
            \/ (col[c] = "W" /\ c \notin popped
                /\ (c \notin wants \/ Bug = "flipwant")))
     THEN "H" ELSE col[c]]
MarkHeldBelow(hs) ==
  [c \in Blocks |->
     heldBelow[c] \/ (c \in hs /\ col[c] = "W" /\ c \notin popped
                      /\ c \in wants /\ Bug # "flipwant")]

\* Mark the heads `hs` wanted, unless already reached or served.
MarkWantedCol(hs) ==
  [c \in Blocks |->
     IF c \in hs /\ c \in PHas /\ col[c] = "-" /\ c \notin served
     THEN "W" ELSE col[c]]

\* The world picks a receiver; its first request asks for M.
Init ==
  /\ world \in {"steady", "hole"}
  /\ rhas = Start(world)
  /\ round = 1
  /\ phase = "walk"
  /\ wants = {Root}
  /\ col = StartCol({Root})
  /\ heldBelow = StartHeldBelow({Root})
  /\ popped = {}
  /\ served = {}
  /\ merged = FALSE
  /\ sentHeld = 0

\* The honest provider takes one block. A held composite marks its heads
\* held. A wanted block is served with its field block; a wanted composite
\* marks its heads wanted, or held if it is held below.
Pop ==
  \* Only while the provider is still walking.
  /\ phase = "walk"
  \* Something wanted is still waiting to be served.
  /\ Pending # {}
  \* Take one of the highest priority blocks still to take.
  /\ \E b \in NextPops :
       \* It is taken now, and never again in this walk.
       /\ popped' = popped \cup {b}
       \* A held block sends nothing; it only passes "held" on.
       /\ IF col[b] = "H"
            \* A held composite marks the composites it links held.
            THEN /\ col' = IF b \in Spine THEN MarkHeldCol(Heads(b)) ELSE col
                 \* A head it reaches that was asked for by name stays
                 \* wanted, and now knows a held block sits above it.
                 /\ heldBelow' = IF b \in Spine THEN MarkHeldBelow(Heads(b))
                                 ELSE heldBelow
                 \* Nothing is sent for a held block.
                 /\ UNCHANGED served
            \* A wanted block is sent, with its field block.
            ELSE /\ served' = served \cup {b}
                                \* The field block goes too while it is still
                                \* uncoloured ("-"). One asked for by name is
                                \* coloured wanted, and is sent when the walk
                                \* takes it itself.
                                \cup {a \in Att(b) \cap PHas : col[a] = "-"}
                 \* A field block passes nothing on; a composite passes
                 \* "held" on when a held block sits above it, since the
                 \* receiver then has everything below, and "wanted" otherwise.
                 /\ col' = IF b \notin Spine THEN col
                           ELSE IF heldBelow[b] THEN MarkHeldCol(Heads(b))
                           ELSE MarkWantedCol(Heads(b))
                 \* Passing "held" on also passes the held-below mark on to
                 \* heads that were asked for by name.
                 /\ heldBelow' = IF b \in Spine /\ heldBelow[b]
                                 THEN MarkHeldBelow(Heads(b)) ELSE heldBelow
  \* The receiver's side and the round do not move while the provider walks.
  /\ UNCHANGED <<rhas, round, phase, wants, merged, sentHeld, world>>

\* Nothing wanted is left: the response goes out.
EndWalk ==
  /\ phase = "walk"
  /\ Pending = {}
  /\ phase' = "deliver"
  /\ UNCHANGED <<rhas, round, wants, col, heldBelow, popped, served, merged,
                 sentHeld, world>>

\* The blocks still missing that the receiver can name: M while it lacks M,
\* otherwise the missing blocks a held block of M's DAG links.
Frontier(r) ==
  IF Root \notin r THEN {Root}
  ELSE {b \in Closure({Root}) \ r : \E a \in Closure({Root}) \cap r : b \in Links(a)}

\* The receiver takes what arrived. It merges once M's DAG is held, or on the
\* provider's word with "trustprovider"; it gives up after MaxRounds;
\* otherwise it asks again from the missing frontier.
Settle(got, done) ==
  LET r == rhas \cup got
  IN /\ rhas' = r
     /\ IF Closure({Root}) \subseteq r \/ (Bug = "trustprovider" /\ done)
          THEN /\ merged' = TRUE
               /\ phase' = "done"
               /\ UNCHANGED <<round, wants, col, heldBelow, popped, served>>
          ELSE IF round >= MaxRounds
          THEN /\ phase' = "done"
               /\ UNCHANGED <<merged, round, wants, col, heldBelow, popped, served>>
          ELSE /\ round' = round + 1
               /\ wants' = Frontier(r)
               /\ col' = StartCol(Frontier(r))
               /\ heldBelow' = StartHeldBelow(Frontier(r))
               /\ popped' = {}
               /\ served' = {}
               /\ phase' = "walk"
               /\ UNCHANGED merged

\* An honest provider's response arrives: exactly what the walk served.
\* The receiver drops what it holds and counts it.
DeliverHonest ==
  /\ phase = "deliver"
  /\ ~Liar
  /\ sentHeld' = sentHeld + Cardinality(served \cap rhas)
  /\ Settle(served, FALSE)
  /\ UNCHANGED world

\* What a lying provider picks its responses from: blocks of each branch, a
\* shared block the receiver may have lost, and the forgery. A small pool
\* keeps the check fast; every lie it allows is one a larger pool allows.
LiarPool == {"M", "A1", "fB", "C2", "X"}

\* A lying provider's response: any of those blocks, and any claim about
\* being done. The CID check drops the forgery ("skipcid" stores it).
DeliverLiar ==
  /\ phase = "deliver"
  /\ Liar
  /\ \E sent \in SUBSET LiarPool, done \in BOOLEAN :
       Settle(IF Bug = "skipcid" THEN sent ELSE sent \ {"X"}, done)
  /\ UNCHANGED <<sentHeld, world>>

\* The fetch is over; it stays over.
Done ==
  /\ phase = "done"
  /\ UNCHANGED vars

\* One step of the fetch.
Next == Pop \/ EndWalk \/ DeliverHonest \/ DeliverLiar \/ Done

\* The fetch, from any start.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  /\ rhas \subseteq Blocks
  /\ round \in 1..MaxRounds
  /\ phase \in {"walk", "deliver", "done"}
  /\ col \in [Blocks -> {"H", "W", "-"}]
  /\ served \subseteq PHas

\* A merged head has every block it reaches held.
INV_MergeComplete == merged => Closure({Root}) \subseteq rhas

\* A forged block is never stored.
INV_NoForged == "X" \notin rhas

\* An honest provider never sends a block the receiver already holds.
INV_HeldNotResent == ~Liar => sentHeld = 0

\* An honest fetch takes one request, or two when a block was lost.
INV_RoundsBounded ==
  \* Only an honest provider promises a bound.
  ~Liar =>
    \* A receiver that lost nothing is served in one request.
    /\ (world = "steady" => round <= 1)
    \* A receiver with a hole asks once more, by name.
    /\ round <= 2

\* An honest fetch that is over has merged.
INV_Converges == (~Liar /\ phase = "done") => merged
=============================================================================
