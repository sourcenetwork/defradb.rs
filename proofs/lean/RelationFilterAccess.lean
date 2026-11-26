/-!
# RelationFilterAccess: a filter through a relation sees only readable documents

The story, in plain words. A query such as
`Employee(filter: {company: {name: {_eq: "Acme"}}}) { name }` keeps an
employee (a parent) when its company (a related child) passes the condition.
Some related documents may be documents the reader is not allowed to read.

Who acts: the reader, who sends the query; the join, which reads the related
children through plans and decides which parents to keep.

The plans: when the query does not select the relation, one plan reads the
children only to decide the filter. When it selects the relation, two plans
may read them: the child scan, whose children the reader is shown, and the
plan the filter is decided from. That is the child scan itself, or, when an
index serves the filter on a one-to-many relation, a separate index plan.
Each plan has the access gate or not.

What can happen to each child: the reader may read it or not, and it passes
the condition or not.

What must never happen:
- a parent kept by a child the reader is not shown: a selection whose two
  plans are gated keeps exactly the parents a shown child passes for, and the
  gated filter-only join keeps the same parents (`gated_matches_selection`);
  a selection whose filter plan has no gate keeps a parent no shown child
  passes for (`ungated_filter_plan_mismatch`);
- the answer depending on a child the reader may not read, which would tell the
  reader something about that child (`gated_ignores_unreadable`); the join
  without the access gate does depend on it (`ungated_reveals_unreadable`);
- the gate changing anything for a reader who may read every child, or a
  collection nothing gates: no policy, no app read validator and not a
  `@chunk` collection (`all_readable_unchanged`).

Rust anchors: `Planner::apply_filter_only_joins` in
`crates/engine/query/src/planner/joins/filter_only.rs`,
`Planner::apply_secondary_id_joins` in
`crates/engine/query/src/planner/joins/secondary_id.rs`, and the index plan
`filter_child_plan` that `Planner::build_relation_child_plan` builds in
`crates/engine/query/src/planner/joins/child_plan.rs`. Each wraps its plan with
`Planner::maybe_wrap_with_acp_filter`, as the child scan of a selection is
wrapped. A secondary relation ID is the one-child case: the related document's
ID is shown, and matched by a filter, only when the reader may read it. A
mutation with a relation filter picks its documents with the same select, read
as the caller.
-/

namespace RelationFilterAccess -- Keep these definitions together.

-- A related document, as the join sees it.
structure Child where
  readable : Bool -- whether the reader may read it: its policy, any app read validator and a chunk's source all allow it
  passes : Bool   -- whether it passes the filter's condition
  deriving DecidableEq

-- The children a plan yields: with the gate only the readable ones, without it every one.
def yields (gated : Bool) (children : List Child) : List Child :=
  children.filter (fun c => !gated || c.readable) -- a gated plan leaves out an unreadable child

-- Whether a join keeps the parent, decided from the children one plan yields.
def keepsFrom (gated : Bool) (children : List Child) : Bool :=
  (yields gated children).any (·.passes) -- some yielded child passes

-- The filter-only join with the access gate.
def gatedKeeps (children : List Child) : Bool :=
  keepsFrom true children -- decided from the readable children

-- The same join without the gate.
def ungatedKeeps (children : List Child) : Bool :=
  keepsFrom false children -- decided from every child

-- A join that selects the relation, by whether each of its two plans has the gate.
structure Selection where
  scanGated : Bool   -- the child scan, whose children the reader is shown
  filterGated : Bool -- the plan the filter is decided from: the child scan, or the index plan

-- The children a selection shows the reader.
def Selection.shows (s : Selection) (children : List Child) : List Child :=
  yields s.scanGated children -- what the child scan yields

-- Whether a selection keeps the parent.
def Selection.keeps (s : Selection) (children : List Child) : Bool :=
  keepsFrom s.filterGated children -- decided from the filter plan

