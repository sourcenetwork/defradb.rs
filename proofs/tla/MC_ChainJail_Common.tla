---- MODULE MC_ChainJail_Common ----
EXTENDS ChainJail
\* Three honest nodes in a line, n3 - n1 - n2, and a cheater "c" that can reach
\* all of them. The line is the point: when the cheater shows one side of its
\* fork to n1 and the other to n2, n3 only ever hears from n1, and n1 stops
\* merging (and so relaying) the cheater once it sees the fork.

mcHonest == {"n1", "n2", "n3"}
mcCheater == "c"
mcPeers == [n \in mcHonest |-> CASE n = "n1" -> {"n2", "n3"}
                                [] n = "n2" -> {"n1"}
                                [] OTHER -> {"n1"}]
\* One height is enough for a fork; a second only grows the state space.
mcHeights == {1}
====
