/-!
# JsonCrdt.Lww: one path's winner does not depend on the order edits arrive in

The story, in plain words. Every path in a JSON field is a register. Each
edit to it comes from a block, and the block carries a priority (its height in
the DAG) and a CID. The register keeps the edit that wins: the higher
priority, and on a tie the higher CID. Peers receive edits in different
orders, and each folds them into its register with `join`.

A block's CID is the hash of its content, so the CID fixes what the edit
says. The register can therefore keep just the winner's priority and CID; the
value is looked up by CID.

What must never happen: two peers that joined the same edits in different
orders, or joined one edit twice, keep different winners. That is exactly
`join` being commutative (`join_comm`), associative (`join_assoc`) and
idempotent (`join_idem`), with "no edit yet" as its identity (`join_none`).

The temporal story (peers, blocks, deletes, edits at different depths) is the
TLA+ model `proofs/tla/JsonCrdt.tla`.

Rust anchors: the JSON CRDT is not written yet. This file is its contract
(issue #2129), for the per-path registers in `crates/defra-core/src/json_crdt.rs`.
-/

namespace JsonCrdt

-- What a register remembers about an edit: its block's priority and CID.
structure Key where
  -- The block's height in the DAG.
  prio : Nat
  -- The block's CID, here a number.
  cid : Nat
  deriving DecidableEq

-- Key a is at least key b: a higher priority, or the same priority and a CID
-- at least as high.
def Key.ge (a b : Key) : Prop :=
  -- Higher priority, or a tie settled by the CID.
  a.prio > b.prio ∨ (a.prio = b.prio ∧ a.cid ≥ b.cid)

-- The comparison can always be decided.
instance (a b : Key) : Decidable (a.ge b) :=
  -- It is an "or" of decidable comparisons of numbers.
  inferInstanceAs (Decidable (a.prio > b.prio ∨ (a.prio = b.prio ∧ a.cid ≥ b.cid)))

-- The winner of two edits: the one whose key is at least the other's.
def pick (a b : Key) : Key :=
  -- a when it wins or ties, b otherwise.
  if a.ge b then a else b

-- Folding an edit into a register: an empty register takes the edit; a full
-- one keeps whichever wins.
def join : Option Key → Option Key → Option Key
  -- Nothing yet on the left: the right side.
  | none, y => y
  -- Nothing on the right: the left side.
  | x, none => x
  -- Two edits: the winner.
  | some a, some b => some (pick a b)

-- Two keys with the same priority and CID are the same key.
theorem Key.ext' {a b : Key} (hp : a.prio = b.prio) (hc : a.cid = b.cid) : a = b := by
  -- Take both apart and compare the fields.
  cases a; cases b; simp_all

/-- Picking between two edits gives the same winner whichever comes first. -/
theorem pick_comm (a b : Key) : pick a b = pick b a := by
  -- Unfold the comparison on both sides.
  unfold pick Key.ge
  -- Try every outcome of the two comparisons.
  split <;> split
  -- Both said "I win": then the keys are equal.
  · exact Key.ext' (by omega) (by omega)
  -- One won each way round: the same key.
  · rfl
  · rfl
  -- Neither won: impossible, one of two keys is always at least the other.
  · exfalso; omega

/-- Picking among three edits gives the same winner however they are grouped. -/
theorem pick_assoc (a b c : Key) : pick (pick a b) c = pick a (pick b c) := by
  -- Try every outcome of the comparisons among a, b and c.
  unfold pick
  by_cases h1 : a.ge b <;> by_cases h2 : b.ge c <;> by_cases h3 : a.ge c <;>
    simp only [h1, h2, h3, if_true, if_false] <;>
    unfold Key.ge at h1 h2 h3 <;>
    -- Each case is the same key, an impossible order, or a full tie.
    first
      | rfl
      | (exfalso; omega)
      | exact Key.ext' (by omega) (by omega)

/-- Picking an edit against itself gives the edit. -/
theorem pick_idem (a : Key) : pick a a = a := by
  -- A key is at least itself.
  unfold pick Key.ge; simp

/-- A register with nothing in it changes nothing when joined. -/
theorem join_none (x : Option Key) : join none x = x ∧ join x none = x := by
  -- Check both sides for an empty and a full register.
  cases x <;> simp [join]

/-- Joining two registers gives the same register whichever comes first. -/
theorem join_comm (x y : Option Key) : join x y = join y x := by
  -- Empty registers are the identity; two full ones reduce to pick.
  cases x <;> cases y <;> simp [join, pick_comm]

/-- Joining three registers gives the same register however they are grouped. -/
theorem join_assoc (x y z : Option Key) : join (join x y) z = join x (join y z) := by
  -- Empty registers drop out; three full ones reduce to pick.
  cases x <;> cases y <;> cases z <;> simp [join, pick_assoc]

/-- Joining a register with itself, as when an edit arrives twice, changes nothing. -/
theorem join_idem (x : Option Key) : join x x = x := by
  -- An empty register stays empty; a full one picks its own edit.
  cases x <;> simp [join, pick_idem]

end JsonCrdt
