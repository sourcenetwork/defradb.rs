---- MODULE RootSingleFlight ----
\* The story: our node asks every connected peer for a document's heads. Each
\* peer that holds the document replies with the same root, and each reply
\* used to start its own DAG fetch, so with four peers the history crossed the
\* network four times. Now a reply claims the root before it fetches. While
\* the claim is held, a later reply starts nothing; when the fetch ends, the
\* claim is dropped whether it succeeded or failed.
\*
\* A fetch merges with the trust of the reply that started it. A reply from
\* an explicit replicator ("Trusted") lets the merge replay blocks a plain
\* peer's reply cannot, so a trusted merge covers a plain reply but not the
\* reverse. The claim is keyed by the root and that trust: a reply waits only
\* behind a fetch whose merge covers it, and a reply that starts nothing then
\* loses nothing, because the running fetch already tries every connected
\* peer, the replier included.
\*
\* Who acts: the peers (replying, R of them, each plain or trusted), the
\* fetch (succeeding or failing, for instance on a timeout).
\*
\* What must never happen: two fetches of one claim key at once; a reply for
\* a root not yet merged under its trust that starts nothing while no fetch
\* covering its trust runs.
\*
\* Code it models (crates/p2p/src/sync/coordinator):
\*   mod.rs                          DagFetchLimiter::claim_root, RootKey, RootClaim (dropped when the fetch ends)
\*   event_handler/doc_sync.rs       handle_doc_sync_reply (claim with the replier's collections, then fetch)
\*   event_handler/branchable_sync.rs handle_branchable_sync_reply (claim with the replier's trust, then fetch)
EXTENDS Naturals

CONSTANTS
  R,           \* how many replies arrive for the root
  GuardMode,   \* "Claim" (a reply fetches only with the claim) | "None"
  ReleaseMode, \* "Drop" (the claim goes when the fetch ends) | "Keep" (only on success)
  KeyMode      \* "RootTrust" (claim keyed by root and trust) | "Root" (by root alone)

\* At least one reply arrives.
ASSUME R \in Nat \ {0}
ASSUME GuardMode \in {"Claim", "None"}
ASSUME ReleaseMode \in {"Drop", "Keep"}
ASSUME KeyMode \in {"RootTrust", "Root"}

Trust == {"Plain", "Trusted"}

\* The claim a reply of trust t takes.
KeyOf(t) == IF KeyMode = "RootTrust" THEN t ELSE "Plain"

\* A merge under trust m accepts what a reply of trust t offers.
Covers(m, t) == m = "Trusted" \/ t = "Plain"

VARIABLES
  arrived,   \* replies handled so far
  fetching,  \* fetching[t]: fetches of the root running under trust t
  claimed,   \* claimed[k]: the claim keyed k is held
  merged,    \* merged[t]: the root's DAG has been merged under trust t
  over,      \* ghost: two fetches of one claim key ran at once
  lost       \* ghost: a reply needing a fetch started nothing while none covered it

vars == <<arrived, fetching, claimed, merged, over, lost>>

TypeOK ==
  /\ arrived \in 0..R
  /\ fetching \in [Trust -> 0..R]
  /\ claimed \in [Trust -> BOOLEAN]
  /\ merged \in [Trust -> BOOLEAN]
  /\ over \in BOOLEAN
  /\ lost \in BOOLEAN

\* A reply of trust t needs nothing once a merge covering it is done.
Done(t) == \E m \in Trust : merged[m] /\ Covers(m, t)

\* A fetch running now will merge what a reply of trust t offers.
Running(t) == \E m \in Trust : fetching[m] > 0 /\ Covers(m, t)

\* Fetches running under the claim keyed k.
KeyFetches(k) ==
  (IF KeyOf("Plain") = k THEN fetching["Plain"] ELSE 0)
  + (IF KeyOf("Trusted") = k THEN fetching["Trusted"] ELSE 0)

\* No reply yet, nothing fetching, the DAG missing.
Init ==
  /\ arrived = 0
  /\ fetching = [t \in Trust |-> 0]
  /\ claimed = [k \in Trust |-> FALSE]
  /\ merged = [t \in Trust |-> FALSE]
  /\ over = FALSE
  /\ lost = FALSE

\* A reply of trust t arrives. A root merged under covering trust needs
\* nothing. Otherwise the reply fetches if it takes its claim (or, without
\* the guard, always); a reply that cannot take the claim starts nothing,
\* which loses it if no running fetch covers its trust.
Reply(t) ==
  /\ arrived < R
  /\ arrived' = arrived + 1
  /\ IF Done(t)
     THEN UNCHANGED <<fetching, claimed, over, lost>>
     ELSE IF GuardMode = "None" \/ ~claimed[KeyOf(t)]
          THEN /\ fetching' = [fetching EXCEPT ![t] = @ + 1]
               /\ claimed' = [claimed EXCEPT ![KeyOf(t)] = TRUE]
               /\ over' = (over \/ KeyFetches(KeyOf(t)) + 1 > 1)
               /\ UNCHANGED lost
          ELSE /\ lost' = (lost \/ ~Running(t))
               /\ UNCHANGED <<fetching, claimed, over>>
  /\ UNCHANGED merged

\* A fetch under trust t merges the DAG under t and drops its claim.
Succeed(t) ==
  /\ fetching[t] > 0
  /\ fetching' = [fetching EXCEPT ![t] = @ - 1]
  /\ merged' = [merged EXCEPT ![t] = TRUE]
  /\ claimed' = [claimed EXCEPT ![KeyOf(t)] = FALSE]
  /\ UNCHANGED <<arrived, over, lost>>

\* A fetch under trust t fails. Under "Keep" its claim stays behind.
Fail(t) ==
  /\ fetching[t] > 0
  /\ fetching' = [fetching EXCEPT ![t] = @ - 1]
  /\ claimed' = IF ReleaseMode = "Drop"
                THEN [claimed EXCEPT ![KeyOf(t)] = FALSE]
                ELSE claimed
  /\ UNCHANGED <<arrived, merged, over, lost>>

Next == \E t \in Trust : Reply(t) \/ Succeed(t) \/ Fail(t)

Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Never two fetches of one claim key at once.
INV_OneFetchPerRoot == ~over

\* A reply that still needs a fetch always has one running that covers it.
INV_NoReplyLost == ~lost
====
