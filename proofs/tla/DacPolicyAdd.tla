---- MODULE DacPolicyAdd ----
\* The story: a client asks the database to add a DAC policy, the rules that
\* say who may read, update and delete the documents of a collection. The
\* database does it once, in one place, for every way in (HTTP, the CLI, FFI,
\* the embedded node).
\*
\* The policy lives in one of two places. With local ACP the node's own store
\* keeps it. With a remote ACP (Vera, a chain) the remote ACP creates it and
\* gives it its ID; the node then keeps a copy in its own store, because every
\* collection and document that names the policy is checked against that copy.
\*
\* Before anything is stored the database checks, in Go's order, and stops at
\* the first that fails:
\*   1. nac      the caller may add policies on this node;
\*   2. acp      the node runs document ACP at all;
\*   3. creator  someone acts in the request: a policy has a creator;
\*   4. data     the policy text is not empty;
\*   5. valid    the policy parses and is one documents can use.
\* With a remote ACP one more guard follows the remote create:
\*   6. mirror   the node keeps a copy of what the remote ACP created.
\*
\* Who acts: the caller's request moving through the checks, then the store.
\* Every check's answer, whether the ACP is remote, and whether keeping the
\* copy fails are chosen up front, so the model explores every combination.
\*
\* What must never happen: a policy stored or created for a caller without
\* permission, without a creator (Vera once got an empty one), or that is
\* empty or invalid; a policy created on the remote ACP that is invalid; an
\* ID handed back that the node cannot find in its own store; a refusal
\* reported with a later check's error than Go's.
\*
\* Code it models (crates/db/src/access/dac/mod.rs):
\*   DB::add_dac_policy_as   the whole sequence, for every edge
\*   DB::add_policy          checks 1 to 5 in Go's order, then the store
\*   parse, build            check 5: the policy's shape and expressions
\*   DacPolicies::keep       the local store, and guard 6 (policies.rs)
\*   RemotePolicies::add_policy  the remote create (crates/acp/src/policy_lookup.rs, Vera's impl in crates/vera/src/cosmos/dac.rs)
EXTENDS Naturals, Sequences   \* Seq and indexing, for the checks in Go's order

CONSTANTS
  Enforced,     \* which guards the database really runs (a subset of the six)
  CreatorFirst  \* the creator is checked before ACP, as the HTTP handler did

\* Only the six named guards can be enforced.
ASSUME Enforced \subseteq {"nac", "acp", "creator", "data", "valid", "mirror"}
\* The order switch is a yes or a no.
ASSUME CreatorFirst \in BOOLEAN

\* The checks before the store, in Go's order.
GoOrder == <<"nac", "acp", "creator", "data", "valid">>

\* The order the database runs checks 1 to 5 in: Go's, or with the creator
\* moved ahead of ACP.
CheckOrder ==
  IF CreatorFirst
    THEN <<"nac", "creator", "acp", "data", "valid">>
    ELSE GoOrder

VARIABLES
  permitted,      \* the caller holds the add-policy permission
  acpOn,          \* the node runs document ACP
  hasCreator,     \* an identity acts in the request
  hasData,        \* the policy text is not empty
  valid,          \* the policy parses and documents can use it
  remote,         \* the ACP is remote (Vera)
  keepFails,      \* keeping the local copy fails
  step,           \* "check", then "store", then "done"
  reason,         \* the check the request was refused at, or "none"
  remoteCreated,  \* the remote ACP holds the policy
  localCopy,      \* the node's own store holds the policy
  returned        \* the caller got the policy's ID back

\* Every variable, for saying a step leaves them all as they are.
vars == <<permitted, acpOn, hasCreator, hasData, valid, remote, keepFails,
          step, reason, remoteCreated, localCopy, returned>>

\* Whether check `c` passes for this request.
Passes(c) ==
  CASE c = "nac"     -> permitted
    [] c = "acp"     -> acpOn
    [] c = "creator" -> hasCreator
    [] c = "data"    -> hasData
    [] c = "valid"   -> valid

\* The positions in `order` of the checks that are enforced and fail.
Failing(order) == {i \in 1..Len(order) : order[i] \in Enforced /\ ~Passes(order[i])}

\* The first enforced check that fails, in `order`; "none" if all pass.
First(order) ==
  IF Failing(order) = {} THEN "none"
  \* the failing position no other failing position comes before
  ELSE order[CHOOSE i \in Failing(order) : \A j \in Failing(order) : i <= j]

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ permitted \in BOOLEAN
  /\ acpOn \in BOOLEAN
  /\ hasCreator \in BOOLEAN
  /\ hasData \in BOOLEAN
  /\ valid \in BOOLEAN
  /\ remote \in BOOLEAN
  /\ keepFails \in BOOLEAN
  /\ step \in {"check", "store", "done"}
  /\ reason \in {"none", "nac", "acp", "creator", "data", "valid"}
  /\ remoteCreated \in BOOLEAN
  /\ localCopy \in BOOLEAN
  /\ returned \in BOOLEAN

\* Any request against any node can arrive; nothing is stored yet.
Init ==
  /\ permitted \in BOOLEAN
  /\ acpOn \in BOOLEAN
  /\ hasCreator \in BOOLEAN
  /\ hasData \in BOOLEAN
  /\ valid \in BOOLEAN
  /\ remote \in BOOLEAN
  /\ keepFails \in BOOLEAN
  /\ step = "check"
  /\ reason = "none"
  /\ remoteCreated = FALSE
  /\ localCopy = FALSE
  /\ returned = FALSE

\* The checks run before anything is stored. The first enforced check that
\* fails ends the request with its error; otherwise the request goes on to the
\* store, even if a check left out would have failed (the bug a RED plants).
Check ==
  /\ step = "check"
  /\ IF First(CheckOrder) # "none"
       THEN /\ reason' = First(CheckOrder) /\ step' = "done"
       ELSE /\ reason' = reason /\ step' = "store"
  /\ UNCHANGED <<permitted, acpOn, hasCreator, hasData, valid, remote,
                 keepFails, remoteCreated, localCopy, returned>>

\* The store. Without ACP, let through, there is nowhere to store, and the
\* request fails. With local ACP the node's store keeps the policy and the ID
\* goes back unless keeping it fails. With a remote ACP the remote creates the
\* policy first; then the node keeps its copy (guard 6) and hands the ID back
\* only once the copy is kept. Without guard 6 the ID goes back with no copy.
Store ==
  /\ step = "store"
  /\ step' = "done"
  /\ IF ~acpOn
       THEN UNCHANGED <<remoteCreated, localCopy, returned>>
     ELSE IF ~remote
       THEN /\ localCopy' = ~keepFails
            /\ returned' = ~keepFails
            /\ UNCHANGED remoteCreated
     ELSE IF "mirror" \in Enforced
       THEN /\ remoteCreated' = TRUE
            /\ localCopy' = ~keepFails
            /\ returned' = ~keepFails
     ELSE /\ remoteCreated' = TRUE
          /\ returned' = TRUE
          /\ UNCHANGED localCopy
  /\ UNCHANGED <<permitted, acpOn, hasCreator, hasData, valid, remote,
                 keepFails, reason>>

\* Each step of a request is one of the two.
Next == Check \/ Store

\* A finished request stays finished.
Done == step = "done" /\ UNCHANGED vars

\* A request starts in Init and then only checks, stores or stays finished.
Spec == Init /\ [][Next \/ Done]_vars

\* Whether the policy exists anywhere.
Stored == remoteCreated \/ localCopy

\* No policy is stored or created for a caller without the permission.
INV_NoPolicyWithoutPermission == Stored => permitted

\* No policy is stored or created without a creator.
INV_NoPolicyWithoutCreator == Stored => hasCreator

\* No policy is stored or created that is empty or invalid; the remote ACP
\* never sees an invalid one.
INV_NoInvalidPolicy == Stored => (hasData /\ valid)

\* No policy exists on a node that runs without document ACP.
INV_NoPolicyWithoutAcp == Stored => acpOn

\* An ID handed back names a policy the node's own store holds, so the
\* collections and documents that name it can be checked.
INV_ReturnedIsKeptLocally == returned => localCopy

\* A request that was not refused and got no ID back failed only because
\* keeping the policy failed, which it reports: none fails silently.
INV_NoSilentFailure ==
  (step = "done" /\ reason = "none" /\ ~returned) => (acpOn /\ keepFails)

\* A refused request reports the first failing check in Go's order.
INV_GoOrder == (step = "done" /\ reason # "none") => reason = First(GoOrder)
====
