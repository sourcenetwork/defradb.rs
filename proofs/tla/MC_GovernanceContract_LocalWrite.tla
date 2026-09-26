---- MODULE MC_GovernanceContract_LocalWrite ----
\* Two writes this node authors. w1 needs a grant that the same batch wrote
\* before it. w2 is a rule that counts its own document: the document w2
\* creates is among what w2 needs, which no peer holds while judging w2.
EXTENDS GovernanceContract

mcReplicas == {"p", "q"}
mcEntries  == {"grant", "self2"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"L"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"grant"} ELSE {"self2"}]
mcSelf  == [w \in mcWrites |-> {}]
mcRefs  == [e \in mcEntries |-> {}]
mcLog   == [e \in mcEntries |-> "L"]
mcAwaitLogs == [w \in mcWrites |-> {}]
mcBad == {}
mcDeliverable == {"grant"}

mcRecords == {}
mcFacts == [w \in mcWrites |-> {}]
mcFactNeeds == [f \in mcRecords |-> {}]
mcAuthored == mcWrites
mcOwn == [w \in mcWrites |-> IF w = "w2" THEN {"self2"} ELSE {}]
mcBatch == [w \in mcWrites |-> IF w = "w1" THEN {"grant"} ELSE {}]
====
