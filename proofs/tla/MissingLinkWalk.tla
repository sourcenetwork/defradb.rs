---- MODULE MissingLinkWalk ----
\* The story. A node is missing part of one document's DAG and fetches it in
\* dispatches. A dispatch walks the blocks it already holds to find what is
\* missing, fetches some of it, and walks again. The walk is local work that
\* grows with the DAG, so it goes page by page: after every page it gives its
\* core to other tasks, and once the dispatch's time slice (its quantum) is
\* spent it may stop at a page boundary and hand the slot back. A stopped walk
\* keeps nothing; the next dispatch walks again from the root.
\*
\* Who acts: the retry clock (dispatches the root), the walker (one page at a
\* time), the fetcher (one productive window per step), and the clock (ends
\* the quantum at any moment it likes, as early as it likes).
\*
\* What must never happen: a stopped walk is taken as a complete DAG; a walk
\* runs more than a page without giving its core away; and, over time, a DAG
\* whose walk outlasts the quantum never converges because every dispatch is
\* stopped before it fetches anything.
\*
\* Code it models (crates/database/p2p/src/sync):
\*   manager/links.rs  walk_missing_links / walk_pages (a yield after every
\*     MISSING_LINK_WALK_PAGE children; a deadline checked only there; a stop
\*     is WalkEnd::Preempted, never a frontier)
\*   coordinator/dag_fetcher.rs  fetch_dag_attempt (a WalkEnd::Preempted
\*     returns FetchAttemptOutcome::Preempted) and ProviderFetchState::walk_deadline
\*     (the quantum applies to a walk only once the attempt fetched a block)
EXTENDS Naturals

\* How many blocks the node is missing when the story starts.
CONSTANT N
\* RED switch: a walk may be stopped before its attempt fetched anything.
CONSTANT StopFirstWalk
\* RED switch: a stopped walk is read as "nothing missing".
CONSTANT StoppedAsEmpty
\* RED switch: the walker never gives its core away between pages.
CONSTANT NoYield

\* fetched: blocks fetched so far. phase: where the dispatch is. pos: pages
\* the current walk has read. progressed: this attempt fetched a block. spent:
\* the quantum is over. unyielded: pages walked since the core was last given
\* away. ready: the node declared the DAG complete.
VARIABLES fetched, phase, pos, progressed, spent, unyielded, ready
vars == <<fetched, phase, pos, progressed, spent, unyielded, ready>>

\* A walk reads one page per held block plus two pages of history the node
\* already had, so it grows as the DAG is fetched (a whole walk, not just the
\* missing part) and always has a page boundary inside it.
Len == 2 + fetched

\* The walk may stop at this page boundary: the quantum is over, and (unless
\* the RED switch plants the bug) the attempt already fetched a block.
MayStop == spent /\ (progressed \/ StopFirstWalk)

\* Nothing fetched yet, nothing running, nothing declared.
Init == /\ fetched = 0
        /\ phase = "Idle"
        /\ pos = 0
        /\ progressed = FALSE
        /\ spent = FALSE
        /\ unyielded = 0
        /\ ready = FALSE

\* The retry clock dispatches the root: a fresh attempt with a fresh quantum.
Dispatch == /\ phase = "Idle"
            /\ ~ready
            /\ phase' = "Walk"
            /\ pos' = 0
            /\ progressed' = FALSE
            /\ spent' = FALSE
            /\ unyielded' = 0
            /\ UNCHANGED <<fetched, ready>>

\* The clock ends the quantum, whenever it likes, while a dispatch runs.
Tick == /\ phase \in {"Walk", "Fetch"}
        /\ ~spent
        /\ spent' = TRUE
        /\ UNCHANGED <<fetched, phase, pos, progressed, unyielded, ready>>

\* The walker reads one more page and reaches a page boundary, where it gives
\* its core away (unless NoYield plants the bug).
Page == /\ phase = "Walk"
        /\ pos < Len
        /\ ~(pos > 0 /\ MayStop)
        /\ pos' = pos + 1
        /\ unyielded' = IF NoYield THEN unyielded + 1 ELSE 0
        /\ UNCHANGED <<fetched, phase, progressed, spent, ready>>

\* At a page boundary past the quantum the walk stops and the slot goes back.
\* The correct code reports "preempted"; StoppedAsEmpty reads it as complete.
Stop == /\ phase = "Walk"
        /\ pos > 0
        /\ pos < Len
        /\ MayStop
        /\ phase' = "Idle"
        /\ ready' = StoppedAsEmpty
        /\ UNCHANGED <<fetched, pos, progressed, spent, unyielded>>

\* The walk read every page. Its frontier is what is still missing: empty
\* means the DAG is complete, otherwise the fetcher goes on.
WalkDone == /\ phase = "Walk"
            /\ pos = Len
            /\ IF fetched = N
                 THEN /\ ready' = TRUE
                      /\ phase' = "Idle"
                 ELSE /\ phase' = "Fetch"
                      /\ UNCHANGED ready
            /\ UNCHANGED <<fetched, pos, progressed, spent, unyielded>>

\* The fetcher brings in at least one missing block, then walks again; past
\* the quantum with progress made, it hands the slot back instead (the
\* existing per-window check in fetch_dag_attempt).
Fetch == /\ phase = "Fetch"
         /\ fetched' = fetched + 1
         /\ progressed' = TRUE
         /\ pos' = 0
         /\ phase' = IF spent THEN "Idle" ELSE "Walk"
         /\ UNCHANGED <<spent, unyielded, ready>>

\* Any of the steps above.
Next == Dispatch \/ Tick \/ Page \/ Stop \/ WalkDone \/ Fetch

\* The clock may fire at any time but need not; every other actor keeps going
\* while it can.
Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(Dispatch) /\ WF_vars(Page) /\ WF_vars(Stop)
        /\ WF_vars(WalkDone) /\ WF_vars(Fetch)

\* Every variable stays in its range.
TypeOK == /\ fetched \in 0..N
          /\ phase \in {"Idle", "Walk", "Fetch"}
          /\ pos \in 0..(N + 2)
          /\ progressed \in BOOLEAN
          /\ spent \in BOOLEAN
          /\ unyielded \in Nat
          /\ ready \in BOOLEAN

\* The DAG is declared complete only when every missing block was fetched:
\* a stopped walk never reads as complete.
INV_StoppedNeverReady == ready => fetched = N

\* The walker never reads two pages without giving its core away.
INV_YieldsEachPage == unyielded <= 1

\* However early the clock fires in every dispatch, the DAG converges.
LIVE_Converges == <>ready
====
