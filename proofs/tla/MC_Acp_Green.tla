---- MODULE MC_Acp_Green ----
\* GREEN: both nodes start holding alice's reader tuple; the authority
\* revokes it, the revocation reaches each node, and each drops the tuple.
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
