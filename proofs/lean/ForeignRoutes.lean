/-! Which deployment decides a policy, and what the node does with its answer.

The story: a node runs on its own Vera deployment, its home. Some policies
belong to other deployments: an account shares a space under its own
policy, and every other account's node must ask that account's home about
it. The node keeps a registry from such a policy to the home that owns it.
A policy can be added (an invite) and removed while the node runs.

When the node asks about a policy, the registry says where to ask: the
owning home for a policy it holds, the node's own deployment for every
other. The answer is "yes", "no" or "away" (the home could not be asked).
A merge or an edit goes through on "yes", waits (parked, or tried again)
on "no", and is tried again later on "away": an away home is never read
as a "no". Under a policy another home owns, the node writes nothing of its
own (no registration, no relationship change, no archive).

When a home reaches a new state, the node asks again about every change
parked under one of that home's policies, and only those. When a policy's
route is removed, the changes parked under it leave the index, and only
those.

A policy is routed only while the node's own deployment holds no policy of
its ID, and the node's own deployment creates no policy that is routed: a
home deciding a policy of the node's own would decide the node's own
documents under it. Both refusals keep that "no shadow" rule. Each is one
step here: the create is refused before anything is sent, and
`proofs/tla/ForeignPolicyCreate.tla` shows that the code's create and add,
interleaved step by step, are these steps.

What must never happen: a routed policy decided by the node's own
deployment; another policy decided by a home; an away home turned into a
refusal; a write of the node's own under a routed policy; a release of a
change parked under another policy; a routed policy the node's own
deployment holds.

Rust owners: crates/engine/vera/src/foreign.rs ForeignPolicies::route,
route_to, remove, claimed, home_changed;
crates/engine/vera/src/dac/foreign_routes.rs
VeraDocumentACP::add_foreign_policy, unheld_here;
crates/engine/vera/src/dac.rs VeraDocumentACP::add_policy, home,
owned_here; crates/engine/vera/src/foreign.rs ForeignPolicies::claim,
begin_create;
crates/engine/vera/src/vera_rs/provider/reach.rs read_error;
crates/database/db/src/merge/governance/deferred.rs
DeferredMerges::release_policy, forget_policy. -/
namespace ForeignRoutes

/-- A policy, named by a number. -/
abbrev Policy := Nat

/-- A home deployment, named by a number. -/
abbrev Home := Nat

/-- The registry: for each policy, the home that owns it, or nothing when
the node's own deployment decides it. -/
def Registry := Policy → Option Home

/-- The registry of a node that was given no foreign policy. -/
def Registry.empty : Registry := fun _ => none

/-- Route `p` to home `h`: `p` answers `h`, every other policy what it
answered before. -/
def Registry.add (r : Registry) (p : Policy) (h : Home) : Registry :=
  -- `p` now goes to `h`; any other `q` keeps its route.
  fun q => if q = p then some h else r q

/-- Forget `p`: the node's own deployment decides it again. -/
def Registry.remove (r : Registry) (p : Policy) : Registry :=
  -- `p` now goes nowhere else; any other `q` keeps its route.
  fun q => if q = p then none else r q

/-- What a deployment says about one question. -/
inductive Answer where
  /-- The deployment grants it. -/
  | yes
  /-- The deployment refuses it. -/
  | no
  /-- The deployment could not be asked. -/
  | away
  deriving DecidableEq, Repr

/-- What each deployment answers: the node's own for the node, and each
home's for that home. A home that cannot be reached answers `away`. -/
structure World where
  /-- The node's own deployment's answer about each policy. -/
  own : Policy → Answer
  /-- Each home's answer about each policy. -/
  home : Home → Policy → Answer

/-- The answer the node acts on for policy `p`: the owning home's when the
registry routes `p`, the node's own deployment's otherwise. -/
def decide (r : Registry) (w : World) (p : Policy) : Answer :=
  -- Ask where the registry says, and nowhere else.
  match r p with
  | some h => w.home h p
  | none => w.own p

/-- What the node does with a replicated change, or a local edit. -/
inductive Step where
  /-- It merges, or commits. -/
  | admit
  /-- It waits for the state to change (a change is parked; an edit is
  denied until the owner grants it). -/
  | wait
  /-- Nothing is known: it is tried again later. -/
  | retry
  deriving DecidableEq, Repr

/-- How an answer turns into a step: only a "yes" admits, a "no" waits, an
away deployment is tried again. -/
def step : Answer → Step
  -- A grant admits.
  | .yes => .admit
  -- A refusal waits for a grant.
  | .no => .wait
  -- No answer decides nothing.
  | .away => .retry

