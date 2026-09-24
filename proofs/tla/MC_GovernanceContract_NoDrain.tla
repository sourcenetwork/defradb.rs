---- MODULE MC_GovernanceContract_NoDrain ----
\* The nameable instance with the re-drive queue never drained: the
\* per-drain budget (REDRIVE_BUDGET) ran out and nothing came back for the
\* leftovers, no further arrival for that CID and no sweep. DrainOne stays
\* enabled; only its fairness is dropped.
\*
\* This is the case INV_NoStuck cannot catch as a state invariant: the
\* composite sits in the queue, so "deferred with nothing queued" is false
\* at every state and the invariant passes while the composite never
\* settles.
EXTENDS MC_GovernanceContract_Nameable

FairnessNoDrain ==
  /\ \A r \in Replicas, e \in Entries : WF_vars(DeliverEntry(r, e))
  /\ \A r \in Replicas, w \in Writes  : WF_vars(DeliverWrite(r, w))

SpecNoDrain == Init /\ [][Next]_vars /\ FairnessNoDrain
====
