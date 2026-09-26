---- MODULE MC_GovernanceMerge_Emit ----
\* The mechanism driven with the orphan and the two facts of
\* MC_GovernanceContract_Emit: a reject record and a fork receipt found
\* during a defer.
EXTENDS GovernanceMerge

mcReplicas == {"p", "q"}
mcEntries  == {"seal", "anchor"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"Ldev", "Ltower"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"seal", "anchor"} ELSE {}]
mcSelf  == [w \in mcWrites |-> {}]
mcRefs  == [e \in mcEntries |-> IF e = "anchor" THEN {"seal"} ELSE {}]
mcLog   == [e \in mcEntries |-> IF e = "anchor" THEN "Ltower" ELSE "Ldev"]
mcAwaitLogs == [w \in mcWrites |-> IF w = "w1" THEN {"Ldev", "Ltower"} ELSE {}]
mcBad == {"w2"}
mcDeliverable == mcEntries

mcRecords == {"fork", "badw2"}
mcFacts == [w \in mcWrites |-> IF w = "w1" THEN {"fork"} ELSE {"badw2"}]
mcFactNeeds == [f \in mcRecords |-> IF f = "fork" THEN {"seal"} ELSE {}]
====