-- With the gate, a plan yields exactly the readable children.
theorem yields_gated (children : List Child) :
    yields true children = children.filter (·.readable) := by
  simp [yields] -- `!true || r` is `r`

-- Without the gate, a plan yields every child.
theorem yields_ungated (children : List Child) :
    yields false children = children := by
  simp [yields] -- `!false || r` is `true`, and that keeps every child

-- Two lists of children the reader cannot tell apart: the readable children
-- are the same, and an unreadable child may hold anything.
inductive Same : List Child → List Child → Prop
  -- No children on either side.
  | nil : Same [] []
  -- The same readable child on both sides.
  | readable (c : Child) {as bs : List Child} :
      c.readable = true → Same as bs → Same (c :: as) (c :: bs)
  -- An unreadable child on each side, whatever it holds.
  | hidden (c d : Child) {as bs : List Child} :
      c.readable = false → d.readable = false → Same as bs → Same (c :: as) (d :: bs)

-- A selection whose two plans have the gate keeps a parent exactly when a
-- child it shows passes, and the gated filter-only join keeps the same parents.
theorem gated_matches_selection (s : Selection) (children : List Child)
    (hscan : s.scanGated = true) (hfilter : s.filterGated = true) :
    s.keeps children = (s.shows children).any (·.passes) ∧
      gatedKeeps children = s.keeps children := by
  simp [Selection.keeps, Selection.shows, keepsFrom, gatedKeeps, hscan, hfilter] -- both read the readable children

-- A selection whose filter plan has no gate keeps a parent although no child
-- it shows passes: the parent is kept by a child the reader may not read.
theorem ungated_filter_plan_mismatch :
    ∃ children, (⟨true, false⟩ : Selection).keeps children ≠
      ((⟨true, false⟩ : Selection).shows children).any (·.passes) :=
  ⟨[⟨false, true⟩], by decide⟩ -- one unreadable child that passes: kept, yet nothing shown passes

-- The gated filter gives the same answer for children the reader cannot tell
-- apart, so it reveals nothing about an unreadable child.
theorem gated_ignores_unreadable {as bs : List Child} (h : Same as bs) :
    gatedKeeps as = gatedKeeps bs := by
  simp only [gatedKeeps, keepsFrom, yields_gated] -- the gated join reads the readable children
  induction h with                                -- one child at a time
  | nil => rfl                                    -- no children: same answer
  | readable c hc _ ih =>                         -- the same readable child:
    simp only [List.filter_cons, hc, if_true, List.any_cons] -- it is kept on both sides,
    rw [ih]                                       --   and the rest agree
  | hidden c d hc hd _ ih =>                      -- two unreadable children:
    simp only [List.filter_cons, hc, hd, Bool.false_eq_true, if_false] -- both left out,
    exact ih                                      --   and the rest agree

-- Without the gate, two lists the reader cannot tell apart can get different
-- answers: the join leaks whether an unreadable child passes.
theorem ungated_reveals_unreadable :
    ∃ as bs, Same as bs ∧ ungatedKeeps as ≠ ungatedKeeps bs :=
  ⟨[⟨false, true⟩], [⟨false, false⟩],             -- one unreadable child that passes, one that does not
    Same.hidden _ _ rfl rfl Same.nil,             -- the reader cannot tell them apart
    by decide⟩                                    -- yet the ungated answers differ

-- When the reader may read every child (or nothing gates the collection: no
-- policy, no app read validator, not a `@chunk` collection), the gate changes nothing.
theorem all_readable_unchanged (children : List Child)
    (h : ∀ c ∈ children, c.readable = true) :
    gatedKeeps children = ungatedKeeps children := by
  simp only [gatedKeeps, ungatedKeeps, keepsFrom, yields_gated, yields_ungated] -- readable children against every child
  rw [List.filter_eq_self.mpr h]                  -- every child is readable, so the gate leaves none out

end RelationFilterAccess
