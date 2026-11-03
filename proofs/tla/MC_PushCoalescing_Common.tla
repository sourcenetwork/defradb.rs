---- MODULE MC_PushCoalescing_Common ----
EXTENDS PushCoalescing
mcPeers == {"peer"}
mcMaxHead == 3
\* One writer: each head links the one before it.
mcLinear == [h \in 1..mcMaxHead |-> IF h = 1 THEN {} ELSE {h - 1}]
\* A hub that merged two writers' heads: 2 and 3 both link 1, and neither links the other.
mcSiblings == [h \in 1..mcMaxHead |-> IF h = 1 THEN {} ELSE {1}]
====
