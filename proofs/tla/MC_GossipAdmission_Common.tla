---- MODULE MC_GossipAdmission_Common ----
\* Shared values for the GossipAdmission configs: three documents against one
\* map slot, so a refusal is reachable; two from peer "a" and one from peer
\* "b", so one peer's budget can fill while the other's has room. One crash,
\* so memory-only state can be lost.
EXTENDS GossipAdmission
\* Three write-once documents, each announced once.
mcDocs == {1, 2, 3}
\* Two origin peers.
mcPeers == {"a", "b"}
\* Documents 1 and 2 come from "a", document 3 from "b".
mcPeerOf == [d \in mcDocs |-> IF d = 3 THEN "b" ELSE "a"]
====
