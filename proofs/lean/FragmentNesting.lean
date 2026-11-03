/-!
# FragmentNesting: the fragment walk stops at the configured depth

The story, in plain words. A GraphQL query can name a fragment, and that
fragment can name another, and so on. The parser expands them by calling
itself once per fragment, so every fragment on a chain costs a slice of the
thread's stack. A client that sends a chain thousands long would run the
stack out and crash the node (#2465).

So the parser counts the fragments on the chain it is expanding. Before it
expands one more, it checks the count: when the count has reached the
configured limit, it refuses the query instead of going deeper.

What must never happen:
- the walk calls itself deeper than the limit (`reach_le`), so the stack it
  needs is bounded by the limit, which the node checks against the real
  thread stack at startup;
- a query is refused that nests no deeper than the limit, or accepted that
  nests deeper (`walk_none_iff`): the limit refuses exactly the deep ones.

The planted bug: a check written `limit < count` instead of
`limit ≤ count` lets one fragment more through than the limit allows
(`off_by_one_accepts_too_deep`).

A selection is a tree: a field, a fragment spread whose fragment holds a
selection, or two selections side by side.

Rust anchors: `FragmentPath::check_nesting` and its callers in
`parse_selection_to_selects`, `parse_selection_to_mutations` and
`parse_selection_set` (`crates/query/src/query_parse/limits.rs`,
`crates/query/src/query_parse/parser.rs`), and `collect_root_fields`
(`crates/query/src/subscription.rs`), which keep the same bound.
-/

namespace FragmentNesting

-- What a query's selection can be.
inductive Sel where
  -- a plain field: nothing to expand
  | field
  -- a fragment spread: expanding it means walking the fragment's selection
  | spread (body : Sel)
  -- two selections side by side in one selection set
  | both (a b : Sel)
  deriving Repr, DecidableEq

open Sel

-- How many fragments the deepest chain in a selection nests.
def depth : Sel → Nat
  -- a field nests no fragments
  | field => 0
  -- a spread nests one more than its fragment's selection
  | spread b => depth b + 1
  -- side by side, the deeper of the two counts
  | both a b => max (depth a) (depth b)

-- The walk, as the parser runs it: `d` fragments are already on the chain.
-- It answers `some r` (parsed, the deepest chain it reached) or `none`
-- (refused).
def walk (limit : Nat) (d : Nat) : Sel → Option Nat
  -- a field is parsed where it stands
  | field => some d
  -- a spread: refuse once the chain holds `limit`, else expand one deeper
  | spread b => if limit ≤ d then none else walk limit (d + 1) b
  -- side by side: both must parse; the deeper reach counts
  | both a b =>
    match walk limit d a, walk limit d b with
    -- both parsed: report the deeper of the two
    | some x, some y => some (max x y)
    -- either refused: the query is refused
    | _, _ => none

-- How deep the walk calls itself, refusals included. The parser walks the
-- items in order and stops at the first refusal, so a refused left side is
-- as far as it goes.
def reach (limit : Nat) (d : Nat) : Sel → Nat
  -- a field is visited at the depth it stands
  | field => d
  -- a spread that is refused goes no deeper; otherwise one level down
  | spread b => if limit ≤ d then d else reach limit (d + 1) b
  -- side by side: stop at the left when it is refused, else the deeper
  | both a b =>
    match walk limit d a with
    -- the left side refused: the walk ends there
    | none => reach limit d a
    -- the left side parsed: the right side is walked too
    | some _ => max (reach limit d a) (reach limit d b)

-- The walk never calls itself deeper than the limit: starting within the
-- limit, every depth it reaches is within it.
theorem reach_le (limit : Nat) :
    ∀ (s : Sel) (d : Nat), d ≤ limit → reach limit d s ≤ limit := by
  intro s
  -- one case per kind of selection
  induction s with
  -- a field: it stays where it starts
  | field => intro d h; simp [reach]; exact h
  -- a spread: refused at the limit, or one level down while below it
  | spread b ih =>
    intro d h
    simp only [reach]
    split
    -- refused: still at `d`
    · exact h
    -- below the limit, so one more level still fits
    · rename_i hlt; exact ih (d + 1) (by omega)
  -- side by side: whichever branch runs, each side stays within the limit
  | both a b iha ihb =>
    intro d h
    simp only [reach]
    split
    -- only the left side ran
    · exact iha d h
    -- both sides ran, and each stays within the limit
    · exact Nat.max_le.mpr ⟨iha d h, ihb d h⟩

-- The deeper of two chains fits exactly when both do.
theorem fit_max (d x y limit : Nat) :
    d + max x y ≤ limit ↔ d + x ≤ limit ∧ d + y ≤ limit := by
  -- whichever of the two is deeper is the max
  rcases Nat.le_total x y with hxy | hyx
  -- the right one is deeper
  · rw [Nat.max_eq_right hxy]; omega
  -- the left one is deeper
  · rw [Nat.max_eq_left hyx]; omega

-- Adding the same start to both chains keeps the deeper one deeper.
theorem add_max (d x y : Nat) : d + max x y = max (d + x) (d + y) := by
  -- whichever of the two is deeper is the max, before and after adding `d`
  rcases Nat.le_total x y with hxy | hyx
  -- the right one is deeper
  · rw [Nat.max_eq_right hxy, Nat.max_eq_right (by omega)]
  -- the left one is deeper
  · rw [Nat.max_eq_left hyx, Nat.max_eq_left (by omega)]

-- The walk's answer, in one line: starting within the limit, it parses
-- exactly when the chain fits (`d + depth s ≤ limit`), and then it reached
-- `d + depth s`.
theorem walk_eq (limit : Nat) :
    ∀ (s : Sel) (d : Nat), d ≤ limit →
      walk limit d s = if d + depth s ≤ limit then some (d + depth s) else none := by
  intro s
  -- one case per kind of selection
  induction s with
  -- a field fits wherever the walk stands
  | field => intro d h; simp [walk, depth, h]
  -- a spread: refused at the limit, or its fragment walked one deeper
  | spread b ih =>
    intro d h
    simp only [walk, depth]
    split
    -- at the limit: refused, and one more fragment would not fit either
    · rename_i hle
      have : ¬ (d + (depth b + 1) ≤ limit) := by omega
      simp [this]
    -- below the limit: the fragment's selection is walked from `d + 1`
    · rename_i hlt
      rw [ih (d + 1) (by omega)]
      have e : d + 1 + depth b = d + (depth b + 1) := by omega
      rw [e]
  -- side by side: both parse exactly when the deeper one fits
  | both a b iha ihb =>
    intro d h
    simp only [walk, depth]
    rw [iha d h, ihb d h]
    by_cases ha : d + depth a ≤ limit
    · by_cases hb : d + depth b ≤ limit
      -- both fit: so does the deeper, and the reach is the deeper one
      · have hf := (fit_max d (depth a) (depth b) limit).mpr ⟨ha, hb⟩
        simp [ha, hb, hf, add_max, Nat.max_le.mpr ⟨ha, hb⟩]
      -- the right side is too deep: so is the pair
      · have hf : ¬ (d + max (depth a) (depth b) ≤ limit) :=
          fun hm => hb ((fit_max d (depth a) (depth b) limit).mp hm).2
        simp [ha, hb, hf]
    -- the left side is too deep: so is the pair
    · have hf : ¬ (d + max (depth a) (depth b) ≤ limit) :=
        fun hm => ha ((fit_max d (depth a) (depth b) limit).mp hm).1
      simp [ha, hf]

-- The limit refuses exactly the queries that nest deeper than it.
theorem walk_none_iff (limit : Nat) (s : Sel) :
    walk limit 0 s = none ↔ limit < depth s := by
  rw [walk_eq limit s 0 (Nat.zero_le _)]
  by_cases h : 0 + depth s ≤ limit <;> simp [h] <;> omega

-- A query the walk accepts nests no deeper than the limit.
theorem accepted_within_limit (limit : Nat) (s : Sel) (r : Nat)
    (h : walk limit 0 s = some r) : r ≤ limit := by
  rw [walk_eq limit s 0 (Nat.zero_le _)] at h
  split at h
  -- it parsed: the reach is the depth, which fits
  · rename_i hfit; simp at h; omega
  -- refused: no answer to bound
  · simp at h

-- The planted bug: the check `limit < d` lets the chain grow one past the
-- limit before it refuses.
def offByOneWalk (limit : Nat) (d : Nat) : Sel → Option Nat
  -- a field is parsed where it stands
  | field => some d
  -- the wrong comparison: refuse only once the chain is past the limit
  | spread b => if limit < d then none else offByOneWalk limit (d + 1) b
  -- side by side, as in the real walk
  | both a b =>
    match offByOneWalk limit d a, offByOneWalk limit d b with
    -- both parsed
    | some x, some y => some (max x y)
    -- either refused
    | _, _ => none

-- With a limit of one, two nested fragments should be refused; the
-- off-by-one walk parses them, reaching depth two.
theorem off_by_one_accepts_too_deep :
    offByOneWalk 1 0 (spread (spread field)) = some 2 ∧
      walk 1 0 (spread (spread field)) = none := by
  decide

end FragmentNesting
