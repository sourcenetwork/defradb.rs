------------------------ MODULE HttpRouterComposition ------------------------
\* A server builds its native router, merges extensions, then wraps the result.
\* The wrapper is optional and is constructed once for each complete router.
\* Rust: crates/http/src/server.rs::Server::router and Server::serve.
EXTENDS Naturals, FiniteSets, TLC
\* Negative runs move or duplicate the callback and expose a missing fence.
CONSTANT Bug
\* Request origins include misses, which pass through the normalized fallback.
Origins == {"native", "application", "missing"}
\* Only assembly order and forwarding are modeled; callback code is trusted.
VARIABLES configured, allowed, origin, phase, guarded, constructors, outcome
\* Each transition retains one complete construction and request observation.
vars == <<configured, allowed, origin, phase, guarded, constructors, outcome>>
\* Both configured and default servers start before any router is assembled.
Init == /\ configured \in BOOLEAN
        /\ allowed \in BOOLEAN
        /\ origin \in Origins
        /\ phase = "native"
        /\ guarded = {}
        /\ constructors = 0
        /\ outcome = "none"
\* Extensions are merged before the optional callback sees the router.
Merge == /\ phase = "native"
         /\ phase' = "merged"
         /\ UNCHANGED <<configured, allowed, origin, guarded, constructors, outcome>>
\* A callback applied too early misses application routes added afterwards.
EarlyScope == IF Bug = "early" THEN {"native", "missing"} ELSE Origins
\* Wrapping only matched handlers lets the fallback bypass the application.
Scope == IF Bug = "fallback" THEN EarlyScope \ {"missing"} ELSE EarlyScope
\* One complete router is wrapped once, with no special exemption for misses.
Wrap == /\ phase = "merged"
        /\ phase' = "ready"
        /\ guarded' = IF configured THEN Scope ELSE {}
        /\ constructors' = IF configured THEN (IF Bug = "twice" THEN 2 ELSE 1) ELSE 0
        /\ UNCHANGED <<configured, allowed, origin, outcome>>
\* A forwarding filter either refuses first or returns the unchanged inner result.
Serve == /\ phase = "ready"
         /\ phase' = "served"
         /\ outcome' = IF configured /\ (~allowed \/ Bug = "overdeny") /\ origin \in guarded
                        THEN "refused" ELSE "inner"
         /\ UNCHANGED <<configured, allowed, origin, guarded, constructors>>
\* No request can be dispatched before merge and wrapper construction finish.
Next == Merge \/ Wrap \/ Serve
\* A configured wrapper covers core routes, extensions and the fallback.
WholeRouter == phase \in {"ready", "served"} /\ configured => guarded = Origins
\* Every router build constructs its configured callback exactly once.
SingleConstructor == phase \in {"ready", "served"} =>
                       constructors = (IF configured THEN 1 ELSE 0)
\* A refused authority cannot escape via an extension or normalized miss.
NoBypass == phase = "served" /\ configured /\ ~allowed => outcome = "refused"
\* Without a refusal the original route keeps its own authorization outcome.
ForwardingPreserved == phase = "served" /\ (~configured \/ allowed) => outcome = "inner"
\* Assembly is finite; terminal stuttering preserves these safety properties.
Spec == Init /\ [][Next]_vars
=============================================================================
