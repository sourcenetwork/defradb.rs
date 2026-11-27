------------------------------ MODULE ForeignPolicy ------------------------------
\* The story, in plain words.
\*
\* Two accounts each run a Vera deployment of their own: the owner's home A
\* and the guest's home B. A collection is shared under the owner's policy
\* P, which lives on A. The guest's node, on B, knows that A owns P: its
\* registry routes P to a light client of A. The registry can also forget
\* P again, and add it back.
\*
\* On A the owner may grant the guest write on a document and revoke it
\* again, any number of times. A may stop answering (its home is away) and
\* come back.
\*
\* B does not see A's state as it is now: its light client holds the newest
\* state of A it verified, and how long ago that was. A verified view is
\* used for at most Bound ticks (the light client's freshness bound, 30
\* seconds in the product); older than that, B has no answer. So for up to
\* Bound ticks B may act on a grant A has since revoked, or on a refusal A
\* has since lifted: that lag is the contract, and nothing beyond it.
\*
\* On B two things ask about the guest's write:
\*   * a change the guest signed arrives by replication and must be merged,
\*     parked (it waits for A's state to change) or left pending (asked
\*     again on the retry clock);
\*   * the guest edits the document on B itself, and the edit is committed,
\*     denied, or refused with "try again".
\* B's own deployment may hold a policy of the same ID as P, and say
\* anything at all about the guest under it.
\*
\* What must never happen:
\*   - while P is routed to A, B merges or commits the guest's write unless
\*     A granted it in a state B verified at most Bound ticks ago
\*     - INV_DecidedByHome;
\*   - while P is routed to A, B registers the document on its own
\*     deployment - INV_NoReceiverRegistration;
\*   - B parks the change or denies the edit unless A refused in a state B
\*     verified at most Bound ticks ago: a home B has not heard from for
\*     longer is "try again", never "no" - INV_AwayNeverDenies;
\*   - a refused change is rejected for good - INV_NeverQuarantined;
\*   - P is routed to A while B's own deployment holds a policy of P's ID:
\*     A would then decide B's own documents under it - INV_NoShadow;
\*   - once A grants the guest for good and answers, the parked change does
\*     not end merged - LIVE_ReleasedOnGrant.
\*
\* Add and CreateOwn are one step each here; ForeignPolicyCreate.tla shows
\* that the steps the code takes for them, interleaved with each other and
\* with a light client that sees the node's own deployment late, are that
\* one step.
\*
\* Rust owners: crates/engine/vera/src/foreign.rs ForeignPolicies (route,
\* route_to, remove, claimed, claim, begin_create, home_changed);
\* crates/engine/vera/src/dac/foreign_routes.rs
\* VeraDocumentACP::add_foreign_policy, unheld_here;
\* crates/engine/vera/src/dac.rs VeraDocumentACP::add_policy,
\* home, owned_here, check_doc_access, is_doc_registered, get_doc_owner;
\* crates/engine/vera/src/vera_rs/provider/foreign.rs ForeignProvider
\* (serving, serves_given, verify_access, query_object_owner, decision_state,
\* observe),
\* whose light client's FreshnessPolicy is the Bound;
\* crates/engine/vera/src/vera_rs/provider/reach.rs read_error;
\* crates/database/db/src/merge/acp_merge_handler.rs
\* AcpCompositeMergeHook::replicas_for, post_commit_action;
\* crates/database/db/src/merge/merge_handler/parked_acp.rs
\* ParkedRelease::policy_changed, policy_removed;
\* crates/database/db/src/merge/merge_handler/policy_redrives.rs
\* PolicyRedrives (the one pending re-drive `moved` stands for);
\* crates/database/db/src/merge/governance/deferred.rs
\* DeferredMerges::release_policy, forget_policy;
\* crates/database/db/src/write/autocommit/helpers.rs and
\* crates/database/db/src/write/create/gate.rs create_gate (the create refusal).
EXTENDS Naturals

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug
\* How many ticks a verified view of A is used for: the freshness bound.
CONSTANT Bound

VARIABLES
  routed,      \* TRUE while B's registry routes P to A
  ownHolds,    \* TRUE while B's own deployment holds a policy of P's ID
  grantA,      \* TRUE while A grants the guest write
  seen,        \* what A granted in the newest state of A that B verified
  age,         \* ticks since B verified that state; Bound + 1 means none usable
  grantB,      \* what B's own deployment says about the guest: anything
  away,        \* TRUE while A cannot be reached
  moved,       \* TRUE when B verified a new state of A it has not handed on yet
  change,      \* the replicated change: "pending", "parked", "merged" or "rejected"
  edit,        \* the guest's edit on B: "none", "committed", "denied" or "retry"
  registeredB  \* TRUE once B registered the document on its own deployment

vars == <<routed, ownHolds, grantA, seen, age, grantB, away, moved, change,
          edit, registeredB>>

