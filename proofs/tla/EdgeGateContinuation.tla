---- MODULE EdgeGateContinuation ----
\* The story, in plain words.
\*
\* The network edge admits work through a gate (crates/edge/src/gate.rs). A
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
\*   * a late spawner, which tries a plain admission after closing: it counts
\*     itself first, then sees the gate closed and backs out (Gate::admit);
\*   * the closer, which closes the gate once;
\*   * the drain, which waits for zero, seals the gate, looks again, and only
\*     then says the gate is drained.
\*
\* What must never happen:
\*   1. once the drain said the gate is drained, a permit is still held -
\*      INV_NothingAfterDrain;
\*   2. a request in flight asks for a continuation and is refused -
\*      INV_InFlightContinues;
\*   3. after closing, a continuation is admitted when nothing was in flight -
\*      INV_NoContinuationWhenIdle.
\*
\* The Rust code each guard models:
\*   Seal      Gate::drain sets `sealed` before its last look at the count, and
\*             Gate::admit_continuation reads `sealed` after it counted itself
\*             and backs out when it is set;
\*   Cas       Gate::admit_continuation counts itself with a compare-and-swap
\*             that only succeeds on a count above zero;
\*   Continue  net::rt::ClientExecutor spawns a connection driver through
\*             Edge::spawn_continuation, not through a plain spawn.

EXTENDS Naturals

CONSTANTS
  Seal,      \* TRUE: the drain seals before its last look, and a continuation checks the seal
  Cas,       \* TRUE: a continuation counts itself only while the count is above zero
  Continue   \* TRUE: a continuation is admitted through the continuation path

VARIABLES
  count,     \* the gate's count of admitted work, transient counts included
  closed,    \* the gate is closed
  sealed,    \* the drain sealed the gate
  drain,     \* the drain: "wait" (for zero), "look" (sealed, looking again), "done" (said drained)
  req,       \* the request: "out", "counted" (admit counted, closed not read yet), "held", "gone"
  rc,        \* the request's continuation: "none", "counted", "held", "refused", "gone"
  sc,        \* the stray continuation: "none", "counted", "held", "refused", "gone"
  late,      \* the late spawner: "none", "counted", "gone"
  rz,        \* (ghost) the request's continuation counted itself in from zero after closing
  sz,        \* (ghost) the stray counted itself in from zero after closing
  idleAdmit, \* (record) a continuation was admitted after closing with nothing in flight
  liveRefused \* (record) the request's continuation was refused while the request held its permit

vars == <<count, closed, sealed, drain, req, rc, sc, late, rz, sz, idleAdmit, liveRefused>>  \* every variable

Init ==                          \* at the start:
  /\ count = 0                   \* nothing admitted
  /\ closed = FALSE              \* the gate admits work
  /\ sealed = FALSE              \* not sealed
  /\ drain = "wait"              \* the drain has not said drained
  /\ req = "out"                 \* the request not admitted yet
  /\ rc = "none"                 \* no continuation asked for
  /\ sc = "none"                 \* the stray not asked for
  /\ late = "none"               \* the late spawner not tried
  /\ rz = FALSE /\ sz = FALSE    \* no continuation counted yet
  /\ idleAdmit = FALSE           \* nothing went wrong
  /\ liveRefused = FALSE         \* nothing went wrong

\* A plain admission, first step: it counts itself (Gate::admit).
ReqCount ==
  /\ req = "out"                                        \* not admitted yet
  /\ req' = "counted" /\ count' = count + 1             \* counted
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* A plain admission, second step: it reads `closed`, and backs out if it is set.
ReqDecide ==
  /\ req = "counted"                                    \* counted itself
  /\ IF closed
       THEN req' = "gone" /\ count' = count - 1         \* refused: backs out
       ELSE req' = "held" /\ UNCHANGED count            \* admitted: holds its permit
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The request ends and lets its permit go.
ReqEnd ==
  /\ req = "held"                                       \* holding its permit
  /\ req' = "gone" /\ count' = count - 1                \* let go
  /\ UNCHANGED <<closed, sealed, drain, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

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
  /\ UNCHANGED <<closed, sealed, drain, req, late, idleAdmit, liveRefused>>  \* the rest stays

