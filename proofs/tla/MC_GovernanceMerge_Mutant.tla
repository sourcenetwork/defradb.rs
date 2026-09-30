---- MODULE MC_GovernanceMerge_Mutant ----
\* A teeth check for the refinement mapping, not a claim about the merge
\* path.
\*
\* A refinement that passes tells you something only if it CAN fail. This
\* module adds one bug to the otherwise-correct mechanism, a composite
\* merged without the validator being consulted (the "silent accept"), and
\* asserts that REFINES_Contract catches it on safety, not merely on
\* liveness.
\*
\* If this run ever goes green, the mapping has stopped observing the merged
\* set and every other GovernanceMerge result is worthless.
EXTENDS MC_GovernanceMerge_Orphan

SilentMerge(r, w) ==
  /\ w \in Unmerged(r)
  /\ merged' = [merged EXCEPT ![r] = @ \cup {w}]
  /\ UNCHANGED << store, pushed, quarantined, pending, pendingLogs, sweepQ, crashes, emitQ, records, drainDue >>

MNext == DNext \/ \E r \in Replicas, w \in Writes : SilentMerge(r, w)

DSpecMutant == DInit /\ [][MNext]_dvars /\ DFairness
====
