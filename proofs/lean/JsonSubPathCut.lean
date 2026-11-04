/-!
# JsonSubPathCut: a filter path stops at the first field that is not a relation

The story, in plain words. A query filter can reach through relations:
`{author: {published: {rating: {_eq: 4.9}}}}` walks from a book to its author
to the author's books. The planner builds one join for each relation on that
walk. Looking only at the filter's shape, a JSON field looks the same:
`{jsonField: {i: {love: {_like: "%family%"}}}}` seems to walk through
`jsonField` and `i`, but `jsonField` is a JSON value, not a relation, and
there is nothing to join. So the planner now reads each path against the
schema and cuts it at the first field that is not a relation; the rest of the
path is a plain condition on that field's value.

Who acts: the planner, holding a schema (what each field of each collection
is) and the paths the filter's shape suggests.

What can happen at each step of a path:
- the field is a relation whose target collection is found: keep it, step in;
- the field is not a relation (JSON, a string, ...): stop, keep what came before;
- the field is unknown, or its target is not found: keep the whole path, so the
  join code reports the error exactly as it did before.

What must never happen:
- a cut path that still steps through a non-relation (`longest_allRel`), or one
  shorter than it needs to be (`longest_max`);
- cutting twice giving something different from cutting once (`cut_idem`);
- a path that is all relations changing at all (`cut_of_allRel`,
  `relationPaths_of_allRel`), so every relation filter plans exactly as before.

