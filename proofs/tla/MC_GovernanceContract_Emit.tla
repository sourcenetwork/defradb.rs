---- MODULE MC_GovernanceContract_Emit ----
\* The orphan case with two facts a verdict can find: one rests on the
\* rejected composite's own bytes (a reject record), one on the seal alone,
\* so it is found during a verdict that ends in a defer (a fork receipt).
EXTENDS GovernanceContract

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
mcAuthored == {}
mcOwn == [w \in mcWrites |-> {}]
mcBatch == [w \in mcWrites |-> {}]
====
