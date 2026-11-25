---------------------------- MODULE RootNamedFetch ----------------------------
\* The story. A document's history is a DAG of six updates. Its spine is
\* composite blocks C1..C6, each linking the one before it. Every update sets
\* the field f, so each composite carries a block of f, and each f block
\* links the f block before it. Every second update also sets the field g, so
\* C2, C4 and C6 carry a block of g, and each g block links the g block two
\* updates back. A provider holds the whole DAG; gossip has already handed
\* the receiver the new head C6.
\*
\* The receiver asks the provider for what it misses, over a rooted
\* transport (pinc). The provider answers one of two ways:
\*   - when the receiver names merged composites it holds (its haves) and
\*     every block asked for is a composite, it walks the spine and sends
\*     exactly the blocks below the asked ones outside the haves' closures;
\*   - otherwise it sends every block within a depth cut below the asked
\*     ones, because it cannot see what the receiver holds.
\*
\* The fix: a document's fetch names the root and asks with no cut, once,
\* when the provider can leave out all the receiver holds: the receiver names
\* a have, or holds nothing below the root. A receiver holding blocks below
\* the root with no have, and a branchable collection's blocks (which name no
\* haves), keep asking for the missing frontier with a cut that doubles.
\*
\* Four receivers:
\*   "steady": holds the history C1..C5 and names C5 as its have;
\*   "empty":  holds only C6, so names no have;
\*   "partial": was pushed C6's update and C5 but merged none of it, so
\*             holds blocks below C6 and names no have;
\*   "branch": the same DAG standing for a branchable collection's blocks:
\*             holds C1..C5, names no have.
\*
\* What must never happen:
\*   - a document's receiver is sent a block it already holds, other than
\*     the root it asked for by name;
\*   - a fetch that may name its root needs more than one request;
\*   - a branchable fetch is sent more of its held history than one cut;
\*   - an honest fetch stops before the whole DAG is held.
\*
\* Rust: crates/database/p2p/src/sync/coordinator/rooted_request.rs rooted_request
\* (a document whose root is a composite, or not yet held, names the root
\* with ClosureDepth::whole; an empty doc_id, a branchable collection, keeps
\* the missing CIDs and the current cut); crates/database/p2p/src/sync/coordinator/
\* dag_fetcher.rs poll_fetch_blocks and poll_fetch_rooted_provider (each
\* request a rooted transport sends asks for what rooted_request names);
\* crates/database/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::whole,
\* initial and widen; crates/database/p2p/src/sync/car/region.rs collect_missing_region
\* (the spine walk, and its depth-cut fallback for a want that is not a
\* composite or with no have held); crates/database/p2p/src/sync/coordinator/
\* event_handler/car.rs drop_held_blocks (counts car_received_already_held
\* without the fetch's root).
EXTENDS Naturals, FiniteSets

\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "frontier" a document asks for its missing frontier at a depth cut, as
\*              before the fix                        -> INV_HeldNotResent
\*   "window"   a document with no haves asks with a doubling cut instead of
\*              once with none                        -> INV_HeldNotResent
\*   "whole"    a branchable fetch asks with no cut too -> INV_BranchCut
\*   "rootany"  a document names its root even when it holds blocks below it
\*              and names no have                     -> INV_HeldNotResent
CONSTANTS Bug

\* The composites, the spine.
Spine == {"C1", "C2", "C3", "C4", "C5", "C6"}

\* The blocks of field f, one per update.
F == {"f1", "f2", "f3", "f4", "f5", "f6"}

\* The blocks of field g, one every second update.
G == {"g2", "g4", "g6"}

\* Every block of the DAG.
Blocks == Spine \cup F \cup G

\* The new head the receiver was told of.
Root == "C6"

\* Each block's links: a composite links the composite before it and carries
\* its update's field blocks; a field block links that field's block before it.
Links(b) ==
  CASE b = "C2" -> {"C1", "f2", "g2"}
    [] b = "C3" -> {"C2", "f3"}
    [] b = "C4" -> {"C3", "f4", "g4"}
    [] b = "C5" -> {"C4", "f5"}
    [] b = "C6" -> {"C5", "f6", "g6"}
    [] b = "C1" -> {"f1"}
    [] b = "f2" -> {"f1"}
    [] b = "f3" -> {"f2"}
    [] b = "f4" -> {"f3"}
    [] b = "f5" -> {"f4"}
    [] b = "f6" -> {"f5"}
    [] b = "g4" -> {"g2"}
    [] b = "g6" -> {"g4"}
    [] OTHER    -> {}

\* The blocks within `n` links below `S`, `S` included.
RECURSIVE Reach(_, _)
Reach(S, n) == IF n = 0 THEN S ELSE Reach(S \cup UNION {Links(b) : b \in S}, n - 1)

\* Everything below `S`: the DAG is eleven links deep at most.
Closure(S) == Reach(S, 12)

\* "No cut": deeper than the DAG, like ClosureDepth::whole.
Whole == 12

\* What each receiver holds at the start.
Start(w) ==
  CASE w = "steady" -> {"C1", "C2", "C3", "C4", "C5", "f1", "f2", "f3", "f4",
                        "f5", "g2", "g4", Root}
    [] w = "empty"  -> {Root}
    [] w = "partial" -> {Root, "C5", "f6", "g6", "f5"}
    [] OTHER        -> {"C1", "C2", "C3", "C4", "C5", "f1", "f2", "f3", "f4",
                        "f5", "g2", "g4"}

\* The haves each receiver names.
Haves(w) == IF w = "steady" THEN {"C5"} ELSE {}

\* Whether the receiver is fetching a document (not a branchable collection).
IsDoc(w) == w # "branch"

\* Whether a document's fetch may name its root: it names a have, or holds
\* nothing below the root ("rootany" names it regardless).
NamesRoot(w) ==
  IsDoc(w) /\ (Haves(w) # {} \/ Links(Root) \cap Start(w) = {} \/ Bug = "rootany")

\* The missing blocks a held block links: what a frontier request names.
Frontier(r) ==
  IF Root \notin r THEN {Root}
  ELSE {b \in Closure({Root}) \ r : \E a \in Closure({Root}) \cap r : b \in Links(a)}

\* What the provider sends for `wants` at cut `cut`, given the haves. With a
\* have and only composites asked, the spine walk: what is asked, and every
\* block below it outside the haves' closures. Otherwise the depth cut.
Serve(wants, cut, haves) ==
  IF haves # {} /\ wants \subseteq Spine
  THEN wants \cup (Closure(wants) \ Closure(haves))
  ELSE Reach(wants, cut)

\* rhas: what the receiver holds. round: the request it is on. cut: the cut
\* of the next frontier request. sentHeld: held blocks sent back, the root
\* asked for by name left out. phase: "ask" or "done". world: the receiver.
VARIABLES rhas, round, cut, sentHeld, phase, world

\* Every variable, for UNCHANGED.
vars == <<rhas, round, cut, sentHeld, phase, world>>

\* The first request: a document names the root with no cut ("frontier" asks
\* for the frontier at the first cut, "window" does so without haves); a
\* branchable fetch asks for its frontier at the first cut ("whole" with none).
FirstWants(w) ==
  IF NamesRoot(w) /\ ~(Bug = "frontier" \/ (Bug = "window" /\ Haves(w) = {}))
  THEN {Root}
  ELSE Frontier(Start(w))
FirstCut(w) ==
  IF (NamesRoot(w) /\ FirstWants(w) = {Root}) \/ (~IsDoc(w) /\ Bug = "whole")
  THEN Whole
  ELSE 2

\* Each receiver starts on its first request.
Init ==
  /\ world \in {"steady", "empty", "partial", "branch"}
  /\ rhas = Start(world)
  /\ round = 0
  /\ cut = 2
  /\ sentHeld = 0
  /\ phase = "ask"

\* One request and its response: the first as above, each later one names
\* the missing frontier at the current cut, which then doubles.
Ask ==
  \* Only while the fetch still asks.
  /\ phase = "ask"
  \* What this request names: the first request's choice, later the frontier.
  /\ LET wants == IF round = 0 THEN FirstWants(world) ELSE Frontier(rhas)
         \* The cut it carries: the first request's, later the doubling one.
         c     == IF round = 0 THEN FirstCut(world) ELSE cut
         \* What the provider sends back.
         got   == Serve(wants, c, Haves(world))
         \* What the receiver holds once the response is stored.
         r     == rhas \cup got
         \* Held blocks sent back are counted, the root named by name left out.
     IN /\ sentHeld' = sentHeld + Cardinality((got \cap rhas) \ {Root})
        \* The receiver stores what arrived.
        /\ rhas' = r
        \* One more request is spent.
        /\ round' = round + 1
        \* A later request doubles the cut for the next one.
        /\ cut' = IF round = 0 THEN cut ELSE cut * 2
        \* The fetch ends once the DAG is held, or gives up after five rounds.
        /\ phase' = IF Closure({Root}) \subseteq r \/ round >= 5 THEN "done" ELSE "ask"
  \* The receiver stays the same one.
  /\ UNCHANGED world

\* The fetch is over; it stays over.
Done ==
  /\ phase = "done"
  /\ UNCHANGED vars

\* One step of the fetch.
Next == Ask \/ Done

\* The fetch, from any receiver.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  /\ rhas \subseteq Blocks
  /\ round \in 0..6
  /\ phase \in {"ask", "done"}

\* A document's receiver is never sent a block it holds, but the named root.
INV_HeldNotResent == IsDoc(world) => sentHeld = 0

\* A fetch that may name its root is one request.
INV_OneRequest == NamesRoot(world) => round <= 1

\* A branchable fetch gets back at most what its first cut reaches of its
\* held history: the four held blocks within two links of the new head.
INV_BranchCut == ~IsDoc(world) => sentHeld <= 4

\* A finished fetch holds the whole DAG.
INV_Converges == phase = "done" => Closure({Root}) \subseteq rhas
=============================================================================