\* A continuation's second step: it reads the seal (continuation path) or `closed` (plain spawn).
ContDecide(c, z, who) ==
  /\ c = "counted"                                      \* counted itself
  /\ LET refuse == IF Continue THEN (Seal /\ sealed) ELSE closed  \* MUTATION no_seal / plain spawn
     IN /\ IF refuse
             THEN c' = "refused" /\ count' = count - 1  \* backs out
             ELSE c' = "held" /\ UNCHANGED count        \* admitted: holds its permit
        /\ liveRefused' = (liveRefused \/ (refuse /\ who = "req" /\ req = "held"))  \* recorded if the request still runs
        /\ idleAdmit' = (idleAdmit \/ (~refuse /\ z))  \* recorded if admitted from zero after closing
  /\ UNCHANGED <<closed, sealed, drain, late, z>>       \* the rest stays

\* A continuation ends and lets its permit go.
ContEnd(c) ==
  /\ c = "held"                                         \* holding a permit
  /\ c' = "gone" /\ count' = count - 1                  \* let go
  /\ UNCHANGED <<closed, sealed, drain, late, rz, sz, idleAdmit, liveRefused>>  \* the rest stays

RcCount == ContCount(rc, rz, "req") /\ UNCHANGED <<sc, sz>>        \* the request's continuation counts
RcDecide == ContDecide(rc, rz, "req") /\ UNCHANGED <<sc, sz, req>>  \* and decides
RcEnd == ContEnd(rc) /\ UNCHANGED <<sc, req>>                      \* and ends
ScCount == ContCount(sc, sz, "stray") /\ UNCHANGED <<rc, rz>>      \* the stray counts
ScDecide == ContDecide(sc, sz, "stray") /\ UNCHANGED <<rc, rz, req>>  \* and decides
ScEnd == ContEnd(sc) /\ UNCHANGED <<rc, req>>                      \* and ends

\* The late spawner counts itself after closing (Gate::admit's first step).
LateCount ==
  /\ late = "none" /\ closed                            \* tries once, after closing
  /\ late' = "counted" /\ count' = count + 1            \* counted
  /\ UNCHANGED <<closed, sealed, drain, req, rc, sc, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* It reads `closed` and backs out.
LateBack ==
  /\ late = "counted"                                   \* counted itself
  /\ late' = "gone" /\ count' = count - 1               \* backs out
  /\ UNCHANGED <<closed, sealed, drain, req, rc, sc, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The closer closes the gate.
Close ==
  /\ ~closed                                            \* once
  /\ closed' = TRUE                                     \* no plain admission from now on
  /\ UNCHANGED <<count, sealed, drain, req, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The drain sees zero after closing. With Seal it seals and looks again; without, it says drained now.
DrainSeesZero ==
  /\ closed /\ drain = "wait" /\ count = 0              \* closed and nothing counted
  /\ IF Seal
       THEN sealed' = TRUE /\ drain' = "look"           \* sealed, one more look
       ELSE drain' = "done" /\ UNCHANGED sealed         \* MUTATION no_seal: drained at once
  /\ UNCHANGED <<count, closed, req, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

\* The drain's last look: zero says drained; anything counted sends it back to waiting.
DrainLook ==
  /\ drain = "look"                                     \* sealed
  /\ drain' = IF count = 0 THEN "done" ELSE "wait"      \* drained, or wait for zero again
  /\ UNCHANGED <<count, closed, sealed, req, rc, sc, late, idleAdmit, liveRefused, rz, sz>>  \* the rest stays

Next ==
  \/ ReqCount \/ ReqDecide \/ ReqEnd
  \/ RcCount \/ RcDecide \/ RcEnd
  \/ ScCount \/ ScDecide \/ ScEnd
  \/ LateCount \/ LateBack
  \/ Close \/ DrainSeesZero \/ DrainLook

Spec == Init /\ [][Next]_vars                           \* every behaviour

TypeOK ==
  /\ count \in 0..4
  /\ closed \in BOOLEAN /\ sealed \in BOOLEAN
  /\ drain \in {"wait", "look", "done"}
  /\ req \in {"out", "counted", "held", "gone"}
  /\ rc \in {"none", "counted", "held", "refused", "gone"}
  /\ sc \in {"none", "counted", "held", "refused", "gone"}
  /\ late \in {"none", "counted", "gone"}
  /\ rz \in BOOLEAN /\ sz \in BOOLEAN
  /\ idleAdmit \in BOOLEAN /\ liveRefused \in BOOLEAN

\* Once the drain said drained, no request or continuation holds a permit.
INV_NothingAfterDrain == drain = "done" => (req # "held" /\ rc # "held" /\ sc # "held")
\* A request that holds its permit is never refused the continuation it asks for.
INV_InFlightContinues == ~liveRefused
\* After closing, no continuation is counted in while nothing was in flight.
INV_NoContinuationWhenIdle == ~idleAdmit
====
