---------------------------- MODULE ProjectionActor ----------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES source, node, owner, actor, visible
vars == <<source, node, owner, actor, visible>>
Init == /\ source \in {"actorless", "node", "foreign"}
        /\ node = "node" /\ owner \in {"node", "foreign"}
        /\ actor = "unset" /\ visible = FALSE
Select == /\ actor = "unset"
          /\ actor' = IF source = "actorless" THEN node ELSE source
          /\ UNCHANGED <<source, node, owner, visible>>
Grant == /\ actor # "unset" /\ ~visible
         /\ visible' = (actor = owner \/ Bug)
         /\ UNCHANGED <<source, node, owner, actor>>
Next == Select \/ Grant
Spec == Init /\ [][Next]_vars
INV_Identity == actor # "unset" => actor = IF source = "actorless" THEN node ELSE source
INV_Authority == visible => actor = owner
=============================================================================
