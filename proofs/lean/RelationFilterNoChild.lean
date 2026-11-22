/-!
# RelationFilterNoChild: a relation filter with no matching child reads no parent

The story, in plain words. A query such as
`Book(filter: {author: {name: {_eq: "Nobody"}}})` joins each Book (a parent)
to its author (a child). The join first reads the children the filter can
match into a cache, then walks the parents and keeps a parent only when its
child is in the cache and passes the filter. When the cache is empty, no
parent can be kept, so the join may return at once without reading a single
parent.

Who acts: the join (`TypeJoinOne` in its primary direction), holding the
child cache and walking the parent plan.

What can happen to each parent:
- its FK names a cached child that passes the filter: it is kept;
- its FK names no cached child, or a child that fails the filter: it is dropped;
- it has no FK at all: it is dropped, because a missing child passes no
  relation filter.

What must never happen:
- the short cut returning other rows than the full walk (`shortcut_same_rows`);
- the short cut being taken by an exhaustive join, which also returns the
  parents with no child (orphans): there the empty cache does not mean an empty
  result (`orphans_need_the_walk`), which is why the code requires that no
  orphan handling is configured.

Rust anchors: `TypeJoinOne::no_child_can_pass`, `TypeJoinOne::init` and
`TypeJoinOne::next` in `crates/engine/query/src/plan/type_join/type_join_one.rs`.
-/

namespace RelationFilterNoChild

-- A parent: its own ID, and the ID of the child its FK names, if any.
structure Parent where
  id : Nat            -- the parent's own ID
  fk : Option Nat     -- the child its FK names, or none for an orphan
  deriving DecidableEq

-- A kept row: the parent's ID paired with its child's ID.
abbrev Row := Nat × Nat

-- The full walk: every parent is read, and kept only when its child is in the
-- cache and passes the filter.
def rows (pass : Nat → Bool) (cache : List Nat) (parents : List Parent) : List Row :=
  parents.filterMap fun p =>          -- read each parent in order
    match p.fk with                   -- look at the child its FK names
    | some k =>                       -- it names child k:
        if k ∈ cache ∧ pass k         --   cached and passing the filter,
        then some (p.id, k)           --   so the parent is kept with it,
        else none                     --   else it is dropped
    | none => none                    -- no FK: a missing child passes nothing

-- The short cut: with an empty cache, return nothing without reading parents.
def rowsShortcut (pass : Nat → Bool) (cache : List Nat) (parents : List Parent) :
    List Row :=
  if cache = [] then []               -- nothing cached: no parent is read
  else rows pass cache parents        -- else the full walk

-- With an empty cache the full walk keeps no parent.
theorem empty_cache_keeps_nothing (pass : Nat → Bool) (parents : List Parent) :
    rows pass [] parents = [] := by
  induction parents with                     -- one parent at a time
  | nil => rfl                               -- no parents: nothing kept
  | cons p ps ih =>                          -- the first parent, then the rest
    cases h : p.fk <;>                       -- with or without an FK,
      simp [rows, List.filterMap, h] at ih ⊢ -- neither is kept
    <;> exact ih

-- The short cut returns exactly the rows of the full walk.
theorem shortcut_same_rows (pass : Nat → Bool) (cache : List Nat)
    (parents : List Parent) :
    rowsShortcut pass cache parents = rows pass cache parents := by
  unfold rowsShortcut                        -- open the short cut
  split                                      -- empty cache, or not
  · next h => subst h; exact (empty_cache_keeps_nothing pass parents).symm
  · rfl                                      -- not empty: it is the full walk

-- An exhaustive join also returns the orphans, parents with no FK.
def rowsExhaustive (pass : Nat → Bool) (cache : List Nat) (parents : List Parent) :
    List Nat :=
  (rows pass cache parents).map Prod.fst ++          -- the kept parents' IDs,
    (parents.filter fun p => p.fk = none).map Parent.id  -- then the orphans' IDs

-- For an exhaustive join an empty cache does not mean an empty result: one
-- orphan is still returned, so the short cut would lose it.
theorem orphans_need_the_walk :
    rowsExhaustive (fun _ => true) [] [⟨7, none⟩] = [7] := by
  decide

end RelationFilterNoChild
