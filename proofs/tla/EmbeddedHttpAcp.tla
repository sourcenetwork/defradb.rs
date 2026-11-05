------------------------ MODULE EmbeddedHttpAcp ------------------------
EXTENDS Naturals, TLC
CONSTANT Bug
VARIABLES caller, bearer, permitted, phase, nativeCaller, nativeBearer, changed
vars == <<caller, bearer, permitted, phase, nativeCaller, nativeBearer, changed>>
Init == /\ caller \in {"owner", "stranger"}
        /\ bearer = caller
        /\ permitted = (caller = "owner")
        /\ phase = "request"
        /\ nativeCaller = "none"
        /\ nativeBearer = "none"
        /\ changed = FALSE
Apply == /\ phase = "request"
         /\ nativeCaller' = IF Bug = "actor" THEN "owner" ELSE caller
         /\ nativeBearer' = IF Bug = "bearer" THEN "none" ELSE bearer
         /\ changed' = IF Bug = "bypass" THEN TRUE ELSE permitted
         /\ phase' = "done"
         /\ UNCHANGED <<caller, bearer, permitted>>
Next == Apply
CallerPreserved == phase = "done" => nativeCaller = caller
BearerPreserved == phase = "done" => nativeBearer = bearer
NoUnauthorizedChange == changed => permitted
Spec == Init /\ [][Next]_vars
=============================================================================
