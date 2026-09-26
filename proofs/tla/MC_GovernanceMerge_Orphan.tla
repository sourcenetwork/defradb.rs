---- MODULE MC_GovernanceMerge_Orphan ----
\* The merge path driven with the orphan: a composite pushed before the
\* inputs its verdict needs, so the plugin's Defer names no CID and the
\* index has nothing to file it under. The same content as
\* MC_GovernanceContract_Orphan, so the two layers are compared on one case.
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

\* ---- emission and the local write path: off in this instance ----
mcRecords == {}
mcFacts == [w \in mcWrites |-> {}]
mcFactNeeds == [f \in mcRecords |-> {}]
mcAuthored == {}
mcOwn == [w \in mcWrites |-> {}]
mcBatch == [w \in mcWrites |-> {}]
====
