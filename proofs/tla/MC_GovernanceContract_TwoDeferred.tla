---- MODULE MC_GovernanceContract_TwoDeferred ----
\* Two composites that both defer with a nameable await, for the defer-index
\* capacity (MAX_DEFERRED_COMPOSITES). Each pins exactly one input, so each
\* occupies exactly one index slot. With MaxIndexed = 1 the second to defer
\* cannot be indexed at all and only the sweep reaches it: with the sweep it
\* settles, without it, it does not.
EXTENDS GovernanceContract

mcReplicas == {"p", "q"}
mcEntries  == {"a", "b"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"La", "Lb"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"a"} ELSE {"b"}]
mcSelf  == [w \in mcWrites |-> IF w = "w1" THEN {"a"} ELSE {"b"}]
mcRefs  == [e \in mcEntries |-> {}]
mcLog   == [e \in mcEntries |-> IF e = "a" THEN "La" ELSE "Lb"]
mcAwaitLogs == [w \in mcWrites |-> {}]

mcBad == {}
mcDeliverable == mcEntries

\* ---- emission and the local write path: off in this instance ----
mcRecords == {}
mcFacts == [w \in mcWrites |-> {}]
mcFactNeeds == [f \in mcRecords |-> {}]
mcAuthored == {}
mcOwn == [w \in mcWrites |-> {}]
mcBatch == [w \in mcWrites |-> {}]
====
