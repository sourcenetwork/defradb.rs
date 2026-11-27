--------------------------- MODULE ForeignPolicyCreate ---------------------------
\* The story, in plain words.
\*
\* A node runs on its own Vera deployment. Two things on it can happen to
\* one policy ID P at the same time, each in several steps:
\*   * a create of P on the node's own deployment: it marks itself in
\*     flight, looks whether P is routed or being routed (and gives up if
\*     so), sends the create, which lands on the deployment, and returns
\*     once the node's own light client has seen P there. The handed-off
\*     work owns the in-flight mark, so a caller that stops waiting does
\*     not remove it;
\*   * an add of a route of P to another deployment's home: it claims P,
\*     looks whether a create of P is in flight (and gives up if so), asks
\*     the node's own light client whether the deployment holds P (and gives
\*     up if so), and routes P.
\* The light client sees the deployment late: P can be held there and not
\* seen yet. A route can also be removed.
\*
\* ForeignPolicy.tla takes the create and the add as one step each, and
\* checks that they keep INV_NoShadow. This model shows that the steps the
\* code takes in between let nothing slip through: that the two-step code
\* is that one step.
\*
\* What must never happen:
\*   - P is routed while the node's own deployment holds it: the home would
\*     decide the node's own documents under it - INV_NoShadow.
\*
\* Rust owners: crates/engine/vera/src/foreign/claims.rs Claims (claim,
\* claimed, any_claimed, create, creating); crates/engine/vera/src/foreign.rs
\* ForeignPolicies::claim, ForeignPolicies::begin_create,
\* ForeignPolicies::claimed; crates/engine/vera/src/dac/foreign_routes.rs
\* VeraDocumentACP::unheld_here; crates/engine/vera/src/dac.rs
\* VeraDocumentACP::add_policy, named_create; the provider contract of
\* crates/engine/vera/src/provider.rs VeraProvider::create_policy and
\* create_policy_pending, kept by
\* crates/engine/vera/src/vera_rs/provider/create_policy.rs
\* create_policy_with, pending_policy_create (its verified read of the
\* record it made).
EXTENDS Naturals

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

VARIABLES
  routed,    \* TRUE while P is routed to another home
  claimed,   \* TRUE while an add holds its claim of P
  creating,  \* TRUE while a create of P is marked in flight
  pending,   \* TRUE while a create of P remains in the worker journal
  ownHolds,  \* TRUE once the node's own deployment holds P
  ownSeen,   \* TRUE once the node's own light client has seen P held
  cpc,       \* where the create is: "idle", "marked", "sent" or "landed"
  apc,       \* where the add is: "idle", "claimed" or "checked"
  waiting    \* TRUE while the create's caller still waits

vars == <<routed, claimed, creating, pending, ownHolds, ownSeen, cpc, apc, waiting>>

\* At the start nothing is routed, held or in flight.
Init ==
  /\ routed = FALSE       \* no route of P
  /\ claimed = FALSE      \* no add of P
  /\ creating = FALSE     \* no create of P
  /\ pending = FALSE      \* no retain request
  /\ ownHolds = FALSE     \* the deployment holds no P
  /\ ownSeen = FALSE      \* and the light client has seen none
  /\ cpc = "idle"         \* the create has not started
  /\ apc = "idle"         \* nor has the add
  /\ waiting = TRUE       \* the create's caller waits

\* The create marks itself in flight, first of all.
CreateMark ==
  /\ cpc = "idle"
  /\ creating' = TRUE
  /\ cpc' = "marked"
  /\ UNCHANGED <<routed, claimed, pending, ownHolds, ownSeen, apc, waiting>>

\* Then it looks for a claim or a route of P: found, it gives up before
\* anything is sent; the planted bug sends it anyway.
CreateCheck ==
  /\ cpc = "marked"
  /\ IF (claimed \/ routed) /\ Bug # "noClaimCheck"
       THEN /\ creating' = FALSE
            /\ cpc' = "idle"
       ELSE /\ cpc' = "sent"
            /\ UNCHANGED creating
  /\ UNCHANGED <<routed, claimed, pending, ownHolds, ownSeen, apc, waiting>>

\* The deployment executes the create: it holds P now.
CreateLand ==
  /\ cpc = "sent"
  /\ ownHolds' = TRUE
  /\ cpc' = "landed"
  /\ UNCHANGED <<routed, claimed, creating, pending, ownSeen, apc, waiting>>

\* The deployment refuses the create: nothing is held, and it ends.
CreateFail ==
  /\ cpc = "sent"
  /\ creating' = FALSE
  /\ cpc' = "idle"
  /\ UNCHANGED <<routed, claimed, pending, ownHolds, ownSeen, apc, waiting>>

