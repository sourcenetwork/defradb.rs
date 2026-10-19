---- MODULE MC_InboundAdmission_Common ----
EXTENDS InboundAdmission
\* Shared values for the InboundAdmission configs.
\* Peers a, b and c each send one bulk message whose handler never returns.
\* Peer a also opens two control streams and stops halfway through both bodies.
\* Peer h sends one control message: the one we promise to handle.
\* Each class holds two bodies and one handler; a peer may hold one body per class.
mcStreams == {"a1", "b1", "c1", "a2", "a3", "h1"}
mcPeerOf == [s \in mcStreams |-> CASE s \in {"a1", "a2", "a3"} -> "a"
                                    [] s = "b1" -> "b"
                                    [] s = "c1" -> "c"
                                    [] s = "h1" -> "h"]
mcClassOf == [s \in mcStreams |-> IF s \in {"a1", "b1", "c1"} THEN "bulk" ELSE "control"]
mcClasses == {"control", "bulk"}
mcStalls == {"a2", "a3"}
mcStuckClasses == {"bulk"}
mcBudget == [c \in mcClasses |-> 2]
mcSlots == [c \in mcClasses |-> 1]
====