\* At the start P is routed to A, B's own deployment holds no policy of its
\* ID, A answers, B has just verified A's state, and anything may hold about
\* the guest on either deployment.
Init ==
  /\ routed = TRUE                 \* the invite added P to the registry
  /\ ownHolds = FALSE              \* B's own deployment holds no policy P
  /\ grantA \in BOOLEAN            \* A may or may not grant the guest yet
  /\ seen = grantA                 \* B has verified A's state ...
  /\ age = 0                       \* ... just now
  /\ grantB \in BOOLEAN            \* B's own deployment may say anything
  /\ away = FALSE                  \* A answers at first
  /\ moved = FALSE                 \* nothing to hand on yet
  /\ change = "pending"            \* the change has not been decided
  /\ edit = "none"                 \* the guest has not edited on B yet
  /\ registeredB = FALSE           \* B registered nothing

\* Whether B's view of A may be used: verified at most Bound ticks ago. The
\* planted bug uses it however old it is.
Fresh == age <= Bound \/ Bug = "unboundedLag"

\* What A's light client on B answers: the verified view while it is
\* fresh, and no answer once it is not. The planted bug takes a stale view
\* as a refusal.
HomeDecision ==
  IF ~Fresh
    THEN IF Bug = "awayDenies" THEN "no" ELSE "away"
    ELSE IF seen THEN "yes" ELSE "no"

\* What B's own deployment answers: what it holds when it holds a policy of
\* P's ID (or, with the planted bug, always), and nothing it can decide
\* (try again) when it holds none.
OwnDecision ==
  IF ownHolds \/ Bug = "ownDecides"
    THEN IF grantB THEN "yes" ELSE "no"
    ELSE "away"

\* What B decides about the guest's write right now: "yes", "no" or
\* "away" (no answer). Routed, A's light client decides; the planted bug
\* lets B's own deployment decide a routed P.
Decision ==
  IF routed /\ Bug # "ownDecides" THEN HomeDecision ELSE OwnDecision

\* What a decision does to a change: it merges on "yes", is parked on "no"
\* (the planted bug rejects it for good), and goes back to the retry clock
\* on "away".
Decided ==
  CASE Decision = "yes" -> "merged"
    [] Decision = "no" -> IF Bug = "quarantine" THEN "rejected" ELSE "parked"
    [] OTHER -> "pending"

\* B tries a pending change, on arrival or on the replication layer's retry
\* clock.
Try ==
  /\ change = "pending"
  /\ change' = Decided
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, away, moved,
                 edit, registeredB>>

\* The guest edits on B: committed on "yes", denied on "no", and "try
\* again" on "away". The planted bug denies the edit when there is no
\* answer.
Edit ==
  /\ edit \in {"none", "retry"}
  /\ edit' = CASE Decision = "yes" -> "committed"
               [] Decision = "no" -> "denied"
               [] OTHER -> IF Bug = "editAwayDenies" THEN "denied" ELSE "retry"
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, away, moved,
                 change, registeredB>>

\* B registers the document on its own deployment, as it does for a
\* replica of a policy it decides itself. Routed, it never does; the planted
\* bug registers it anyway.
RegisterOnB ==
  /\ ~registeredB
  /\ ~routed \/ Bug = "registerForeign"
  /\ registeredB' = TRUE
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, away, moved,
                 change, edit>>

\* The owner grants the guest write on A. B does not see it yet.
Grant ==
  /\ ~grantA
  /\ grantA' = TRUE
  /\ UNCHANGED <<routed, ownHolds, seen, age, grantB, away, moved, change,
                 edit, registeredB>>

\* The owner revokes the guest's write on A. B does not see it yet.
Revoke ==
  /\ grantA
  /\ grantA' = FALSE
  /\ UNCHANGED <<routed, ownHolds, seen, age, grantB, away, moved, change,
                 edit, registeredB>>

\* B's light client verifies A's newest state; when what A grants differs
\* from the view before, the state root moved and the registry is to tell
\* the merge.
Refresh ==
  /\ routed
  /\ ~away
  /\ seen' = grantA
  /\ age' = 0
  /\ moved' = (moved \/ seen # grantA)
  /\ UNCHANGED <<routed, ownHolds, grantA, grantB, away, change, edit,
                 registeredB>>

\* Time passes: B's view of A ages, up to one past the bound.
Tick ==
  /\ age <= Bound
  /\ age' = age + 1
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, grantB, away, moved, change,
                 edit, registeredB>>

\* B's own deployment changes what it says about the guest.
ChangeB ==
  /\ grantB' = ~grantB
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, away, moved, change,
                 edit, registeredB>>

\* A stops answering.
Leave ==
  /\ ~away
  /\ away' = TRUE
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, moved, change,
                 edit, registeredB>>

\* A answers again.
Return ==
  /\ away
  /\ away' = FALSE
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, moved, change,
                 edit, registeredB>>

