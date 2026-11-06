---- MODULE NodeAcpOps ----
\* The story: node access control (NAC) decides who may do what on a node.
\* Once set up it is on or temporarily off; while it is off everyone may do
\* everything, and its relationships wait, kept, for it to come back. A
\* client asks the database to turn NAC off, to turn it back on, or to give
\* someone a relation on the node; or the node restarts. The database does
\* each once, in one place, for every way in (HTTP, the CLI, FFI, the
\* embedded node).
\*
\* Before it changes anything the database checks, in Go's order, and stops
\* at the first that fails:
\*   disable    configured, already (NAC is not off yet), nac (the caller's
\*              live permission);
\*   re-enable  configured, stored (the caller's permission in the stored
\*              relationships, since the live check lets everyone through
\*              while NAC is off), already (NAC is not on yet);
\*   add        configured, nac, acp (NAC is on), identity (someone acts),
\*              relation (one the node's policy has), target (a DID or *),
\*              authority (the caller is an admin, or a delegate the Lean
\*              model NodeAcpDelegation admits for this relation and target;
\*              a delete runs the same checks).
\* A restart, with or without NAC asked for at start, leaves a set-up NAC as
\* it was: a disabled NAC stays off.
\*
\* Who acts: one request, or one restart, moving through its checks. Its
\* kind and every check's answer are chosen up front, so the model explores
\* every combination.
\*
\* What must never happen: NAC turned off by a caller without the live
\* permission; NAC turned back on by a caller without the stored permission;
\* a relation given while NAC is off, without an identity, for a relation or
\* target the policy does not have, by a caller without the permission, or
\* by a caller with no authority over that relation and target; a
\* status change on a node where NAC was never set up; a restart that turns a
\* disabled NAC back on; a refusal reported with another check's error than
\* Go's.
\*
\* Code it models (crates/db/src/access/nac_ops/mod.rs):
\*   DB::disable_nac_as                 the disable checks, then NacManager::disable
\*   DB::re_enable_nac_as               the re-enable checks, then NacManager::re_enable
\*   DB::check_stored_node_access       the stored check (NacManager::check_permission_persisted)
\*   DB::add_nac_actor_relationship_as  the add checks (DB::nac_relationship), then the grant
\*   NodeACP::authorize_change          the authority check (crates/acp/src/nac/node_acp/operations.rs)
\*   NacManager::initialize             the restart (crates/db/src/nac/mod.rs)
EXTENDS Naturals, Sequences   \* Seq and indexing, for the checks in Go's order

CONSTANTS
  Enforced,        \* which checks the database really runs
  LiveReEnable,    \* re-enable checks the live permission instead of the stored one
  StartReEnables,  \* a restart turns a disabled NAC back on, as before
  AlreadyFirst     \* re-enable checks "already on" before the caller

\* Only the named checks can be enforced.
ASSUME Enforced \subseteq {"configured", "already", "nac", "stored", "acp",
                           "identity", "relation", "target", "authority"}
\* The two switches are yes or no.
ASSUME LiveReEnable \in BOOLEAN /\ StartReEnables \in BOOLEAN /\ AlreadyFirst \in BOOLEAN

\* Go's checks for each request, in Go's order.
GoOrder(o) ==
  CASE o = "disable"  -> <<"configured", "already", "nac">>
    [] o = "reenable" -> <<"configured", "stored", "already">>
    [] o = "add"      -> <<"configured", "nac", "acp", "identity", "relation", "target",
                           "authority">>
    [] o = "restart"  -> <<>>

\* The order the database runs a request's checks in: Go's, or for re-enable
\* with "already on" moved ahead of the caller's check when the RED switch says.
CheckOrder(o) ==
  IF AlreadyFirst /\ o = "reenable"
    THEN <<"configured", "already", "stored">>
    ELSE GoOrder(o)

VARIABLES
  op,           \* the request: "disable", "reenable", "add", or a "restart"
  status,       \* NAC before it: "none" (never set up), "on" or "off"
  permitted,    \* the caller holds the permission the request needs
  stored,       \* the stored relationships give the caller that permission
  hasIdentity,  \* someone acts in the request
  relationOk,   \* the relation is one the node's policy has
  targetOk,     \* the target is a DID or *
  authorityOk,  \* the caller is an admin, or a delegate admitted for this change
  step,         \* "check", then "apply", then "done"
  reason,       \* the check the request was refused at, or "none"
  after,        \* NAC after the request
  granted       \* the relation was given

\* Every variable, for saying a step leaves them all as they are.
vars == <<op, status, permitted, stored, hasIdentity, relationOk, targetOk,
          authorityOk, step, reason, after, granted>>

\* The live NAC check: everyone passes while NAC is not on.
Live == status # "on" \/ permitted

\* Whether check `c` passes for this request.
Passes(c) ==
  CASE c = "configured" -> status # "none"
    [] c = "already"    -> IF op = "disable" THEN status # "off" ELSE status # "on"
    [] c = "nac"        -> Live
    \* the stored check, or the live one when the RED switch plants that bug
    [] c = "stored"     -> IF LiveReEnable THEN Live ELSE stored
    [] c = "acp"        -> status = "on"
    [] c = "identity"   -> hasIdentity
    [] c = "relation"   -> relationOk
    [] c = "target"     -> targetOk
    [] c = "authority"  -> authorityOk

\* The positions in `order` of the checks that are enforced and fail.
Failing(order) == {i \in 1..Len(order) : order[i] \in Enforced /\ ~Passes(order[i])}

\* The first enforced check that fails in `order`; "none" if all pass.
First(order) ==
  IF Failing(order) = {} THEN "none"
  \* the failing position no other failing position comes before
  ELSE order[CHOOSE i \in Failing(order) : \A j \in Failing(order) : i <= j]

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ op \in {"disable", "reenable", "add", "restart"}
  /\ status \in {"none", "on", "off"}
  /\ permitted \in BOOLEAN /\ stored \in BOOLEAN /\ hasIdentity \in BOOLEAN
  /\ relationOk \in BOOLEAN /\ targetOk \in BOOLEAN /\ authorityOk \in BOOLEAN
  /\ step \in {"check", "apply", "done"}
  /\ reason \in {"none", "configured", "already", "nac", "stored", "acp",
                 "identity", "relation", "target", "authority"}
  /\ after \in {"none", "on", "off"}
  /\ granted \in BOOLEAN

\* Any request against any node can arrive; nothing has changed yet.
Init ==
  /\ op \in {"disable", "reenable", "add", "restart"}
  /\ status \in {"none", "on", "off"}
  /\ permitted \in BOOLEAN /\ stored \in BOOLEAN /\ hasIdentity \in BOOLEAN
  /\ relationOk \in BOOLEAN /\ targetOk \in BOOLEAN /\ authorityOk \in BOOLEAN
  /\ step = "check"
  /\ reason = "none"
  /\ after = status
  /\ granted = FALSE

\* The checks run first. The first enforced check that fails ends the
\* request with its error; otherwise the request goes on, even if a check
\* left out would have failed (the bug a RED config plants).
Check ==
  /\ step = "check"
  /\ IF First(CheckOrder(op)) # "none"
       THEN /\ reason' = First(CheckOrder(op)) /\ step' = "done"
       ELSE /\ reason' = reason /\ step' = "apply"
  /\ UNCHANGED <<op, status, permitted, stored, hasIdentity, relationOk,
                 targetOk, authorityOk, after, granted>>

\* The change. Disable turns NAC off and re-enable turns it on, even on a
\* node where NAC was never set up when the check for that is left out. Add
\* gives the relation. A restart leaves a set-up NAC as it was, unless the
\* RED switch makes it turn a disabled NAC back on.
Apply ==
  /\ step = "apply"
  /\ step' = "done"
  /\ CASE op = "disable"  -> after' = "off" /\ UNCHANGED granted
       [] op = "reenable" -> after' = "on" /\ UNCHANGED granted
       [] op = "add"      -> granted' = TRUE /\ UNCHANGED after
       [] op = "restart"  -> /\ after' = IF StartReEnables /\ status = "off"
                                            THEN "on" ELSE status
                             /\ UNCHANGED granted
  /\ UNCHANGED <<op, status, permitted, stored, hasIdentity, relationOk,
                 targetOk, authorityOk, reason>>

\* Each step of a request is one of the two.
Next == Check \/ Apply

\* A finished request stays finished.
Done == step = "done" /\ UNCHANGED vars

\* A request starts in Init and then only checks, applies or stays finished.
Spec == Init /\ [][Next \/ Done]_vars

\* NAC is turned off only by a caller with the live permission.
INV_NoDisableWithoutPermission ==
  (op = "disable" /\ status = "on" /\ after = "off") => permitted

\* NAC is turned back on only by a caller with the stored permission.
INV_NoReEnableWithoutStoredPermission ==
  (op = "reenable" /\ status = "off" /\ after = "on") => stored

\* A relation is given only while NAC is on, by a permitted caller with an
\* identity and authority over it, for a relation and a target the policy has.
INV_NoGrantUnlessAllChecksPass ==
  granted => (status = "on" /\ permitted /\ hasIdentity /\ relationOk /\ targetOk
              /\ authorityOk)

\* A node where NAC was never set up keeps it so.
INV_NoChangeUnlessConfigured == status = "none" => after = "none"

\* A restart leaves NAC as it was.
INV_RestartKeepsStatus == op = "restart" => after = status

\* A disable or re-enable that was not refused changed the status: one that
\* would change nothing is refused with Go's "already" error.
INV_NoSilentNoOp ==
  (step = "done" /\ reason = "none" /\ op \in {"disable", "reenable"}) => after # status

\* A refused request reports the first failing check in Go's order.
INV_GoOrder == (step = "done" /\ reason # "none") => reason = First(GoOrder(op))
====
