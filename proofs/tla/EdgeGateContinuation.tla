---- MODULE EdgeGateContinuation ----
\* The story, in plain words.
\*
\* The network edge admits work through a gate (crates/foundation/edge/src/gate.rs). A
\* gate counts what it admitted. Shutdown closes the gate, so it admits
\* nothing new, and the drain waits for the count to reach zero before the
\* rest is stopped.
\*
\* A request already admitted may need more work to finish: an outbound
\* connection to the KMS or to Vera, whose driver must run as its own task
\* (net::rt::ClientExecutor). That task continues the request, so the gate
\* admits it after closing as long as admitted work is still in flight
\* (Gate::admit_continuation, Edge::spawn_continuation).
\*
\* Who acts:
\*   * a request: admitted, holds its permit, may ask for a continuation, and
\*     lets its permit go when it ends;
\*   * a stray continuation, asked for by nothing the gate admitted (a pooled
\*     client's idle sweep);
\*   * two newcomers, new top-level work (a fresh inbound request, a task a
\*     caller outside the edge spawns): each tries a plain admission
\*     (Gate::admit), counting itself first and then reading `closed`. Before
\*     closing it is admitted, runs, ends, and may come back as the next
\*     request, for ever. After closing it backs out, and a caller told
\*     Closed does not try again;
\*   * the closer, which closes the gate once;
\*   * the drain, which waits for zero, seals the gate, looks again, and only
\*     then says the gate is drained. The drain's report (crates/foundation/edge/src/handle.rs
\*     drain_gate) is that result itself, not a later look at the count.
\*
\* What must never happen:
\*   1. once the drain said the gate is drained, a permit is still held -
\*      INV_NothingAfterDrain;
\*   2. a request in flight asks for a continuation and is refused -
\*      INV_InFlightContinues;
\*   3. after closing, a continuation is admitted when nothing was in flight -
\*      INV_NoContinuationWhenIdle;
\*   4. the drain never says drained, although every piece of work it waits
\*      for finishes on its own - LIVE_DrainEnds. A stream of new top-level
\*      work admitted after closing would keep the count above zero for ever.
\*      (Work that hangs is cut off by the drain's deadline, handle.rs
\*      drain_gate; ServerDrain.tla models that deadline.)
\*
\* The Rust code each guard models:
\*   Seal      Gate::drain sets `sealed` before its last look at the count, and
\*             Gate::admit_continuation reads `sealed` after it counted itself
\*             and backs out when it is set;
\*   Cas       Gate::admit_continuation counts itself with a compare-and-swap
\*             that only succeeds on a count above zero;
\*   Continue  net::rt::ClientExecutor spawns a connection driver through
\*             Edge::spawn_continuation, not through a plain spawn;
\*   Refuse    Gate::admit backs out once it reads `closed`, so new top-level
\*             work is refused while the drain runs.

EXTENDS Naturals

CONSTANTS
  Seal,      \* TRUE: the drain seals before its last look, and a continuation checks the seal
  Cas,       \* TRUE: a continuation counts itself only while the count is above zero
  Continue,  \* TRUE: a continuation is admitted through the continuation path
  Refuse     \* TRUE: a plain admission backs out once the gate is closed

VARIABLES
  count,     \* the gate's count of admitted work, transient counts included
  closed,    \* the gate is closed
  sealed,    \* the drain sealed the gate
  drain,     \* the drain: "wait" (for zero), "look" (sealed, looking again), "done" (said drained)
  req,       \* the request: "out", "counted" (admit counted, closed not read yet), "held", "gone"
  rc,        \* the request's continuation: "none", "counted", "held", "refused", "gone"
  sc,        \* the stray continuation: "none", "counted", "held", "refused", "gone"
  fresh,     \* each newcomer: "none" (not trying), "counted", "held", "refused" (told Closed)
  rz,        \* (ghost) the request's continuation counted itself in from zero after closing
  sz,        \* (ghost) the stray counted itself in from zero after closing
  idleAdmit, \* (record) a continuation was admitted after closing with nothing in flight
  liveRefused \* (record) the request's continuation was refused while the request held its permit

vars == <<count, closed, sealed, drain, req, rc, sc, fresh, rz, sz, idleAdmit, liveRefused>>  \* every variable

Newcomers == {1, 2}              \* two newcomers: enough for one to arrive while the other runs

