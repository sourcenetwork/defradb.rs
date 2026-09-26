---- MODULE MC_GovernanceContract_Orphan ----
\* A composite that arrives before the inputs its verdict needs, pinning
\* none of them (Self = {}) and holding nothing that references them, so
\* its Defer can name nothing and no composite arrival re-drives it. The
\* case DeferredMerges::defer drops on the floor (`awaiting.is_empty()`).
\*
\*   w1  the orphan. Needs "seal" and "anchor" (a device's seal of its
\*       write and a tower's anchor of that seal, in Fefra's terms); pins
\*       neither.
\*   w2  a composite that rejects on present bytes, decided on sight
\*       whatever the replica holds. Present to keep "reject is local" and
\*       "reject is intrinsic" non-vacuous.
\*
\* "anchor" names its target "seal" (Refs), so a replica that already holds
\* the anchor CAN name the seal: the arrival order that does not exhibit
\* the gap. TLC explores both; the gap is a property of the arrival order,
\* not of the composite.
EXTENDS GovernanceContract

mcReplicas == {"p", "q"}
mcEntries  == {"seal", "anchor"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"Ldev", "Ltower"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"seal", "anchor"} ELSE {}]
mcSelf == [w \in mcWrites |-> {}]
mcRefs == [e \in mcEntries |-> IF e = "anchor" THEN {"seal"} ELSE {}]
mcLog == [e \in mcEntries |-> IF e = "anchor" THEN "Ltower" ELSE "Ldev"]

\* With field keys the composite awaits the writer's log and the covering
\* anchorer's log, so any new entry of either re-judges it.
mcAwaitLogs == [w \in mcWrites |-> IF w = "w1" THEN {"Ldev", "Ltower"} ELSE {}]

mcBad == {"w2"}
mcDeliverable == mcEntries

\* ---- emission and the local write path: off in this instance ----
mcRecords == {}
mcFacts == [w \in mcWrites |-> {}]
mcFactNeeds == [f \in mcRecords |-> {}]
mcAuthored == {}
mcOwn == [w \in mcWrites |-> {}]
mcBatch == [w \in mcWrites |-> {}]
====