The second half is the leaf rule (#2430): a sub-selection such as
`jsonField { i }` on a field that is not a relation is an error, the way
graphql-go's ScalarLeafs rule rejects it. Validation now rejects exactly those
selections (`newValidate_false_iff`). Before, the planner's selection join
rejected the same selections later, with other words, so the set of queries
that run is the same (`newRun_eq_mainRun`).

Rust anchors:
- `Planner::relation_filter_paths` and `Planner::relation_prefix`
  (`crates/query/src/planner/joins/relation_paths.rs`);
- `validate_select` (`crates/query/src/runner/plan_validation.rs`), and the
  same check in `Planner::apply_selection_relation_join`
  (`crates/query/src/planner/joins/selection_join.rs`).
-/

namespace JsonSubPathCut

-- What one field of a collection is, as a walk along a filter path sees it.
-- `C` names collections.
inductive Step (C : Type) where
  -- a relation whose target collection resolves to `next`
  | rel (next : C)
  -- a field that is not a relation: JSON, a string, a number, ...
  | leaf
  -- a field the collection does not have, or a relation whose target is missing
  | stuck

-- A schema says, for each collection and field name, what that field is.
-- `S` names fields (the segments of a path).
abbrev Schema (C S : Type) := C → S → Step C

-- Every definition below works for any collections `C` and field names `S`.
variable {C S : Type}

-- `relation_prefix`: walk the path from collection `c` and cut it.
def cut (σ : Schema C S) : C → List S → List S
  -- an empty path stays empty
  | _, [] => []
  -- look at the first field on the current collection
  | c, s :: rest =>
    -- what is this field?
    match σ c s with
    -- a relation: keep the field and walk on in its target collection
    | .rel n => s :: cut σ n rest
    -- not a relation: stop here, keep only what came before
    | .leaf => []
    -- unknown: keep the whole rest, so the join reports the error as before
    | .stuck => s :: rest

-- `true` when every field on the path is a relation that resolves.
def allRel (σ : Schema C S) : C → List S → Bool
  -- the empty path has no field that is not a relation
  | _, [] => true
  -- the first field must be a relation, and so must the rest, in its target
  | c, s :: rest =>
    -- what is the first field?
    match σ c s with
    -- a relation: check the rest from its target
    | .rel n => allRel σ n rest
    -- anything else breaks the chain
    | _ => false

-- The longest beginning of the path made only of relations that resolve.
def longest (σ : Schema C S) : C → List S → List S
  -- nothing to take from an empty path
  | _, [] => []
  -- take relations while they last
  | c, s :: rest =>
    -- what is the first field?
    match σ c s with
    -- a relation: take it and keep going in its target
    | .rel n => s :: longest σ n rest
    -- anything else ends the run of relations
    | _ => []

-- `true` when the walk meets no unknown field before it stops.
def resolved (σ : Schema C S) : C → List S → Bool
  -- walking off the end met nothing unknown
  | _, [] => true
  -- look at the first field
  | c, s :: rest =>
    -- what is it?
    match σ c s with
    -- a relation: the rest must resolve too
    | .rel n => resolved σ n rest
    -- a non-relation ends the walk, with nothing unknown on it
    | .leaf => true
    -- an unknown field
    | .stuck => false

-- Theorem: the cut keeps a beginning of the path, never anything new.
theorem cut_prefix (σ : Schema C S) :
    ∀ (c : C) (p : List S), ∃ t, cut σ c p ++ t = p := by
  -- go along the path, from any collection
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty path: the cut is empty, the tail is empty
  | nil => exact ⟨[], rfl⟩
  -- a first field `s` and a rest
  | cons s rest ih =>
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: the rest's own tail works, after `s`
    | rel n =>
      -- the rest of the path has a tail `t` after its cut
      obtain ⟨t, ht⟩ := ih n
      -- the same tail follows `s` and the rest's cut
      exact ⟨t, by simp [cut, h, ht]⟩
    -- not a relation: the cut is empty, the whole path is the tail
    | leaf => exact ⟨s :: rest, by simp [cut, h]⟩
    -- unknown: the cut is the whole path, nothing is left over
    | stuck => exact ⟨[], by simp [cut, h]⟩

-- Theorem: when nothing on the walk is unknown, the cut is exactly the longest
-- run of relations at the start of the path.
theorem cut_eq_longest (σ : Schema C S) :
    ∀ (c : C) (p : List S), resolved σ c p = true → cut σ c p = longest σ c p := by
  -- any collection, any path
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty path: both are empty
  | nil => intro _; rfl
  -- a first field and a rest
  | cons s rest ih =>
    -- assume the walk resolves
    intro hr
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: both keep `s`, and the rest matches by induction
    | rel n =>
      -- the rest resolves too
      have hr' : resolved σ n rest = true := by simpa [resolved, h] using hr
      -- keep `s` on both sides, the rest agrees
      simp [cut, longest, h, ih n hr']
    -- not a relation: both stop with nothing
    | leaf => simp [cut, longest, h]
    -- unknown: impossible, the walk was assumed to resolve
    | stuck => simp [resolved, h] at hr

-- Theorem: the longest run of relations is made only of relations.
theorem longest_allRel (σ : Schema C S) :
    ∀ (c : C) (p : List S), allRel σ c (longest σ c p) = true := by
  -- any collection, any path
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty run is trivially all relations
  | nil => rfl
  -- a first field and a rest
  | cons s rest ih =>
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: `s` is one, and the rest of the run is by induction
    | rel n => simp [longest, allRel, h, ih n]
    -- not a relation: the run is empty
    | leaf => simp [longest, allRel, h]
    -- unknown: the run is empty
    | stuck => simp [longest, allRel, h]

-- Theorem: any beginning of the path that is all relations fits inside the
-- longest run, so no shorter cut would do.
theorem longest_max (σ : Schema C S) :
    ∀ (c : C) (q p : List S), allRel σ c q = true → (∃ t, q ++ t = p) →
      ∃ u, q ++ u = longest σ c p := by
  -- any collection and a candidate beginning `q`
  intro c q
  -- induction on the candidate, for every collection and path
  induction q generalizing c with
  -- the empty candidate fits in front of anything
  | nil => intro p _ _; exact ⟨longest σ c p, rfl⟩
  -- a candidate with a first field `s`
  | cons s q' ih =>
    -- the path, the candidate being all relations, and the candidate starting it
    intro p hq ⟨t, ht⟩
    -- the path starts with `s` and continues with `q' ++ t`
    subst ht
    -- split on what `s` is
    cases h : σ c s with
    -- a relation: the rest of the candidate fits the rest's run, by induction
    | rel n =>
      -- the rest of the candidate is all relations in the target
      have hq' : allRel σ n q' = true := by simpa [allRel, h] using hq
      -- so it fits inside the target's longest run
      obtain ⟨u, hu⟩ := ih n (q' ++ t) hq' ⟨t, rfl⟩
      -- put `s` back in front on both sides
      exact ⟨u, by simp [longest, h, hu]⟩
    -- not a relation: impossible, the candidate was all relations
    | leaf => simp [allRel, h] at hq
    -- unknown: impossible, the candidate was all relations
    | stuck => simp [allRel, h] at hq

-- Theorem: a path that is all relations is not cut at all.
theorem cut_of_allRel (σ : Schema C S) :
    ∀ (c : C) (p : List S), allRel σ c p = true → cut σ c p = p := by
  -- any collection, any path
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty path stays empty
  | nil => intro _; rfl
  -- a first field and a rest
  | cons s rest ih =>
    -- assume it is all relations
    intro ha
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: keep `s`, and the rest stays by induction
    | rel n =>
      -- the rest is all relations in the target
      have ha' : allRel σ n rest = true := by simpa [allRel, h] using ha
      -- so nothing changes
      simp [cut, h, ih n ha']
    -- not a relation: impossible on an all-relation path
    | leaf => simp [allRel, h] at ha
    -- unknown: impossible on an all-relation path
    | stuck => simp [allRel, h] at ha

-- Theorem: a walk that meets an unknown field keeps the whole path, so the join
-- code sees it and reports the error as it did before.
theorem cut_of_unresolved (σ : Schema C S) :
    ∀ (c : C) (p : List S), resolved σ c p = false → cut σ c p = p := by
  -- any collection, any path
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty path resolves, so this case cannot happen
  | nil => intro h; simp [resolved] at h
  -- a first field and a rest
  | cons s rest ih =>
    -- assume the walk meets an unknown field
    intro hr
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: the unknown field is further on, so the rest is kept whole
    | rel n =>
      -- the rest does not resolve
      have hr' : resolved σ n rest = false := by simpa [resolved, h] using hr
      -- keep `s` and the whole rest
      simp [cut, h, ih n hr']
    -- not a relation: impossible, that walk would have resolved
    | leaf => simp [resolved, h] at hr
    -- unknown: the whole path is kept right here
    | stuck => simp [cut, h]

-- Theorem: cutting a cut path changes nothing.
theorem cut_idem (σ : Schema C S) :
    ∀ (c : C) (p : List S), cut σ c (cut σ c p) = cut σ c p := by
  -- any collection, any path
  intro c p
  -- induction on the path, for every starting collection
  induction p generalizing c with
  -- the empty path
  | nil => rfl
  -- a first field and a rest
  | cons s rest ih =>
    -- split on what the first field is
    cases h : σ c s with
    -- a relation: `s` stays, and the rest is idempotent by induction
    | rel n => simp [cut, h, ih n]
    -- not a relation: the cut is empty, and the empty path cuts to itself
    | leaf => simp [cut, h]
    -- unknown: the path is kept whole both times
    | stuck => simp [cut, h]

-- `relation_filter_paths`: cut every path the filter's shape suggests and keep
-- those still longer than one field (a single relation needs no chain of joins).
def relationPaths (σ : Schema C S) (c : C) : List (List S) → List (List S)
  -- no paths in, no paths out
  | [] => []
  -- cut the first path, keep it if it still has two fields or more
  | p :: ps =>
    -- the first path, cut
    if 1 < (cut σ c p).length then cut σ c p :: relationPaths σ c ps
    -- too short: drop it
    else relationPaths σ c ps

-- Theorem: when every suggested path is all relations and longer than one field
-- (what `Filter::get_multi_level_relation_paths` returns for relation filters),
-- the planner gets exactly the paths it got before this change.
theorem relationPaths_of_allRel (σ : Schema C S) (c : C) :
    ∀ ps : List (List S), (∀ p ∈ ps, allRel σ c p = true ∧ 1 < p.length) →
      relationPaths σ c ps = ps := by
  -- any list of paths
  intro ps
  -- induction on the list of paths
  induction ps with
  -- no paths
  | nil => intro _; rfl
  -- a first path and the others
  | cons p ps ih =>
    -- every path is all relations and long enough
    intro h
    -- in particular the first one
    have hp := h p (by simp)
    -- and the others
    have hps : ∀ q ∈ ps, allRel σ c q = true ∧ 1 < q.length :=
      fun q hq => h q (by simp [hq])
    -- the first is kept uncut, the others by induction
    simp [relationPaths, cut_of_allRel σ c p hp.1, hp.2, ih hps]

-- The two issue examples, on a tiny schema.
-- Collections: the JSON blob of #2427, and books with authors.
inductive Coll where
  -- `type jsonBlob { jsonField: JSON }`
  | jsonBlob
  -- `type Book { author: Author }`
  | book
  -- `type Author { published: [Book] meta: JSON }`
  | author
  deriving DecidableEq

-- The field names the examples use.
inductive Seg where
  -- the JSON field of #2427
  | jsonField
  -- keys inside the JSON value
  | i
  -- a key inside `i`
  | love
  -- book to author
  | authorF
  -- author to books
  | published
  -- a JSON field on the author
  | meta
  -- a key inside `meta`
  | a
  deriving DecidableEq

-- The schema: what each field is on each collection.
def demo : Schema Coll Seg
  -- the blob's JSON field is not a relation
  | .jsonBlob, .jsonField => .leaf
  -- a book's author is a relation to authors
  | .book, .authorF => .rel .author
  -- an author's books are a relation to books
  | .author, .published => .rel .book
  -- an author's `meta` is JSON
  | .author, .meta => .leaf
  -- anything else is unknown
  | _, _ => .stuck

-- #2427: the JSON sub-path asks for no join at all.
example : relationPaths demo .jsonBlob [[.jsonField, .i]] = [] := by decide

-- A relation chain is kept as it was.
example : relationPaths demo .book [[.authorF, .published]] = [[.authorF, .published]] := by
  decide

-- A JSON field behind a relation leaves one relation, a single-level filter.
example : relationPaths demo .book [[.authorF, .meta, .a]] = [] := by decide

/-! ## The leaf rule (#2430) -/

-- One requested item of a selection set.
inductive Item (S : Type) where
  -- `name { ... }`: a field with a sub-selection (`Requestable::Select`)
  | sub (name : S)
  -- a plain field, an aggregate, a similarity, ... (every other `Requestable`)
  | other

-- What `collection.field_by_name` says about a name: `none` when the collection
-- has no such field, `some true` for a relation, `some false` for anything else.
abbrev Kinds (S : Type) := S → Option Bool

-- The new loop in `validate_select`: no sub-selection on a known non-relation.
def leafOk (kind : Kinds S) : List (Item S) → Bool
  -- an empty selection breaks no rule
  | [] => true
  -- a sub-selection: its field must not be a known non-relation
  | .sub n :: rest => (kind n != some false) && leafOk kind rest
  -- anything else is not this rule's business
  | .other :: rest => leafOk kind rest

-- Theorem: the leaf rule fails exactly when some known non-relation field
-- carries a sub-selection.
theorem leafOk_false_iff (kind : Kinds S) :
    ∀ sel : List (Item S),
      leafOk kind sel = false ↔ ∃ n, Item.sub n ∈ sel ∧ kind n = some false := by
  -- any selection
  intro sel
  -- induction on the selection's items
  induction sel with
  -- empty: the rule holds and there is no offending item
  | nil => simp [leafOk]
  -- a first item and the rest
  | cons it rest ih =>
    -- split on the kind of item
    cases it with
    -- a sub-selection on `n`
    | sub n =>
      -- either `n` is the offender, or the offender is in the rest
      cases hk : kind n with
      -- unknown field: not an offender, look in the rest
      | none =>
        -- this item passes; an offender is in the list exactly when it is in
        -- the rest, since `n` itself is no offender
        simp [leafOk, hk, ih]
      -- a known field: a relation passes, a non-relation fails
      | some b =>
        -- split on relation or not
        cases b with
        -- not a relation: this item is the offender
        | false => simp [leafOk, hk]
        -- a relation: this item passes, the rest decides
        | true =>
          -- this item passes; an offender is in the list exactly when it is in
          -- the rest, since `n` is a relation
          simp [leafOk, hk, ih]
    -- any other item passes, the rest decides
    | other => simp [leafOk, ih]

-- `validate_select` now: the leaf rule, and every check that was there before
-- (`main`, whatever those checks decide).
def newValidate (kind : Kinds S) (main : List (Item S) → Bool) (sel : List (Item S)) : Bool :=
  -- both must accept
  leafOk kind sel && main sel

-- Theorem: validation rejects a selection exactly when the old checks reject it
-- or a non-relation field carries a sub-selection.
theorem newValidate_false_iff (kind : Kinds S) (main : List (Item S) → Bool)
    (sel : List (Item S)) :
    newValidate kind main sel = false ↔
      main sel = false ∨ ∃ n, Item.sub n ∈ sel ∧ kind n = some false := by
  -- the leaf rule fails exactly when an offender exists
  have key := leafOk_false_iff kind sel
  -- a conjunction of booleans is false when either side is
  cases h1 : leafOk kind sel <;> cases h2 : main sel <;> simp_all [newValidate]

-- Theorem: every selection the old checks accepted and that breaks no leaf rule
-- is still accepted.
theorem newValidate_of_main (kind : Kinds S) (main : List (Item S) → Bool)
    (sel : List (Item S)) (hm : main sel = true)
    (hl : ¬ ∃ n, Item.sub n ∈ sel ∧ kind n = some false) :
    newValidate kind main sel = true := by
  -- the leaf rule holds, since no offender exists
  have hl' : leafOk kind sel = true := by
    -- a boolean that is not false is true
    cases h : leafOk kind sel with
    -- false would give an offender
    | false => exact absurd ((leafOk_false_iff kind sel).1 h) hl
    -- true is what we want
    | true => rfl
  -- both sides of the conjunction hold
  simp [newValidate, hl', hm]

-- How a query ended on main: validation (`main`), then the planner's selection
-- join, which failed on the same sub-selections (`apply_selection_relation_join`,
-- "is not a relation") before it could run.
def mainRun (kind : Kinds S) (main : List (Item S) → Bool) (sel : List (Item S)) : Bool :=
  -- validation, then the planner's own leaf check
  main sel && leafOk kind sel

-- How a query ends now: the new validation, then the same planner check, which
-- now raises the Go error text.
def newRun (kind : Kinds S) (main : List (Item S) → Bool) (sel : List (Item S)) : Bool :=
  -- new validation, then the planner's leaf check
  newValidate kind main sel && leafOk kind sel

-- Theorem: exactly the same selections run as on main; only the error text of
-- the rejected ones changes.
theorem newRun_eq_mainRun (kind : Kinds S) (main : List (Item S) → Bool)
    (sel : List (Item S)) : newRun kind main sel = mainRun kind main sel := by
  -- every combination of the two booleans agrees
  cases h1 : leafOk kind sel <;> cases h2 : main sel <;> simp [newRun, mainRun, newValidate, h1, h2]

end JsonSubPathCut