Init ==                          \* at the start:
  /\ count = 0                   \* nothing admitted
  /\ closed = FALSE              \* the gate admits work
  /\ sealed = FALSE              \* not sealed
  /\ drain = "wait"              \* the drain has not said drained
  /\ req = "out"                 \* the request not admitted yet
  /\ rc = "none"                 \* no continuation asked for
  /\ sc = "none"                 \* the stray not asked for
  /\ fresh = [n \in Newcomers |-> "none"]  \* no newcomer trying
  /\ rz = FALSE /\ sz = FALSE    \* no continuation counted yet
  /\ idleAdmit = FALSE           \* nothing went wrong
  /\ liveRefused = FALSE         \* nothing went wrong

\* A plain admission, first step: it counts itself (Gate::admit).
ReqCount ==
  /\ req = "out"                                        \* not admitted yet
  /\ req' = "counted" /\ count' = count + 1             \* counted
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* A plain admission, second step: it reads `closed`, and backs out if it is set.
ReqDecide ==
  /\ req = "counted"                                    \* counted itself
  /\ IF closed
       THEN req' = "gone" /\ count' = count - 1         \* refused: backs out
       ELSE req' = "held" /\ UNCHANGED count            \* admitted: holds its permit
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The request ends and lets its permit go.
ReqEnd ==
  /\ req = "held"                                       \* holding its permit
  /\ req' = "gone" /\ count' = count - 1                \* let go
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* A continuation's first step, for the request (`who` = "req") or the stray (`who` = "stray"),
\* with `z` its ghost. Through the continuation path it counts itself, with a compare-and-swap
\* when Cas holds; through a plain spawn it is the plain admission.
ContCount(c, z, who) ==
  /\ c = "none"                                         \* not asked for yet
  /\ who = "req" => req = "held"                        \* the request asks while it holds its permit
  /\ IF Continue /\ Cas /\ closed /\ count = 0           \* MUTATION no_cas: the compare-and-swap fails on zero
       THEN /\ c' = "refused" /\ UNCHANGED count        \* refused, nothing counted
       ELSE /\ c' = "counted" /\ count' = count + 1     \* counted
  /\ z' = (closed /\ count = 0)                         \* noted: counted in from zero after closing
  /\ UNCHANGED <<closed, sealed, drain, req, fresh, idleAdmit, liveRefused>>  \* the rest stays

\* A continuation's second step: it reads the seal (continuation path) or `closed` (plain spawn).
ContDecide(c, z, who) ==
  /\ c = "counted"                                      \* counted itself
  /\ LET refuse == IF Continue THEN (Seal /\ sealed) ELSE closed  \* MUTATION no_seal / plain spawn
     IN /\ IF refuse
             THEN c' = "refused" /\ count' = count - 1  \* backs out
             ELSE c' = "held" /\ UNCHANGED count        \* admitted: holds its permit
        /\ liveRefused' = (liveRefused \/ (refuse /\ who = "req" /\ req = "held"))  \* recorded if the request still runs
        /\ idleAdmit' = (idleAdmit \/ (~refuse /\ z))  \* recorded if admitted from zero after closing
  /\ UNCHANGED <<closed, sealed, drain, fresh, z>>       \* the rest stays

\* A continuation ends and lets its permit go.
ContEnd(c) ==
  /\ c = "held"                                         \* holding a permit
  /\ c' = "gone" /\ count' = count - 1                  \* let go
  /\ UNCHANGED <<closed, sealed, drain, fresh, rz, sz, idleAdmit, liveRefused>>  \* the rest stays

RcCount == ContCount(rc, rz, "req") /\ UNCHANGED <<sc, sz>>        \* the request's continuation counts
RcDecide == ContDecide(rc, rz, "req") /\ UNCHANGED <<sc, sz, req>>  \* and decides
RcEnd == ContEnd(rc) /\ UNCHANGED <<sc, req>>                      \* and ends
ScCount == ContCount(sc, sz, "stray") /\ UNCHANGED <<rc, rz>>      \* the stray counts
ScDecide == ContDecide(sc, sz, "stray") /\ UNCHANGED <<rc, rz, req>>  \* and decides
ScEnd == ContEnd(sc) /\ UNCHANGED <<rc, req>>                      \* and ends

