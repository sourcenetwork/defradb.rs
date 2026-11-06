---- MODULE MC_PushLogAdmission_Common ----
EXTENDS PushLogAdmission
\* Shared model values for the PushLogAdmission GREEN/RED configs. The heads are one
\* peer's pushes - see PushLogAdmission.tla.
mcHeads == {1, 2, 3}
\* Independent pushes: three heads with no parents, each its own scope, against one
\* pending slot, so admission overflow is reachable with concurrent inflight pushes.
mcCap == 1
mcNoParents == [h \in mcHeads |-> {}]
mcFlatPrio == [h \in mcHeads |-> 1]
mcOwnScope == [h \in mcHeads |-> h]
\* Sibling heads: 2 and 3 both link 1 and share its priority, and all three come from
\* one sender scope, as when a hub forwards two concurrent writes of one document.
\* Two slots, so both siblings can be pending at once.
mcSiblingCap == 2
mcSiblingParents == [h \in mcHeads |-> IF h = 1 THEN {} ELSE {1}]
mcSiblingPrio == [h \in mcHeads |-> IF h = 1 THEN 1 ELSE 2]
mcOneScope == [h \in mcHeads |-> "hub"]
\* The share configs: two durable slots, of which the pushing peer may hold one.
mcShareCap == 2
mcQuota    == 1
====
