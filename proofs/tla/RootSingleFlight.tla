---- MODULE RootSingleFlight ----
\* The story: our node asks every connected peer for a document's heads. Each
\* peer that holds the document replies with the same root, and each reply
\* used to start its own DAG fetch, so with four peers the history crossed the
\* network four times. Now a reply claims the root before it fetches. While
\* the claim is held, a later reply starts nothing; when the fetch ends, the
\* claim is dropped whether it succeeded or failed.
\*
\* A reply that starts nothing loses nothing: the running fetch already
\* tries every connected peer, the replier included.
\*
\* Who acts: the peers (replying, R of them), the fetch (succeeding or
\* failing, for instance on a timeout).
\*
\* What must never happen: two fetches of one root at once; a reply for a
\* missing root that starts nothing while nothing is fetching it.
\*
\* Code it models (crates/p2p/src/sync/coordinator):
\*   mod.rs                          DagFetchLimiter::claim_root, RootClaim (dropped when the fetch ends)
\*   event_handler/doc_sync.rs       handle_doc_sync_reply (claim, then fetch)
\*   event_handler/branchable_sync.rs handle_branchable_sync_reply (claim, then fetch)
EXTENDS Naturals

CONSTANTS
  R,           \* how many replies arrive for the root
  GuardMode,   \* "Claim" (a reply fetches only with the claim) | "None"
  ReleaseMode  \* "Drop" (the claim goes when the fetch ends) | "Keep" (only on success)

\* At least one reply arrives.
ASSUME R \in Nat \ {0}
ASSUME GuardMode \in {"Claim", "None"}
ASSUME ReleaseMode \in {"Drop", "Keep"}

VARIABLES
  arrived,   \* replies handled so far
  fetching,  \* fetches of the root running now
  claimed,   \* the root's claim is held
  local,     \* the root's DAG is stored locally
  over,      \* ghost: two fetches of the root ran at once
  lost       \* ghost: a reply for a missing root started nothing while nothing fetched it

vars == <<arrived, fetching, claimed, local, over, lost>>

TypeOK ==
  /\ arrived \in 0..R
  /\ fetching \in 0..R
  /\ claimed \in BOOLEAN
  /\ local \in BOOLEAN
  /\ over \in BOOLEAN
  /\ lost \in BOOLEAN

\* No reply yet, nothing fetching, the DAG missing.
Init ==
  /\ arrived = 0
  /\ fetching = 0
  /\ claimed = FALSE
  /\ local = FALSE
  /\ over = FALSE
  /\ lost = FALSE

\* A reply arrives. A stored root needs nothing. Otherwise the reply fetches
\* if it takes the claim (or, without the guard, always); a reply that cannot
\* take the claim starts nothing, which loses it if nothing is fetching.
Reply ==
  /\ arrived < R
  /\ arrived' = arrived + 1
  /\ IF local
     THEN UNCHANGED <<fetching, claimed, over, lost>>
     ELSE IF GuardMode = "None" \/ ~claimed
          THEN /\ fetching' = fetching + 1
               /\ claimed' = TRUE
               /\ over' = (over \/ fetching + 1 > 1)
               /\ UNCHANGED lost
          ELSE /\ lost' = (lost \/ fetching = 0)
               /\ UNCHANGED <<fetching, claimed, over>>
  /\ UNCHANGED local

\* A fetch stores the DAG and drops the claim.
Succeed ==
  /\ fetching > 0
  /\ fetching' = fetching - 1
  /\ local' = TRUE
  /\ claimed' = FALSE
  /\ UNCHANGED <<arrived, over, lost>>

\* A fetch fails. Under "Keep" its claim stays behind.
Fail ==
  /\ fetching > 0
  /\ fetching' = fetching - 1
  /\ claimed' = IF ReleaseMode = "Drop" THEN FALSE ELSE claimed
  /\ UNCHANGED <<arrived, local, over, lost>>

Next == Reply \/ Succeed \/ Fail

Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Never two fetches of one root at once.
INV_OneFetchPerRoot == ~over

\* A reply for a missing root always has a fetch running for it.
INV_NoReplyLost == ~lost
====
