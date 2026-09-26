---- MODULE MC_GovernanceContract_Nameable ----
\* The arrival order the defer index was written for: every input the
\* verdict still needs is named by something the replica already has, so
\* each arrival re-drives the composite and names the next gap. A
\* three-link chain:
\*
\*   the composite pins its first input     (Self)
\*   a held "est" names "seal"              (Refs)
\*   a held "seal" names "anchor"
\*
\* The control for the RED runs: here arrival re-drive alone carries the
\* composite to a verdict with no sweep at all.
EXTENDS GovernanceContract

mcReplicas == {"p", "q"}
mcEntries  == {"est", "seal", "anchor"}
mcWrites   == {"w1", "w2"}
mcLogs     == {"Ldev", "Ltower"}

mcNeeds == [w \in mcWrites |-> IF w = "w1" THEN {"est", "seal", "anchor"} ELSE {}]
mcSelf == [w \in mcWrites |-> IF w = "w1" THEN {"est"} ELSE {}]
mcRefs == [e \in mcEntries |->
             CASE e = "est"  -> {"seal"}
               [] e = "seal" -> {"anchor"}
               [] OTHER      -> {}]
mcLog == [e \in mcEntries |-> IF e = "anchor" THEN "Ltower" ELSE "Ldev"]
mcAwaitLogs == [w \in mcWrites |-> {}]

mcBad == {"w2"}
mcDeliverable == mcEntries
====
