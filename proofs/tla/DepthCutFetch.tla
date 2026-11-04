---------------------------- MODULE DepthCutFetch ----------------------------
\* The story. A document's history is a chain of blocks, oldest first, and a
\* provider holds all of it. A receiver holds the oldest part (its history)
\* and misses the newest part, ending in the head it was just told about.
\* The provider cannot see what the receiver holds. So the receiver asks for
\* the blocks below its missing frontier, but only down to a cut of a few
\* links; each round that leaves blocks missing, it asks again from the new
\* frontier with twice the cut, never more than the response cap.
\*
\* What must never happen:
\*   - the receiver is sent much of the history it already holds: at most the
\*     first cut, or twice the blocks it was missing;
\*   - a receiver missing many blocks needs a round per block or two: the
\*     rounds stay logarithmic in what it misses;
\*   - the fetch stops before every block is held.
\*
\* Rust: crates/p2p/src/sync/car.rs collect_dag_blocks_within (the provider
\* walks at most the requested cut below each wanted CID);
\* crates/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::initial and
\* ClosureDepth::widen (the first cut and its doubling);
\* crates/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt (one
\* round per missing frontier, widening after each round that still left one).
\* The model is one attempt: each retry attempt restarts at InitCut, so
\* INV_RoundsLogarithmic bounds rounds per attempt, not per fetch.
EXTENDS Naturals, FiniteSets

\* N: blocks in the chain; block N is the head, block 1 the oldest.
\* InitCut: the first cut, ClosureDepth::initial.
\* Cap: the widest cut, ClosureDepth's MAX_CLOSURE_DEPTH.
\* MaxRounds: the round bound the chosen N must meet with doubling.
\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "unbounded" the provider ignores the cut       -> INV_HistoryNotResent
\*   "nowiden"   the receiver never widens the cut  -> INV_RoundsLogarithmic
CONSTANTS N, InitCut, Cap, MaxRounds, Bug

\* Every block of the chain.
Blocks == 1..N

\* The larger of two numbers.
Max2(a, b) == IF a >= b THEN a ELSE b

\* The smaller of two numbers.
Min2(a, b) == IF a <= b THEN a ELSE b

\* The largest member of a non-empty set of numbers.
MaxOf(S) == CHOOSE x \in S : \A y \in S : y <= x

\* history: the oldest blocks the receiver held before the fetch began.
\* has: every block the receiver holds now.
\* cut: how many links below the frontier the next round asks for.
\* resent: how many history blocks the provider has sent back so far.
\* rounds: how many requests the receiver has made.
VARIABLES history, has, cut, resent, rounds

\* Every variable, for UNCHANGED and fairness.
vars == <<history, has, cut, resent, rounds>>

\* The blocks still missing.
Missing == Blocks \ has

\* How many blocks the receiver was missing when the fetch began.
MissingAtStart == N - Cardinality(history)

\* The world picks how much history the receiver holds: none of it, or all
\* of it but the head, or anything between. The head is always missing.
Init ==
  /\ history \in {1..h : h \in 0..(N - 1)}
  /\ has = history
  /\ cut = InitCut
  /\ resent = 0
  /\ rounds = 0

\* One round. The frontier is the newest missing block: every block above it
\* is held, so it is what the receiver walks to and asks for. The provider
\* sends the frontier and every block at most `cut` links below it (the
\* "unbounded" bug sends the frontier's whole closure). The receiver stores
\* them, counts the history blocks among them, and doubles the cut up to the
\* cap (the "nowiden" bug keeps it).
Round ==
  /\ Missing # {}
  /\ LET frontier == MaxOf(Missing)
         \* What the provider walks: the frontier and the chain below it,
         \* cut at `cut` links unless the bug ignores the cut.
         served == IF Bug = "unbounded"
                     THEN 1..frontier
                     ELSE {b \in 1..frontier : frontier - b <= cut}
     IN /\ has' = has \cup served
        /\ resent' = resent + Cardinality(served \cap history)
  /\ rounds' = rounds + 1
  /\ cut' = IF Bug = "nowiden" THEN cut ELSE Min2(2 * cut, Cap)
  /\ UNCHANGED history

\* The fetch is done once nothing is missing; it then stays done.
Done ==
  /\ Missing = {}
  /\ UNCHANGED vars

\* A round, or nothing left to do.
Next == Round \/ Done

\* The receiver keeps making rounds while something is missing.
Spec == Init /\ [][Next]_vars /\ WF_vars(Round)

\* Every variable stays in its range.
INV_TypeOK ==
  /\ history \subseteq Blocks
  /\ has \subseteq Blocks
  /\ cut \in 0..Cap
  /\ resent \in 0..N
  /\ rounds \in Nat

\* What the receiver held is never lost, and what it is sent is real blocks.
INV_HoldsHistory == history \subseteq has

\* History sent back is at most the first cut, or twice what was missing.
\* The last round overshoots by at most its own cut, and that cut is at most
\* twice the one before, which still fell short of the missing blocks.
INV_HistoryNotResent == resent <= Max2(InitCut, 2 * MissingAtStart)

\* The rounds stay within the logarithmic bound for this chain length.
INV_RoundsLogarithmic == rounds <= MaxRounds

\* The receiver eventually holds every block.
LIVE_Completes == <>(Missing = {})
=============================================================================
