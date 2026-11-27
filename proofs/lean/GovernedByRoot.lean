/-!
What a node does with a replicated composite depends on the collection's
definition and on what the node installed when it was built.

A node may have installed no merge governance at all. If it installed some, it
claimed a list of collection names and has a merge validator or has none. A
collection is governed on such a node when its name was claimed, or when the
node has a validator and the collection's definition declares a governance
root and carries no access-control policy. An ungoverned composite merges as in any collection; a governed one goes
to the validator; a governed one on a node with no validator is held
unmerged.

Before roots governed anything, only the claimed names counted. The theorems
below say four things. A node with a validator judges every rooted collection
that carries no policy, whatever names it claimed, so two such nodes agree on
it. A node with no validator, or with no governance, does exactly what it did
before. A collection that carries a policy is handled exactly as before on
every node, so access control keeps it. And the change only ever adds
judgement: whatever was judged before is judged still.

Rust owner: crates/database/db/src/merge/governance/validator.rs
MergeGovernance::governs; crates/database/db/src/merge/governance/judge.rs
DbMergeHandler::is_governed and DbMergeHandler::judge_governed.
-/
namespace GovernedByRoot

-- What the decision reads of a collection: its name, its governance root
-- when its definition declares one, and whether it carries an access-control
-- policy.
structure Collection where
  name : String
  root : Option String
  policy : Bool

-- What a node installed as merge governance: the names it claimed, and
-- whether it has a merge validator.
structure Governance where
  claimed : List String
  validator : Bool

-- A collection is governed under installed governance when its name was
-- claimed, or when there is a validator and the collection declares a root
-- and carries no policy.
def governs (g : Governance) (c : Collection) : Bool :=
  g.claimed.contains c.name || (g.validator && c.root.isSome && !c.policy)

-- The rule before roots governed anything: the claimed names only.
def governsBefore (g : Governance) (c : Collection) : Bool :=
  g.claimed.contains c.name

-- Whether a node governs a collection. A node with no governance installed
-- (`none`) governs nothing.
def isGoverned (node : Option Governance) (c : Collection) : Bool :=
  match node with
  | none => false
  | some g => governs g c

-- The same question under the rule from before.
def isGovernedBefore (node : Option Governance) (c : Collection) : Bool :=
  match node with
  | none => false
  | some g => governsBefore g c

-- Whether the node has a merge validator. A node with no governance has none.
def hasValidator (node : Option Governance) : Bool :=
  match node with
  | none => false
  | some g => g.validator

-- What the merge path does with a replicated composite.
inductive Path where
  -- It merges as in any collection, judged by nobody here.
  | ungoverned
  -- It is handed to the merge validator, which accepts, rejects or defers.
  | judged
  -- It is left unmerged, because nothing on this node can judge it.
  | held
  deriving DecidableEq, Repr

-- The path a composite takes, given whether its collection is governed and
-- whether the node has a validator.
def route (governed validator : Bool) : Path :=
  if governed then (if validator then Path.judged else Path.held)
  else Path.ungoverned

-- The path of a composite of `c` on a node.
def path (node : Option Governance) (c : Collection) : Path :=
  route (isGoverned node c) (hasValidator node)

-- The path it took before roots governed anything.
def pathBefore (node : Option Governance) (c : Collection) : Path :=
  route (isGovernedBefore node c) (hasValidator node)

-- On a node with no governance installed, every composite merges as in any
-- collection, rooted or not.
theorem no_governance_is_ungoverned (c : Collection) :
    path none c = Path.ungoverned := by
  -- Unfolding the definitions leaves `route false false`, which is the
  -- ungoverned path by computation.
  rfl

-- A node with no governance installed does what it did before.
theorem no_governance_as_before (c : Collection) :
    path none c = pathBefore none c := by
  -- Both sides compute to the ungoverned path.
  rfl

-- A node that installed governance without a validator does what it did
-- before: the root arm needs a validator, so only the claimed names count.
theorem no_validator_as_before (claimed : List String) (c : Collection) :
    path (some ⟨claimed, false⟩) c = pathBefore (some ⟨claimed, false⟩) c := by
  -- Unfold both sides down to the two `governs` tests. With no validator the
  -- root arm is `false && _`, which is `false`, and `x || false` is `x`, so
  -- simp finds the two sides are the same expression.
  simp [path, pathBefore, isGoverned, isGovernedBefore, governs, governsBefore,
    hasValidator]

-- On a node with a validator, a composite of a rooted collection that
-- carries no policy is judged, whatever names the node claimed.
theorem rooted_with_a_validator_is_judged (claimed : List String) (c : Collection)
    (h : c.root.isSome = true) (open_ : c.policy = false) :
    path (some ⟨claimed, true⟩) c = Path.judged := by
  -- Unfold the definitions and use `h` and `open_`: the root arm is
  -- `true && true && !false`, so the collection is governed whatever the
  -- name arm says, and with a validator simp computes the route to `judged`.
  simp [path, isGoverned, governs, hasValidator, route, h, open_]

