---- MODULE MC_InboundAdmission_Common ----
EXTENDS InboundAdmission
\* Shared values for the InboundAdmission configs.
\* Connection a sends two bulk messages (a1, a4) and one control message (a2)
\* whose body it stops sending halfway. Connections b and c send one bulk
\* message each. Every bulk handler never returns. Connection h sends one
\* control message: the one we promise to handle.
\* Control holds one body, bulk two; a connection may hold one body per class.
\* Bulk events are completions (CAR responses); control events are requests.
mcStreams == {"a1", "a4", "b1", "c1", "a2", "h1"}
mcConnOf == [s \in mcStreams |-> CASE s \in {"a1", "a4", "a2"} -> "a"
                                    [] s = "b1" -> "b"
                                    [] s = "c1" -> "c"
                                    [] s = "h1" -> "h"]
mcBulk == {"a1", "a4", "b1", "c1"}
mcClassOf == [s \in mcStreams |-> IF s \in mcBulk THEN "bulk" ELSE "control"]
mcLaneOf == [s \in mcStreams |-> IF s \in mcBulk THEN "completion" ELSE "admission"]
mcClasses == {"control", "bulk"}
mcLanes == {"admission", "completion"}
mcStalls == {"a2"}
mcStuckClasses == {"bulk"}
mcBudget == [c \in mcClasses |-> IF c = "control" THEN 1 ELSE 2]
mcSlots == [l \in mcLanes |-> 1]
====