/-- Whether the node may write something of its own under `p`: only under
a policy no other home owns. -/
def ownedHere (r : Registry) (p : Policy) : Bool :=
  -- A routed policy is written only on its home.
  (r p).isNone

/-- A change parked under one object, named by its policy and object. -/
structure Parked where
  /-- The policy of the object the change waits under. -/
  policy : Policy
  /-- The object the change waits under. -/
  object : Nat

/-- The parked changes a new state of the home of `p` asks again about:
those under `p`, kept in order. -/
def releasePolicy (parked : List Parked) (p : Policy) : List Parked :=
  -- Keep exactly those whose policy is `p`.
  parked.filter (fun k => k.policy == p)

/-- A policy just added goes to the home it was added with. -/
theorem route_after_add (r : Registry) (p : Policy) (h : Home) :
    (r.add p h) p = some h := by
  -- The `if` takes its first branch.
  simp [Registry.add]

/-- Adding a policy changes no other policy's route. -/
theorem add_keeps_others (r : Registry) (p q : Policy) (h : Home) (hq : q ≠ p) :
    (r.add p h) q = r q := by
  -- The `if` takes its second branch, since `q` is not `p`.
  simp [Registry.add, hq]

/-- A policy just removed goes to the node's own deployment. -/
theorem route_after_remove (r : Registry) (p : Policy) :
    (r.remove p) p = none := by
  -- The `if` takes its first branch.
  simp [Registry.remove]

/-- Removing a policy changes no other policy's route. -/
theorem remove_keeps_others (r : Registry) (p q : Policy) (hq : q ≠ p) :
    (r.remove p) q = r q := by
  -- The `if` takes its second branch, since `q` is not `p`.
  simp [Registry.remove, hq]

/-- A node given no foreign policy decides everything on its own
deployment, exactly as before. -/
theorem empty_decides_on_own (w : World) (p : Policy) :
    decide Registry.empty w p = w.own p := by
  -- The empty registry routes nothing.
  simp [decide, Registry.empty]

