---- MODULE GovernanceHeads ----
\* The governed blocks a validator never sees, and what the host does with
\* them. A collection block installs a head over the composites it links; a
\* definition stores a version over the one it patches. Neither is judged by
\* the plugin, so what they may do rests on the verdicts already taken on the
\* composites and versions they name.
\*
\*   GovernanceMerge.tla   composites: verdict, defer, re-drive, sweep
\*   GovernanceHeads.tla   collection blocks and definitions over them (this)
\*
\* Anchors (crates/db/src/merge/ unless said otherwise; line numbers in
\* GovernanceHeads_DESIGN.md):
\*   governance/collection_block.rs   a block's verdict derived from its links:
\*                                    install when every link and parent has
\*                                    merged, reject when one is known rejected,
\*                                    else defer on what is missing
\*   merge_handler/composite.rs       the downsample-source skip and where it
\*                                    sits relative to judge_governed
\*   crates/p2p/src/sync/replication/handlers.rs   a terminal skip is marked merged
\*   merge_handler/definition.rs      a nameless patch whose base is not held
\*   governance/sweep.rs              the definition arm of the sweep
\*
\* What this module asks:
\*  1. Is a head ever installed over a composite no verdict accepted? The
\*     link check reads the merged mark, so anything that sets that mark
\*     without a verdict is a hole. SkipBeforeJudge is the downsample-source
\*     skip as it was coded: terminal, and before the verdict.
\*  2. Is a patch that arrives before the version it patches ever lost?
\*     OrphanTerminal is the terminal skip that dropped it.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Writes,           \* governed composites
  Verdict,          \* [Writes -> {"Accept", "Reject"}]  the validator, once inputs are held
  Blocks,           \* collection blocks
  Links,            \* [Blocks -> SUBSET Writes]
  Defs,             \* definitions
  Prev,             \* [Defs -> Defs \cup {"none"}]  the version a patch supersedes
  Downsampled,      \* SUBSET Writes  replicated writes into a local-only downsample source
  SkipBeforeJudge,  \* the skip is terminal and runs before the verdict (as coded)
  OrphanTerminal    \* a patch whose base is not held is dropped as terminal (as coded before)

ASSUME Verdict \in [Writes -> {"Accept", "Reject"}]
ASSUME Links \in [Blocks -> SUBSET Writes]
ASSUME Prev \in [Defs -> Defs \cup {"none"}]
ASSUME Downsampled \subseteq Writes
ASSUME SkipBeforeJudge \in BOOLEAN
ASSUME OrphanTerminal \in BOOLEAN

VARIABLES
  pushed,       \* SUBSET Writes  composites received
  merged,       \* SUBSET Writes  the durable merged mark, what a link check reads
  accepted,     \* SUBSET Writes  ghost: the validator said Accept
  quarantined,  \* SUBSET Writes
  heads,        \* SUBSET Blocks  installed
  waitingBlocks,\* SUBSET Blocks  deferred on a missing link
  rejectedBlocks, \* SUBSET Blocks
  stored,       \* SUBSET Defs
  waitingDefs,  \* SUBSET Defs    deferred on a base not held
  droppedDefs   \* SUBSET Defs    skipped as terminal: never looked at again

vars == << pushed, merged, accepted, quarantined, heads, waitingBlocks, rejectedBlocks, stored, waitingDefs, droppedDefs >>

\* A replicated composite arrives. The downsample-source skip, as coded,
\* answers before the verdict is taken and its answer is a terminal skip,
\* which the replication handler marks merged. Fixed, the verdict comes
\* first: a reject quarantines, an accept is then skipped or merged, and
\* either way the mark means what the link check takes it to mean.
PushWrite(w) ==
  /\ w \notin pushed
  /\ pushed' = pushed \cup {w}
  /\ IF w \in Downsampled /\ SkipBeforeJudge
       THEN /\ merged' = merged \cup {w}
            /\ UNCHANGED << accepted, quarantined >>
       ELSE IF Verdict[w] = "Accept"
       THEN /\ merged' = merged \cup {w}
            /\ accepted' = accepted \cup {w}
            /\ UNCHANGED quarantined
       ELSE /\ quarantined' = quarantined \cup {w}
            /\ UNCHANGED << merged, accepted >>
  /\ UNCHANGED << heads, waitingBlocks, rejectedBlocks, stored, waitingDefs, droppedDefs >>

