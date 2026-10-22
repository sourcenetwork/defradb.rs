---- MODULE MC_InboundAdmission_Common ----
EXTENDS InboundAdmission
\* Shared values for the InboundAdmission configs.
\* Peer a sends two bulk messages, a1 on connection a and a4 on a second
\* connection a', and one control message (a2) whose body it trickles in below
\* the minimum rate, one byte often enough that the idle timer never fires.
\* Peers b and c send one bulk message each. Every bulk handler never returns.
\* Peer h sends one control message: the one we promise to handle.
\* Control holds one body, bulk two; a peer may hold one body per class.
\* Bulk events are completions (CAR responses); control events are requests.
mcStreams == {"a1", "a4", "b1", "c1", "a2", "h1"}
mcConnOf == [s \in mcStreams |-> CASE s \in {"a1", "a2"} -> "a"
                                    [] s = "a4" -> "a'"
                                    [] s = "b1" -> "b"
                                    [] s = "c1" -> "c"
                                    [] s = "h1" -> "h"]
mcPeerOf == [k \in {"a", "a'", "b", "c", "h"} |-> IF k = "a'" THEN "a" ELSE k]
mcBulk == {"a1", "a4", "b1", "c1"}
mcClassOf == [s \in mcStreams |-> IF s \in mcBulk THEN "bulk" ELSE "control"]
mcLaneOf == [s \in mcStreams |-> IF s \in mcBulk THEN "completion" ELSE "admission"]
mcClasses == {"control", "bulk"}
mcLanes == {"admission", "completion"}
mcStalls == {}
mcTrickles == {"a2"}
mcStuckClasses == {"bulk"}
mcBudget == [c \in mcClasses |-> IF c = "control" THEN 1 ELSE 2]
mcSlots == [l \in mcLanes |-> 1]
====