/-- A routed policy is decided by its home alone: whatever the node's own
deployment says, the answer is the home's. -/
theorem routed_decided_by_home (r : Registry) (w w' : World) (p : Policy) (h : Home)
    (hr : r p = some h) (same : w.home h p = w'.home h p) :
    decide r w p = decide r w' p := by
  -- Both decisions read the home, which says the same in both worlds.
  simp [decide, hr, same]

/-- A policy that is not routed is decided by the node's own deployment
alone: no home's answer reaches it. -/
theorem unrouted_decided_by_own (r : Registry) (w w' : World) (p : Policy)
    (hr : r p = none) (same : w.own p = w'.own p) :
    decide r w p = decide r w' p := by
  -- Both decisions read the node's own deployment, which says the same.
  simp [decide, hr, same]

/-- A routed policy whose home is away is tried again, never waited on as
a refusal and never admitted. -/
theorem away_home_is_retried (r : Registry) (w : World) (p : Policy) (h : Home)
    (hr : r p = some h) (away : w.home h p = .away) :
    step (decide r w p) = .retry := by
  -- The decision is the home's `away`, which steps to `retry`.
  simp [decide, hr, away, step]

/-- Only a grant admits: a change merges, or an edit commits, only where
the deciding deployment said "yes". -/
theorem admitted_only_on_yes (a : Answer) (h : step a = .admit) : a = .yes := by
  -- Check each answer: only `yes` steps to `admit`.
  cases a <;> simp_all [step]

/-- Under a routed policy the node writes nothing of its own. -/
theorem routed_writes_nothing (r : Registry) (p : Policy) (h : Home) (hr : r p = some h) :
    ownedHere r p = false := by
  -- The route is there, so it is not `none`.
  simp [ownedHere, hr]

/-- Under a policy no home owns, the node writes as before. -/
theorem unrouted_writes (r : Registry) (p : Policy) (hr : r p = none) :
    ownedHere r p = true := by
  -- No route, so the node's own deployment takes the write.
  simp [ownedHere, hr]

/-- A new state of a home releases only changes parked under its policy. -/
theorem release_only_that_policy (parked : List Parked) (p : Policy) (k : Parked)
    (hk : k ∈ releasePolicy parked p) : k.policy = p := by
  -- A kept change passed the filter, which checks its policy.
  simp [releasePolicy] at hk
  exact hk.2

/-- A new state of a home releases every change parked under its policy. -/
theorem release_every_change_of_the_policy (parked : List Parked) (p : Policy) (k : Parked)
    (hk : k ∈ parked) (hp : k.policy = p) : k ∈ releasePolicy parked p := by
  -- The change is in the list and passes the filter.
  simp [releasePolicy, hk, hp]

/-- The parked changes a removed route leaves in the index: every one not
under `p`, kept in order. -/
def forgetPolicy (parked : List Parked) (p : Policy) : List Parked :=
  -- Keep exactly those whose policy is not `p`.
  parked.filter (fun k => k.policy != p)

/-- A removed route forgets every change parked under its policy. -/
theorem forget_drops_the_policy (parked : List Parked) (p : Policy) (k : Parked)
    (hk : k ∈ forgetPolicy parked p) : k.policy ≠ p := by
  -- A kept change passed the filter, which checks its policy is not `p`.
  simp [forgetPolicy] at hk
  exact hk.2

/-- A removed route keeps every change parked under another policy. -/
theorem forget_keeps_other_policies (parked : List Parked) (p : Policy) (k : Parked)
    (hk : k ∈ parked) (hp : k.policy ≠ p) : k ∈ forgetPolicy parked p := by
  -- The change is in the list and passes the filter.
  simp [forgetPolicy, hk, hp]

/-- Which policy IDs the node's own deployment holds. -/
def Held := Policy → Bool

/-- The rule both refusals keep: no routed policy is one the node's own
deployment holds. -/
def NoShadow (r : Registry) (own : Held) : Prop :=
  -- Every routed policy is one the node's own deployment does not hold.
  ∀ p, (r p).isSome → own p = false

/-- Route `p` to `h`, refused (nothing changes) while the node's own
deployment holds `p`. -/
def Registry.addChecked (r : Registry) (own : Held) (p : Policy) (h : Home) : Registry :=
  -- Held here: the registry is left as it was; otherwise `p` goes to `h`.
  if own p then r else r.add p h

/-- The node's own deployment creates `p`, refused before anything is sent
(nothing changes) while `p` is routed. -/
def Held.createChecked (own : Held) (r : Registry) (p : Policy) : Held :=
  -- Routed: the node's own deployment is left as it was; otherwise it holds `p`.
  fun q => if q = p ∧ (r p).isNone then true else own q

/-- A node given no foreign policy keeps the rule, whatever it holds. -/
theorem empty_no_shadow (own : Held) : NoShadow Registry.empty own := by
  -- The empty registry routes nothing, so there is nothing to check.
  intro p hp
  simp [Registry.empty] at hp

/-- A route the node's own deployment holds is refused. -/
theorem add_refused_when_held (r : Registry) (own : Held) (p : Policy) (h : Home)
    (held : own p = true) : r.addChecked own p h = r := by
  -- The `if` takes its first branch.
  simp [Registry.addChecked, held]

/-- A checked add keeps the rule. -/
theorem addChecked_no_shadow (r : Registry) (own : Held) (p : Policy) (h : Home)
    (safe : NoShadow r own) : NoShadow (r.addChecked own p h) own := by
  -- Either nothing changed, or `p` was added while not held.
  intro q hq
  unfold Registry.addChecked at hq
  by_cases hp : own p = true
  · -- Refused: the registry is the old one, which kept the rule.
    simp [hp] at hq
    exact safe q hq
  · -- Added: `q` is `p`, which is not held, or kept its old route.
    simp [hp] at hq
    by_cases hqp : q = p
    · subst hqp
      simpa using hp
    · simp [Registry.add, hqp] at hq
      exact safe q (by simpa using hq)

/-- A checked create keeps the rule. -/
theorem createChecked_no_shadow (r : Registry) (own : Held) (p : Policy)
    (safe : NoShadow r own) : NoShadow r (own.createChecked r p) := by
  -- A routed policy is either not `p` or `p` while routed: neither changes.
  intro q hq
  unfold Held.createChecked
  have hnone : ¬ (r q).isNone := by
    cases h : r q <;> simp_all
  by_cases hqp : q = p
  · subst hqp
    simp [hnone, safe q hq]
  · simp [hqp, safe q hq]

/-- Removing a route keeps the rule. -/
theorem remove_no_shadow (r : Registry) (own : Held) (p : Policy)
    (safe : NoShadow r own) : NoShadow (r.remove p) own := by
  -- A removed policy is no longer routed; any other keeps its old route.
  intro q hq
  by_cases hqp : q = p
  · simp [Registry.remove, hqp] at hq
  · simp [Registry.remove, hqp] at hq
    exact safe q (by simpa using hq)

end ForeignRoutes