\* A collection block's verdict is derived from its links (collection_block.rs).
BlockVerdict(b) ==
  IF Links[b] \subseteq merged THEN "Install"
  ELSE IF Links[b] \cap quarantined # {} THEN "Reject"
  ELSE "Defer"

ApplyBlock(b) ==
  LET v == BlockVerdict(b) IN
  /\ heads' = IF v = "Install" THEN heads \cup {b} ELSE heads
  /\ rejectedBlocks' = IF v = "Reject" THEN rejectedBlocks \cup {b} ELSE rejectedBlocks
  /\ waitingBlocks' = IF v = "Defer" THEN waitingBlocks \cup {b} ELSE waitingBlocks \ {b}

PushBlock(b) ==
  /\ b \notin heads \cup waitingBlocks \cup rejectedBlocks
  /\ ApplyBlock(b)
  /\ UNCHANGED << pushed, merged, accepted, quarantined, stored, waitingDefs, droppedDefs >>

\* A deferred block is re-driven when a link merges (the release), or by the sweep.
RedriveBlock(b) ==
  /\ b \in waitingBlocks
  /\ ApplyBlock(b)
  /\ UNCHANGED << pushed, merged, accepted, quarantined, stored, waitingDefs, droppedDefs >>

\* A definition arrives. An initial one, or a patch whose base is held, is
\* stored. A patch whose base is not held either waits on it (fixed) or is
\* dropped as terminal (as coded before), after which nothing re-drives it.
BaseHeld(d) == Prev[d] = "none" \/ Prev[d] \in stored

PushDef(d) ==
  /\ d \notin stored \cup waitingDefs \cup droppedDefs
  /\ IF BaseHeld(d)
       THEN /\ stored' = stored \cup {d}
            /\ UNCHANGED << waitingDefs, droppedDefs >>
       ELSE IF OrphanTerminal
       THEN /\ droppedDefs' = droppedDefs \cup {d}
            /\ UNCHANGED << stored, waitingDefs >>
       ELSE /\ waitingDefs' = waitingDefs \cup {d}
            /\ UNCHANGED << stored, droppedDefs >>
  /\ UNCHANGED << pushed, merged, accepted, quarantined, heads, waitingBlocks, rejectedBlocks >>

\* A stored version releases the patches waiting on it (definition.rs), and
\* the sweep re-drives a waiting patch after a restart.
RedriveDef(d) ==
  /\ d \in waitingDefs
  /\ BaseHeld(d)
  /\ stored' = stored \cup {d}
  /\ waitingDefs' = waitingDefs \ {d}
  /\ UNCHANGED << pushed, merged, accepted, quarantined, heads, waitingBlocks, rejectedBlocks, droppedDefs >>

Next ==
  \/ \E w \in Writes : PushWrite(w)
  \/ \E b \in Blocks : PushBlock(b) \/ RedriveBlock(b)
  \/ \E d \in Defs   : PushDef(d) \/ RedriveDef(d)

Init ==
  /\ pushed = {} /\ merged = {} /\ accepted = {} /\ quarantined = {}
  /\ heads = {} /\ waitingBlocks = {} /\ rejectedBlocks = {}
  /\ stored = {} /\ waitingDefs = {} /\ droppedDefs = {}

Fairness ==
  /\ \A w \in Writes : WF_vars(PushWrite(w))
  /\ \A b \in Blocks : WF_vars(PushBlock(b)) /\ WF_vars(RedriveBlock(b))
  /\ \A d \in Defs   : WF_vars(PushDef(d)) /\ WF_vars(RedriveDef(d))

Spec == Init /\ [][Next]_vars /\ Fairness

\* ---- What the host owes ----

\* The merged mark of a governed composite is a verdict's: nothing sets it
\* without the validator having accepted.
INV_MergedIsJudged == merged \subseteq accepted

\* A head is installed only over accepted composites (D-48's open item).
INV_HeadOverAccepted == \A b \in heads : Links[b] \subseteq accepted

\* A block whose links all accept is eventually installed; one with a
\* rejected link never is.
Installable(b) == \A w \in Links[b] : Verdict[w] = "Accept"
L_InstallableInstalls == <>[](\A b \in Blocks : Installable(b) => b \in heads)
INV_RejectedLinkNoHead == \A b \in heads : Links[b] \cap quarantined = {}

\* A definition whose chain of bases is all deliverable is eventually stored.
RECURSIVE Chain(_)
Chain(d) == IF Prev[d] = "none" THEN {d} ELSE {d} \cup Chain(Prev[d])
L_DefsStored == <>[](\A d \in Defs : d \in stored)

====