\* A newcomer counts itself (Gate::admit's first step), before or after closing.
FreshCount(n) ==
  /\ fresh[n] = "none"                                  \* not trying yet
  /\ fresh' = [fresh EXCEPT ![n] = "counted"]           \* counted
  /\ count' = count + 1                                 \* the gate's count goes up
  /\ UNCHANGED <<closed, sealed, drain, req, rc, sc, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* It reads `closed`: refused and backing out when the gate is closed, admitted otherwise.
FreshDecide(n) ==
  /\ fresh[n] = "counted"                               \* counted itself
  /\ IF Refuse /\ closed                                \* MUTATION no_refuse: closed never refuses
       THEN fresh' = [fresh EXCEPT ![n] = "refused"] /\ count' = count - 1  \* backs out, for good
       ELSE fresh' = [fresh EXCEPT ![n] = "held"] /\ UNCHANGED count        \* admitted: holds its permit
  /\ UNCHANGED <<closed, sealed, drain, req, rc, sc, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* A newcomer's work ends; it lets its permit go and may come back as the next request.
FreshEnd(n) ==
  /\ fresh[n] = "held"                                  \* holding its permit
  /\ fresh' = [fresh EXCEPT ![n] = "none"]              \* let go, free to try again
  /\ count' = count - 1                                 \* the gate's count goes down
  /\ UNCHANGED <<closed, sealed, drain, req, rc, sc, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The closer closes the gate.
Close ==
  /\ ~closed                                            \* once
  /\ closed' = TRUE                                     \* no plain admission from now on
  /\ UNCHANGED <<count, sealed, drain, req, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The drain sees zero after closing. With Seal it seals and looks again; without, it says drained now.
DrainSeesZero ==
  /\ closed /\ drain = "wait" /\ count = 0              \* closed and nothing counted
  /\ IF Seal
       THEN sealed' = TRUE /\ drain' = "look"           \* sealed, one more look
       ELSE drain' = "done" /\ UNCHANGED sealed         \* MUTATION no_seal: drained at once
  /\ UNCHANGED <<count, closed, req, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The drain's last look: zero says drained; anything counted sends it back to waiting.
DrainLook ==
  /\ drain = "look"                                     \* sealed
  /\ drain' = IF count = 0 THEN "done" ELSE "wait"      \* drained, or wait for zero again
  /\ UNCHANGED <<count, closed, sealed, req, rc, sc, fresh, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

Next ==
  \/ ReqCount \/ ReqDecide \/ ReqEnd
  \/ RcCount \/ RcDecide \/ RcEnd
  \/ ScCount \/ ScDecide \/ ScEnd
  \/ \E n \in Newcomers : FreshCount(n) \/ FreshDecide(n) \/ FreshEnd(n)
  \/ Close \/ DrainSeesZero \/ DrainLook

\* Fairness: whatever has started goes on. The closer closes, an admission
\* that counted itself decides, admitted work ends, and the drain takes its
\* steps once they stay possible. Nobody is forced to start new work.
Fairness ==
  /\ WF_vars(Close)                                     \* shutdown does begin
  /\ WF_vars(ReqDecide) /\ WF_vars(ReqEnd)              \* the request decides and ends
  /\ WF_vars(RcDecide) /\ WF_vars(RcEnd)                \* its continuation decides and ends
  /\ WF_vars(ScDecide) /\ WF_vars(ScEnd)                \* the stray decides and ends
  /\ \A n \in Newcomers : WF_vars(FreshDecide(n)) /\ WF_vars(FreshEnd(n))  \* newcomers decide and end
  /\ WF_vars(DrainSeesZero) /\ WF_vars(DrainLook)       \* the drain looks once it can

Spec == Init /\ [][Next]_vars /\ Fairness               \* every fair behaviour

TypeOK ==
  /\ count \in 0..5
  /\ closed \in BOOLEAN /\ sealed \in BOOLEAN
  /\ drain \in {"wait", "look", "done"}
  /\ req \in {"out", "counted", "held", "gone"}
  /\ rc \in {"none", "counted", "held", "refused", "gone"}
  /\ sc \in {"none", "counted", "held", "refused", "gone"}
  /\ fresh \in [Newcomers -> {"none", "counted", "held", "refused"}]
  /\ rz \in BOOLEAN /\ sz \in BOOLEAN
  /\ idleAdmit \in BOOLEAN /\ liveRefused \in BOOLEAN

\* Once the drain said drained, no request, continuation or newcomer holds a permit.
INV_NothingAfterDrain ==
  drain = "done" => (req # "held" /\ rc # "held" /\ sc # "held" /\ \A n \in Newcomers : fresh[n] # "held")
\* A request that holds its permit is never refused the continuation it asks for.
INV_InFlightContinues == ~liveRefused
\* After closing, no continuation is counted in while nothing was in flight.
INV_NoContinuationWhenIdle == ~idleAdmit
\* When every piece of work finishes on its own, the drain says drained.
LIVE_DrainEnds == <>(drain = "done")
====
