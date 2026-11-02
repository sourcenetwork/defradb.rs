---- MODULE DagFetchBudget ----
\* The story: a node fetches the missing blocks of one DAG root in batches.
\* For each batch it asks its providers one at a time (a rotation), and each
\* provider gets one fetch window. In the case modeled here no provider ever
\* delivers a block: each window either stalls (nothing comes back until the
\* window ends), fails slowly (the transport gives up only at its own response
\* or connect timeout), or is refused fast (the provider says no at once,
\* because it is saturated or does not hold the blocks yet).
\*
\* Who acts: the fetcher (one attempt over every batch) and the providers
\* (which pick how each window ends).
\*
\* What must never happen: the time one attempt spends waiting on windows that
\* bring nothing grows with the number of batches. One attempt may wait at
\* most one full rotation of stalls (Providers windows) plus a fixed allowance
\* of fast refusals (MaxFree, each costing at most Fast), however many batches
\* the missing frontier spans.
\*
\* Code it models (crates/p2p/src/sync/coordinator):
\*   dag_fetcher.rs  fetch_dag_attempt              (stall_budget = providers, free_refusals = MAX_FREE_REFUSALS)
\*   dag_fetcher.rs  poll_fetch_blocks_rotating     (Ask: budget check, then spend by outcome)
\*   dag_fetcher.rs  window_outcome                 (Classify: Refused only within FAST_REFUSAL)
\*   dag_retry.rs    FAST_REFUSAL, MAX_FREE_REFUSALS
\* Rust tests: tests/unit/provider_ranking.rs
\*   a_failure_at_the_response_timeout_spends_the_stall_budget,
\*   free_refusals_are_capped_per_attempt, a_fast_refusal_does_not_spend_the_stall_budget.
EXTENDS Naturals

CONSTANTS
  Batches,     \* how many batches the missing frontier spans
  Providers,   \* how many providers the rotation holds
  Window,      \* time a stalled or slowly failing window costs
  Fast,        \* most time a fast refusal costs (FAST_REFUSAL)
  MaxFree,     \* fast refusals per attempt that cost no stall budget
  SlowMode,    \* "Stall" (a slow failure is a stall) | "Refusal" (the bug: it is free)
  CapMode,     \* "Capped" (free refusals run out) | "Uncapped" (every refusal is free)
  CheckMode    \* "Check" (an empty budget skips the batch) | "NoCheck" (ask anyway)

\* The free refusals together cost no more than one more window.
ASSUME MaxFree * Fast <= Window

VARIABLES
  batch,   \* the batch being fetched, 1..Batches; Batches + 1 once the attempt ends
  tried,   \* providers already asked for this batch
  budget,  \* stall windows the attempt may still spend
  free,    \* fast refusals the attempt may still take without spending budget
  waited   \* time the attempt has spent in windows that brought nothing

\* Everything that changes, for the frame of each action.
vars == <<batch, tried, budget, free, waited>>

\* An attempt starts at the first batch with a full budget and nothing waited.
Init == /\ batch = 1 /\ tried = 0 /\ budget = Providers
        /\ free = MaxFree /\ waited = 0

\* The attempt is still walking batches.
Running == batch <= Batches

\* Move on to the next batch with no provider asked yet.
NextBatch == /\ batch' = batch + 1 /\ tried' = 0

\* Every provider was asked for this batch and none delivered: give up on it.
Exhausted == /\ Running /\ tried = Providers
             /\ NextBatch /\ UNCHANGED <<budget, free, waited>>

\* The budget is spent, so the batch fails without asking anyone (the Rust
\* `stall_budget == 0` early return). The RED NoCheck config drops this.
Skip == /\ Running /\ tried < Providers /\ budget = 0 /\ CheckMode = "Check"
        /\ NextBatch /\ UNCHANGED <<budget, free, waited>>

\* A provider may be asked while the budget lasts (or always, without the check).
MayAsk == /\ Running /\ tried < Providers
          /\ (budget > 0 \/ CheckMode = "NoCheck")

\* Spend one stall window: the budget never goes below zero, as in Rust.
SpendStall == budget' = IF budget > 0 THEN budget - 1 ELSE 0

\* The provider's window stalls: the full window is waited and budget spent.
Stall == /\ MayAsk /\ tried' = tried + 1
         /\ waited' = waited + Window /\ SpendStall
         /\ UNCHANGED <<batch, free>>

\* The provider fails only at the transport timeout, a full window in. It is a
\* stall and spends budget; the RED Refusal config counts it as a free refusal.
SlowFail == /\ MayAsk /\ tried' = tried + 1 /\ waited' = waited + Window
            /\ IF SlowMode = "Stall"
                 THEN SpendStall /\ UNCHANGED free
                 ELSE UNCHANGED <<budget, free>>
            /\ UNCHANGED batch

\* The provider refuses within FAST_REFUSAL. It takes a free refusal while any
\* are left, and spends stall budget after that; the RED Uncapped config lets
\* every refusal be free.
Refuse == /\ MayAsk /\ tried' = tried + 1 /\ waited' = waited + Fast
          /\ IF free > 0 \/ CapMode = "Uncapped"
               THEN /\ free' = IF free > 0 THEN free - 1 ELSE 0
                    /\ UNCHANGED budget
               ELSE SpendStall /\ UNCHANGED free
          /\ UNCHANGED batch

\* The attempt has ended; nothing more happens.
Done == ~Running /\ UNCHANGED vars

\* The fetcher and the providers take turns until the attempt ends.
Next == Exhausted \/ Skip \/ Stall \/ SlowFail \/ Refuse \/ Done

\* The bound the stall budget exists for: one rotation of windows plus the free
\* refusals, with no term for the number of batches.
AttemptBound == Providers * Window + MaxFree * Fast

\* Promise: an attempt never waits longer than that bound on windows that
\* brought nothing, however many batches it walks.
INV_WaitBounded == waited <= AttemptBound

\* Promise: the budget and the free refusals are never overdrawn.
INV_TypeOK == /\ batch \in 1..(Batches + 1) /\ tried \in 0..Providers
              /\ budget \in 0..Providers /\ free \in 0..MaxFree
====
