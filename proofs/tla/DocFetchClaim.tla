---------------------------- MODULE DocFetchClaim ----------------------------
\* The story. A receiver holds nothing of a document and hears of three of
\* its heads at once (vinc releases a peer's held publishes together). Head
\* 3 sits on head 2, which sits on head 1, so each head's history holds the
\* ones below it. Each head starts its own fetch task.
\*
\* A fetch names haves: heads whose whole history is already held here. The
\* provider then sends only the head's history less the haves' histories.
\* A head becomes a have two ways: its fetch completed (remembered per
\* document), or its merge ran, which happens some time after the fetch.
\*
\* So fetches of one document run one at a time. A task that finds another
\* fetching the document waits on that fetch's "ended" signal. The holder,
\* when done, first records its head as completed, then frees the document,
\* then sets the signal; the waiter wakes at once and claims the document.
\*
\* What must never happen:
\*   - two fetches of the document run at once;
\*   - a block the receiver already holds is sent to it again;
\*   - a task waits on a signal that will never be set.
\*
\* Not modelled: the tables' size cap (MAX_DOC_CLAIMS). The model's memory
\* of completed heads never fills, so "no block is sent again" holds only
\* while the node remembers fewer documents than the cap. Past it, the code
\* clears the whole memory (counted by dag_fetch_doc_completed_cleared), and
\* a document's next fetch may be sent history again, which
\* car_received_already_held shows. A fetch past a full claim table runs
\* unguarded (counted by dag_fetch_doc_claims_full).
\*
\* Rust: crates/p2p/src/sync/coordinator/doc_claim.rs DagFetchLimiter::
\* claim_doc (the claim, the wait on the running fetch's Event, the read of
\* doc_completed once the claim is held) and DocClaim's Drop (record the
\* completed roots, remove the claim, set the Event);
\* crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag (claim,
\* name the handed roots as haves, record a Complete root); the provider
\* side is ExactDagFetch.tla.
EXTENDS Naturals, FiniteSets

\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "noguard"    fetches never claim the document -> INV_OneFetchPerDoc
\*   "nomemory"   completed heads are not remembered -> INV_HeldNotResent
\*   "recordlate" the head is recorded after the claim is freed -> INV_HeldNotResent
\*   "nowake"     a finished fetch never sets its signal -> INV_NoLostWakeup
CONSTANTS Bug

\* The heads, one fetch task each. A task is named by its head.
Heads == 1..3

\* Every block a head's history holds: head i holds blocks 1 to i.
Hist(i) == 1..i

\* Every block the histories of a set of heads hold.
HistOf(S) == UNION {Hist(i) : i \in S}

\* st: where each task is ("idle", "waiting", "fetching", "late" recording
\* after it freed the claim, "done"). owner: the task holding the claim, 0
\* when free. waitOn: whose signal a waiting task waits on. ended: the tasks
\* whose signal is set. completed: the heads remembered as completed.
\* merged: the heads whose merge ran. haves: what each fetch names. held:
\* the blocks the receiver holds. resent: blocks sent that it already held.
VARIABLES st, owner, waitOn, ended, completed, merged, haves, held, resent

\* Every variable, for UNCHANGED.
vars == <<st, owner, waitOn, ended, completed, merged, haves, held, resent>>

\* Every task idle, nothing held, nothing claimed, remembered or merged.
Init ==
  /\ st = [t \in Heads |-> "idle"]
  /\ owner = 0
  /\ waitOn = [t \in Heads |-> 0]
  /\ ended = {}
  /\ completed = {}
  /\ merged = {}
  /\ haves = [t \in Heads |-> {}]
  /\ held = {}
  /\ resent = 0

\* What a fetch names as haves when it starts: remembered and merged heads.
\* "nomemory" forgets the remembered ones.
Known == (IF Bug = "nomemory" THEN {} ELSE completed) \cup merged

\* An idle task finds the document free and claims it, then names its haves.
\* "noguard" skips the claim: the task fetches whatever else runs.
Claim(t) ==
  \* Only a task that has not started yet.
  /\ st[t] = "idle"
  \* The document is free, unless the guard is gone.
  /\ (owner = 0 \/ Bug = "noguard")
  \* The task holds the claim now (with no guard, nothing is held).
  /\ owner' = IF Bug = "noguard" THEN owner ELSE t
  \* It fetches.
  /\ st' = [st EXCEPT ![t] = "fetching"]
  \* It names what is known complete at this moment.
  /\ haves' = [haves EXCEPT ![t] = Known]
  \* Nothing else moves.
  /\ UNCHANGED <<waitOn, ended, completed, merged, held, resent>>

