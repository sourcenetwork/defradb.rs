---- MODULE MC_GovernanceHeads ----
\* One composite the validator rejects, marked as a local-only downsample
\* source, and a collection block linking it; one initial definition and a
\* patch of it, which may arrive first.
EXTENDS GovernanceHeads

mcWrites == {"w1", "w2"}
mcVerdict == [w \in mcWrites |-> IF w = "w1" THEN "Reject" ELSE "Accept"]
mcBlocks == {"b1", "b2"}
mcLinks == [b \in mcBlocks |-> IF b = "b1" THEN {"w1"} ELSE {"w2"}]
mcDefs == {"d1", "d2"}
mcPrev == [d \in mcDefs |-> IF d = "d2" THEN "d1" ELSE "none"]
mcDownsampled == {"w1"}
====