\* The registry hands on the new state: the merge asks again about what is
\* parked under P, once however many new states came in the meantime
\* (`moved` is one flag: PolicyRedrives folds notices into one run). A
\* parked change is decided again; with no answer it goes back to the
\* retry clock. The planted bug tells nothing, and has no sweep either.
HomeMoved ==
  /\ moved
  /\ moved' = FALSE
  /\ change' = IF change = "parked" /\ Bug # "noRelease" THEN Decided ELSE change
  /\ UNCHANGED <<routed, ownHolds, grantA, seen, age, grantB, away, edit,
                 registeredB>>

\* The registry forgets P: a parked change leaves the index for the retry
\* clock, and nothing is handed on any more.
Remove ==
  /\ routed
  /\ routed' = FALSE
  /\ moved' = FALSE
  /\ change' = IF change = "parked" THEN "pending" ELSE change
  /\ UNCHANGED <<ownHolds, grantA, seen, age, grantB, away, edit, registeredB>>

\* The registry routes P to A again, refused while B's own deployment holds
\* a policy of P's ID; the planted bug routes it anyway. A new light client
\* has verified nothing yet: until it has proven A's genesis and state, it
\* has no answer.
Add ==
  /\ ~routed
  /\ ~ownHolds \/ Bug = "shadowAdd"
  /\ routed' = TRUE
  /\ seen' = FALSE
  /\ age' = Bound + 1
  /\ moved' = TRUE
  /\ UNCHANGED <<ownHolds, grantA, grantB, away, change, edit, registeredB>>

\* B's own deployment creates or adopts a policy of P's ID, refused before
\* it is sent while P is routed (or being routed); the planted bug creates
\* it anyway.
CreateOwn ==
  /\ ~ownHolds
  /\ ~routed \/ Bug = "shadowCreate"
  /\ ownHolds' = TRUE
  /\ UNCHANGED <<routed, grantA, seen, age, grantB, away, moved, change, edit,
                 registeredB>>

\* One step of anyone.
Next ==
  \/ Try
  \/ Edit
  \/ RegisterOnB
  \/ Grant
  \/ Revoke
  \/ Refresh
  \/ Tick
  \/ ChangeB
  \/ Leave
  \/ Return
  \/ HomeMoved
  \/ Remove
  \/ Add
  \/ CreateOwn

\* The safety specification lets the registry forget and add P.
Spec == Init /\ [][Next]_vars

\* The liveness specification keeps P routed. B's light client verifies A
\* whenever A answers, the registry hands on every new state, and the retry
\* clock tries a pending change whenever B's view could let it in.
LiveNext == Next /\ routed' = TRUE
LiveSpec ==
  /\ Init
  /\ [][LiveNext]_vars
  /\ WF_vars(Refresh /\ routed' = TRUE)
  /\ WF_vars(HomeMoved /\ routed' = TRUE)
  /\ SF_vars(Try /\ Decision = "yes" /\ routed' = TRUE)

\* The variables hold values of the expected kinds.
TypeOK ==
  /\ routed \in BOOLEAN
  /\ ownHolds \in BOOLEAN
  /\ grantA \in BOOLEAN
  /\ seen \in BOOLEAN
  /\ age \in 0..(Bound + 1)
  /\ grantB \in BOOLEAN
  /\ away \in BOOLEAN
  /\ moved \in BOOLEAN
  /\ change \in {"pending", "parked", "merged", "rejected"}
  /\ edit \in {"none", "committed", "denied", "retry"}
  /\ registeredB \in BOOLEAN

\* While P is routed to A, B merges the change or commits the edit only on
\* a grant of A that B verified at most Bound ticks ago.
INV_DecidedByHome ==
  [][routed =>
       /\ (change # "merged" /\ change' = "merged") => (seen /\ age <= Bound)
       /\ (edit # "committed" /\ edit' = "committed") => (seen /\ age <= Bound)]_vars

\* While P is routed to A, B registers nothing on its own deployment.
INV_NoReceiverRegistration ==
  [][(routed /\ ~registeredB /\ registeredB') => FALSE]_vars

\* While P is routed to A, B parks the change or denies the edit only on a
\* refusal of A that B verified at most Bound ticks ago: once B has not
\* heard from A for longer, it never says no.
INV_AwayNeverDenies ==
  [][routed =>
       /\ (change # "parked" /\ change' = "parked") => (~seen /\ age <= Bound)
       /\ (edit # "denied" /\ edit' = "denied") => (~seen /\ age <= Bound)]_vars

\* A refused change is never rejected for good.
INV_NeverQuarantined == change # "rejected"

\* P is never routed to A while B's own deployment holds a policy of its ID.
INV_NoShadow == ~(routed /\ ownHolds)

\* Once A grants the guest for good and answers for good, the change ends
\* merged.
LIVE_ReleasedOnGrant == <>[](grantA /\ ~away) => <>(change = "merged")
=============================================================================