-- Two nodes with validators that claimed different names agree on the path
-- of a rooted collection that carries no policy.
theorem validating_nodes_agree_on_rooted (claimed other : List String)
    (c : Collection) (h : c.root.isSome = true) (open_ : c.policy = false) :
    path (some ⟨claimed, true⟩) c = path (some ⟨other, true⟩) c := by
  -- Rewrite each side with the theorem above; both become `judged`, and
  -- `rw` closes `judged = judged` itself.
  rw [rooted_with_a_validator_is_judged claimed c h open_,
    rooted_with_a_validator_is_judged other c h open_]

-- Access control takes precedence over a declared root: a collection that
-- carries a policy takes the path it took before, on every node, whatever
-- root it declares.
theorem policied_as_before (node : Option Governance) (c : Collection)
    (policied : c.policy = true) : path node c = pathBefore node c := by
  -- Look at the two kinds of node one at a time.
  cases node with
  -- No governance: both sides compute to the ungoverned path.
  | none => rfl
  -- Governance `g`: unfold both sides down to the two `governs` tests. With
  -- a policy the root arm ends in `!true`, which is `false`, so the arm is
  -- `false` and `x || false` is `x`; simp finds the two sides are the same
  -- expression.
  | some g =>
    simp [path, pathBefore, isGoverned, isGovernedBefore, governs, governsBefore,
      policied]

-- A node with a validator never holds a composite: it judges or merges it.
theorem validator_never_holds (claimed : List String) (c : Collection) :
    path (some ⟨claimed, true⟩) c ≠ Path.held := by
  -- Unfold to the `if` inside `route`, with the validator question left as
  -- `hasValidator`.
  unfold path route
  -- Split on whether the collection is governed. Governed: the validator is
  -- there, so the path is `judged`. Not governed: it is `ungoverned`. simp
  -- computes each and sees it differs from `held`.
  cases isGoverned (some ⟨claimed, true⟩) c <;> simp [hasValidator]

-- A collection the node claimed by name is governed, root or no root.
theorem claimed_is_governed (g : Governance) (c : Collection)
    (h : g.claimed.contains c.name = true) : isGoverned (some g) c = true := by
  -- Unfold; the match on `some g` reduces to `governs g c`.
  dsimp only [isGoverned, governs]
  -- The name arm is `true` by `h`.
  rw [h]
  -- `true || _` computes to `true`.
  rfl

-- With no validator, a composite of a claimed collection is held, as before.
theorem claimed_without_a_validator_is_held (claimed : List String) (c : Collection)
    (h : claimed.contains c.name = true) :
    path (some ⟨claimed, false⟩) c = Path.held := by
  -- Unfold; the matches on `some _` reduce.
  dsimp only [path, isGoverned, governs, hasValidator, route]
  -- The name arm is `true` by `h`.
  rw [h]
  -- Governed, no validator: the `if`s compute to `held`.
  rfl

-- A collection that declares no root and that the node did not claim merges
-- as it always did, validator or no validator.
theorem rootless_and_unclaimed_is_ungoverned (claimed : List String) (validator : Bool)
    (c : Collection) (rootless : c.root = none)
    (unclaimed : claimed.contains c.name = false) :
    path (some ⟨claimed, validator⟩) c = Path.ungoverned := by
  -- Unfold; the matches on `some _` reduce.
  dsimp only [path, isGoverned, governs, hasValidator, route]
  -- The root is `none` and the name arm is `false`.
  rw [rootless, unclaimed]
  -- Either way the validator goes, `none.isSome` is `false`, so the
  -- collection is not governed and the `if` computes to `ungoverned`.
  cases validator <;> rfl

-- The change only adds judgement: a composite judged before is judged now.
theorem judged_before_is_judged_now (node : Option Governance) (c : Collection)
    (h : pathBefore node c = Path.judged) : path node c = Path.judged := by
  -- Look at the two kinds of node one at a time.
  cases node with
  -- No governance: the old path computes to `ungoverned`, so the hypothesis
  -- says `ungoverned = judged`, which simp refutes.
  | none => simp [pathBefore, isGovernedBefore, route] at h
  -- Governance `g`: split on its validator and on whether it claimed the
  -- name, four cases in all. In each, simp computes both paths from the
  -- definitions and the two facts. Where the old path was not `judged` the
  -- hypothesis is false; where it was (claimed, with a validator) the name
  -- arm makes the new path `judged` too.
  | some g =>
    cases hv : g.validator <;> cases hc : g.claimed.contains c.name <;>
      simp_all [path, pathBefore, isGoverned, isGovernedBefore, governs,
        governsBefore, hasValidator, route]

end GovernedByRoot
