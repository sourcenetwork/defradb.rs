/-!
# JoinFetchedParent: a parent read by key is joined as the plan it skipped would join it

The story, in plain words. A query such as
`Book(filter: {author: {name: {_eq: "Alice"}}}) { title publisher { name } }`
has two joins on each Book (a parent): the outer one to its author, the inner
one to its publisher. When the author's name and the Book's author FK are both
indexed, the outer join does not run the inner plan. It reads the matching
authors, and looks each one's Books up by key. Those Books come back raw, so
the outer join hands each one to the inner join, which joins it exactly as it
joins a Book it scanned itself.

Who acts: the outer join (`TypeJoinOne` driven by its child's index), which
finds the parents, and the inner join (the plan the outer one wraps), which
holds a cache of the children it may attach and, sometimes, a filter on them.

What can happen to each parent the outer join finds:
- the inner join has no filter: the parent is kept, with its cached child or
  with none;
- the inner join has a filter, and the parent's child is cached and passes it:
  the parent is kept with that child;
- the inner join has a filter, and the child is missing or fails it: the parent
  is dropped.

What must never happen:
- the lookup path returning other rows than the full walk, where the inner
  plan joins every parent and the outer join then keeps the ones it selects
  (`lookup_same_rows`);
- a parent whose child fails the inner filter being returned
  (`lookup_keeps_only_passing`);
- the child being attached without asking the inner join, which is the bug
  `raw_attach_keeps_a_rejected_parent` plants: the filter is skipped and a
  rejected parent comes back.

The inner join can also be a list relation, such as a Book's reviews: it
attaches every cached child whose FK names the parent, and its filter keeps a
parent when at least one of those children passes. The same promises hold for
it (`lookup_many_same_rows`, `lookup_many_keeps_only_passing`), and the bug
`pass_through_drops_the_children` plants is the one where the list join hands
the parent on without attaching its children: the parent comes back with an
empty list.

Rust anchors: `TypeJoinOne::join_fetched`, `TypeJoinOne::child_of_fetched`,
`TypeJoinOne::next_inverted_index` and `TypeJoinOne::next_ordered_primary_inner`
in `crates/engine/query/src/plan/type_join/type_join_one.rs`;
`TypeJoinMany::join_fetched` in
`crates/engine/query/src/plan/type_join/type_join_many/plan_node.rs` and
`TypeJoinMany::join_children_of_fetched` in
`crates/engine/query/src/plan/type_join/type_join_many/fetched.rs`.
-/

namespace JoinFetchedParent

-- A parent: its own ID, and the ID of the child its inner FK names, if any.
structure Parent where
  id : Nat            -- the parent's own ID
  fk : Option Nat     -- the child its inner FK names, or none
  deriving DecidableEq

-- A returned row: the parent's ID with the child joined into it, if any.
abbrev Row := Nat × Option Nat

-- The child the inner join finds for a parent: the one its FK names, when
-- that child is in the cache.
def childOf (cache : List Nat) (p : Parent) : Option Nat :=
  match p.fk with                     -- look at the child the FK names
  | some k =>                         -- it names child k:
      if cache.contains k             --   k is cached,
      then some k                     --   so it is the child,
      else none                       --   else there is no child
  | none => none                      -- no FK: no child

-- The inner join on one parent. `filtered` says whether it has a filter, and
-- `pass` is that filter.
def joinOne (filtered : Bool) (pass : Nat → Bool) (cache : List Nat) (p : Parent) :
    Option Row :=
  match childOf cache p with          -- find the parent's child
  | some k =>                         -- there is one:
      if filtered && !pass k          --   a filter that it fails
      then none                       --   drops the parent,
      else some (p.id, some k)        --   else the parent is kept with it
  | none =>                           -- there is none:
      if filtered                     --   a filter is passed by no missing child,
      then none                       --   so the parent is dropped,
      else some (p.id, none)          --   else it is kept with no child

-- The full walk: the inner plan joins every parent, and the outer join keeps
-- the rows of the parents it selects (`sel`).
def walkRows (sel : Parent → Bool) (filtered : Bool) (pass : Nat → Bool)
    (cache : List Nat) (parents : List Parent) : List Row :=
  parents.filterMap fun p =>                   -- every parent is read
    if sel p                                   -- the outer join selects it:
    then joinOne filtered pass cache p         --   the inner join decides,
    else none                                  -- else it is not returned

-- The lookup path: the outer join reads only the parents it selects, by key,
-- and hands each one to the inner join.
def lookupRows (sel : Parent → Bool) (filtered : Bool) (pass : Nat → Bool)
    (cache : List Nat) (parents : List Parent) : List Row :=
  (parents.filter sel).filterMap               -- only the selected parents,
    (joinOne filtered pass cache)              -- each joined by the inner join

-- The lookup path returns exactly the rows of the full walk.
theorem lookup_same_rows (sel : Parent → Bool) (filtered : Bool) (pass : Nat → Bool)
    (cache : List Nat) (parents : List Parent) :
    lookupRows sel filtered pass cache parents
      = walkRows sel filtered pass cache parents := by
  induction parents with                       -- one parent at a time
  | nil => rfl                                 -- no parents: no rows either way
  | cons p ps ih =>                            -- the first parent, then the rest
    unfold lookupRows walkRows at ih ⊢         -- open both paths
    cases h : sel p <;>                        -- selected or not,
      simp [List.filter, List.filterMap, h, ih] -- both paths treat it alike

-- With an inner filter, every returned row carries a child that passes it.
theorem lookup_keeps_only_passing (sel : Parent → Bool) (pass : Nat → Bool)
    (cache : List Nat) (parents : List Parent) (r : Row)
    (hr : r ∈ lookupRows sel true pass cache parents) :
    ∃ k, r.2 = some k ∧ pass k = true := by
  unfold lookupRows at hr                      -- open the lookup path
  rw [List.mem_filterMap] at hr                -- the row came from some parent
  obtain ⟨p, _, hp⟩ := hr                      -- call that parent p
  unfold joinOne at hp                         -- open the inner join on p
  cases hc : childOf cache p with              -- p has a child or not
  | none => simp [hc] at hp                    -- none: p was dropped, no row
  | some k =>                                  -- child k:
    cases hk : pass k with                     -- k passes the filter or not
    | false => simp [hc, hk] at hp             -- fails: p was dropped, no row
    | true =>                                  -- passes: the row carries k
      simp [hc, hk] at hp                      -- the row is (p.id, some k)
      exact ⟨k, by rw [← hp], hk⟩              -- so its child is k, which passes

-- The planted bug: attach the child the FK names without asking the inner
-- join, so its filter is never consulted.
def rawAttachRows (sel : Parent → Bool) (parents : List Parent) : List Row :=
  (parents.filter sel).map fun p => (p.id, p.fk)  -- every selected parent is kept

-- Parent 7 names child 3, which fails the inner filter. The full walk drops
-- it, and the raw attach returns it.
theorem raw_attach_keeps_a_rejected_parent :
    walkRows (fun _ => true) true (fun _ => false) [3] [⟨7, some 3⟩] = [] ∧
    rawAttachRows (fun _ => true) [⟨7, some 3⟩] = [(7, some 3)] := by
  decide

-- A list relation's cached child: its own ID, and the parent its FK names.
abbrev Kid := Nat × Nat

-- A returned row of a list join: the parent's ID with all its children.
abbrev ManyRow := Nat × List Nat

-- The children the list join finds for a parent: every cached child whose FK
-- names it, in cache order.
def childrenOf (kids : List Kid) (p : Parent) : List Nat :=
  (kids.filter fun k => k.2 == p.id).map Prod.fst  -- the IDs of p's children

-- The list join on one parent. A filter keeps the parent when at least one
-- child passes it; the parent is then kept with all its children.
def joinMany (filtered : Bool) (pass : Nat → Bool) (kids : List Kid) (p : Parent) :
    Option ManyRow :=
  if filtered && !((childrenOf kids p).any pass)  -- a filter no child passes
  then none                                       -- drops the parent,
  else some (p.id, childrenOf kids p)             -- else it is kept with them

-- The full walk with a list join inside: every parent is joined, and the
-- outer join keeps the rows of the parents it selects.
def walkManyRows (sel : Parent → Bool) (filtered : Bool) (pass : Nat → Bool)
    (kids : List Kid) (parents : List Parent) : List ManyRow :=
  parents.filterMap fun p =>                   -- every parent is read
    if sel p                                   -- the outer join selects it:
    then joinMany filtered pass kids p         --   the list join decides,
    else none                                  -- else it is not returned

-- The lookup path with a list join inside: only the selected parents, each
-- handed to the list join.
def lookupManyRows (sel : Parent → Bool) (filtered : Bool) (pass : Nat → Bool)
    (kids : List Kid) (parents : List Parent) : List ManyRow :=
  (parents.filter sel).filterMap               -- only the selected parents,
    (joinMany filtered pass kids)              -- each joined by the list join

-- With a list join inside, the lookup path returns exactly the rows of the
-- full walk.
theorem lookup_many_same_rows (sel : Parent → Bool) (filtered : Bool)
    (pass : Nat → Bool) (kids : List Kid) (parents : List Parent) :
    lookupManyRows sel filtered pass kids parents
      = walkManyRows sel filtered pass kids parents := by
  induction parents with                       -- one parent at a time
  | nil => rfl                                 -- no parents: no rows either way
  | cons p ps ih =>                            -- the first parent, then the rest
    unfold lookupManyRows walkManyRows at ih ⊢ -- open both paths
    cases h : sel p <;>                        -- selected or not,
      simp [List.filter, List.filterMap, h, ih] -- both paths treat it alike

-- With a filter on the list join, every returned row has a child that passes.
theorem lookup_many_keeps_only_passing (sel : Parent → Bool) (pass : Nat → Bool)
    (kids : List Kid) (parents : List Parent) (r : ManyRow)
    (hr : r ∈ lookupManyRows sel true pass kids parents) :
    ∃ k, k ∈ r.2 ∧ pass k = true := by
  unfold lookupManyRows at hr                  -- open the lookup path
  rw [List.mem_filterMap] at hr                -- the row came from some parent
  obtain ⟨p, _, hp⟩ := hr                      -- call that parent p
  unfold joinMany at hp                        -- open the list join on p
  cases ha : (childrenOf kids p).any pass with -- some child passes or none
  | false => simp [ha] at hp                   -- none: p was dropped, no row
  | true =>                                    -- one does: the row carries them
    simp [ha] at hp                            -- the row is (p.id, children)
    rw [List.any_eq_true] at ha                -- name the passing child
    obtain ⟨k, hk, hpk⟩ := ha                  -- k, one of p's children
    exact ⟨k, by rw [← hp]; exact hk, hpk⟩     -- it is in the row and passes

-- The planted bug: the list join hands the parent on without attaching its
-- children, so the parent comes back with an empty list.
def passThroughRows (sel : Parent → Bool) (parents : List Parent) : List ManyRow :=
  (parents.filter sel).map fun p => (p.id, [])  -- no children attached

-- Parent 7 has the child 3. The full walk returns it with that child, and the
-- pass-through returns it with none.
theorem pass_through_drops_the_children :
    walkManyRows (fun _ => true) false (fun _ => true) [(3, 7)] [⟨7, none⟩]
      = [(7, [3])] ∧
    passThroughRows (fun _ => true) [⟨7, none⟩] = [(7, [])] := by
  decide

end JoinFetchedParent
