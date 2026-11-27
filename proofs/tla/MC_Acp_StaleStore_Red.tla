---- MODULE MC_Acp_StaleStore_Red ----
\* RED: the same scenario as MC_Acp_Green, but a node that receives the
\* revocation keeps alice's tuple in its copy, so it still lets her read.
EXTENDS Acp

\* The owner node and one replica.
mcNodes == {"owner", "replica"}
\* One permission: alice may read doc1.
mcTuples == {"doc1#reader@alice"}
\* The authority holds it at the start.
mcInitialAuthority == mcTuples
\* Both nodes already hold it in their copies.
mcInitiallyKnown == [n \in mcNodes |-> mcTuples]
\* Nothing new is granted in this scenario.
mcGrantable == {}
\* The one tuple is revoked.
mcRevocable == mcTuples
====