\* An idle task finds another fetching the document and waits on its signal.
Wait(t) ==
  \* Only a task that has not started yet, with the guard in place.
  /\ st[t] = "idle"
  /\ Bug # "noguard"
  \* Another task holds the claim.
  /\ owner # 0
  \* The task waits on that holder's signal.
  /\ st' = [st EXCEPT ![t] = "waiting"]
  /\ waitOn' = [waitOn EXCEPT ![t] = owner]
  \* Nothing else moves.
  /\ UNCHANGED <<owner, ended, completed, merged, haves, held, resent>>

\* A waiting task's signal is set: it wakes and tries to claim again.
Wake(t) ==
  \* The task is waiting and its signal is set.
  /\ st[t] = "waiting"
  /\ waitOn[t] \in ended
  \* It is idle again, so Claim or Wait is its next step.
  /\ st' = [st EXCEPT ![t] = "idle"]
  \* Nothing else moves.
  /\ UNCHANGED <<owner, waitOn, ended, completed, merged, haves, held, resent>>

\* A fetching task's response arrives and the fetch completes. The provider
\* sends the head's history less its haves' histories; the receiver counts
\* what it already held. Then the task records its head, frees the claim
\* and sets its signal ("recordlate" records only afterwards, "nowake"
\* never sets the signal).
Finish(t) ==
  \* Only a task that is fetching.
  /\ st[t] = "fetching"
  \* The blocks the provider sends.
  /\ LET sent == Hist(t) \ HistOf(haves[t])
     \* Each one the receiver already held is counted.
     IN /\ resent' = resent + Cardinality(sent \cap held)
        \* The receiver now holds the head's whole history.
        /\ held' = held \cup Hist(t)
  \* The head is remembered as completed, unless recorded late.
  /\ completed' = IF Bug = "recordlate" THEN completed ELSE completed \cup {t}
  \* The claim is freed (a fetch with no guard held none).
  /\ owner' = IF owner = t THEN 0 ELSE owner
  \* The signal is set, unless it never is.
  /\ ended' = IF Bug = "nowake" THEN ended ELSE ended \cup {t}
  \* The task is done, or still has its head to record.
  /\ st' = [st EXCEPT ![t] = IF Bug = "recordlate" THEN "late" ELSE "done"]
  \* Nothing else moves.
  /\ UNCHANGED <<waitOn, merged, haves>>

\* A task that freed its claim before recording its head records it now.
Record(t) ==
  \* Only a task with its head still to record.
  /\ st[t] = "late"
  \* The head is remembered, and the task is done.
  /\ completed' = completed \cup {t}
  /\ st' = [st EXCEPT ![t] = "done"]
  \* Nothing else moves.
  /\ UNCHANGED <<owner, waitOn, ended, merged, haves, held, resent>>

\* The merge of a fetched head runs, at any time after its fetch.
Merge(t) ==
  \* The head's fetch is over and its merge has not run.
  /\ st[t] \in {"late", "done"}
  /\ t \notin merged
  \* The head is merged.
  /\ merged' = merged \cup {t}
  \* Nothing else moves.
  /\ UNCHANGED <<st, owner, waitOn, ended, completed, haves, held, resent>>

\* One step of some task.
Next == \E t \in Heads : Claim(t) \/ Wait(t) \/ Wake(t) \/ Finish(t)
                         \/ Record(t) \/ Merge(t)

\* The fetches, from the burst.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  /\ st \in [Heads -> {"idle", "waiting", "fetching", "late", "done"}]
  /\ owner \in Heads \cup {0}
  /\ completed \subseteq Heads
  /\ held \subseteq HistOf(Heads)

\* At most one fetch of the document runs at a time.
INV_OneFetchPerDoc == Cardinality({t \in Heads : st[t] = "fetching"}) <= 1

\* No block the receiver already holds is sent to it again.
INV_HeldNotResent == resent = 0

\* A waiting task's signal is set, or its holder is still fetching and will
\* set it.
INV_NoLostWakeup ==
  \A t \in Heads :
    st[t] = "waiting" => (waitOn[t] \in ended \/ st[waitOn[t]] = "fetching")
=============================================================================
