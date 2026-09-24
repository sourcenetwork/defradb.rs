---- MODULE MC_GovernanceContract_Withheld ----
\* An input replication will never deliver: withheld by a replication
\* policy or not yet in existence. Withholding is safe for replicated state
\* only if the peer's merge path defers, so the composite that needs it
\* must sit deferred forever and must NOT be rejected.
\*
\*   w1  needs "gone", which is not Deliverable. Not settleable: it defers
\*       forever, and that is correct, not a liveness failure.
\*   w2  needs only "here". Settleable, and must still settle although the
\*       replica is permanently missing another input.
EXTENDS GovernanceContract

mcReplicas == {"p", "q"}
mcEntries  == {"here", "gone"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"Lhere", "Lgone"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"here", "gone"} ELSE {"here"}]
mcSelf  == [w \in mcWrites |-> IF w = "w1" THEN {"here", "gone"} ELSE {"here"}]
mcRefs  == [e \in mcEntries |-> {}]
mcLog   == [e \in mcEntries |-> IF e = "here" THEN "Lhere" ELSE "Lgone"]
mcAwaitLogs == [w \in mcWrites |-> {}]

mcBad == {}
mcDeliverable == {"here"}
====