\* A stopped host leaves the request in its journal and loses the in-memory
\* mark. The next host checks the journal before taking a route.
CreateRetain ==
  /\ cpc = "sent"
  /\ creating' = FALSE
  /\ pending' = TRUE
  /\ cpc' = "idle"
  /\ waiting' = FALSE
  /\ UNCHANGED <<routed, claimed, ownHolds, ownSeen, apc>>

\* The caller stops waiting without stopping the handed-off work. The work
\* keeps the in-flight mark; the planted bug forgets it here.
CallerStops ==
  /\ waiting
  /\ waiting' = FALSE
  /\ IF Bug = "forgetsMark" THEN creating' = FALSE ELSE UNCHANGED creating
  /\ UNCHANGED <<routed, claimed, pending, ownHolds, ownSeen, cpc, apc>>

\* The create returns, and its mark goes, once the light client has seen
\* P held; the planted bug returns before that.
CreateReturn ==
  /\ cpc = "landed"
  /\ ownSeen \/ Bug = "earlyReturn"
  /\ creating' = FALSE
  /\ cpc' = "idle"
  /\ UNCHANGED <<routed, claimed, pending, ownHolds, ownSeen, apc, waiting>>

\* A retained request reaches its terminal outcome: the deployment holds P.
RetainedLand ==
  /\ pending
  /\ ownHolds' = TRUE
  /\ ownSeen' = TRUE
  /\ pending' = FALSE
  /\ UNCHANGED <<routed, claimed, creating, cpc, apc, waiting>>

\* A retained request reaches its terminal outcome: the deployment refuses it.
RetainedFail ==
  /\ pending
  /\ pending' = FALSE
  /\ UNCHANGED <<routed, claimed, creating, ownHolds, ownSeen, cpc, apc, waiting>>

\* The node's own light client verifies a newer state, which holds P.
OwnSeen ==
  /\ ownHolds
  /\ ~ownSeen
  /\ ownSeen' = TRUE
  /\ UNCHANGED <<routed, claimed, creating, pending, ownHolds, cpc, apc, waiting>>

\* An add of an unrouted P claims it, first of all.
AddClaim ==
  /\ apc = "idle"
  /\ ~routed
  /\ claimed' = TRUE
  /\ apc' = "claimed"
  /\ UNCHANGED <<routed, creating, pending, ownHolds, ownSeen, cpc, waiting>>

\* Then it looks for a create of P in flight or retained in the journal:
\* found, it gives up. One planted bug skips the in-memory check, another
\* skips the retained-request check, and the remaining bugs go on past both.
AddCheckCreating ==
  /\ apc = "claimed"
  /\ IF (creating /\ Bug # "noCreatingCheck") \/
        (pending /\ Bug # "noPendingCheck")
       THEN /\ claimed' = FALSE
            /\ apc' = "idle"
       ELSE /\ apc' = "checked"
            /\ UNCHANGED claimed
  /\ UNCHANGED <<routed, creating, pending, ownHolds, ownSeen, cpc, waiting>>

\* Then it asks the light client whether the deployment holds P: seen
\* held, it gives up; otherwise it routes P, and drops its claim after.
\* The planted bug routes P whatever the light client says.
AddRoute ==
  /\ apc = "checked"
  /\ IF ownSeen /\ Bug # "shadowAdd"
       THEN UNCHANGED routed
       ELSE routed' = TRUE
  /\ claimed' = FALSE
  /\ apc' = "idle"
  /\ UNCHANGED <<creating, pending, ownHolds, ownSeen, cpc, waiting>>

\* The route of P is removed.
Remove ==
  /\ routed
  /\ routed' = FALSE
  /\ UNCHANGED <<claimed, creating, pending, ownHolds, ownSeen, cpc, apc, waiting>>

\* One step of anyone.
Next ==
  \/ CreateMark
  \/ CreateCheck
  \/ CreateLand
  \/ CreateFail
  \/ CreateRetain
  \/ CallerStops
  \/ CreateReturn
  \/ RetainedLand
  \/ RetainedFail
  \/ OwnSeen
  \/ AddClaim
  \/ AddCheckCreating
  \/ AddRoute
  \/ Remove

\* Any interleaving of the steps, from the start.
Spec == Init /\ [][Next]_vars

\* The variables hold values of the expected kinds.
TypeOK ==
  /\ routed \in BOOLEAN
  /\ claimed \in BOOLEAN
  /\ creating \in BOOLEAN
  /\ pending \in BOOLEAN
  /\ ownHolds \in BOOLEAN
  /\ ownSeen \in BOOLEAN
  /\ cpc \in {"idle", "marked", "sent", "landed"}
  /\ apc \in {"idle", "claimed", "checked"}
  /\ waiting \in BOOLEAN

\* P is never routed while the node's own deployment holds it.
INV_NoShadow == ~(routed /\ ownHolds)
=============================================================================
