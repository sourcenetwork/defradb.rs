/-!
The story, in plain words. A materialized view keeps its items in a cache.
The node fills the cache by running the view's own query, and that query
needs a field list. The view declares its fields; its source query names
fields too, but a lens on the view can add fields the source never names.
So the list is built from the view's fields: every plain field the view
declares, and every relation field the source query gives an inner selection
for, with that inner selection. Fields whose names start with `_` are left
out, as the node's own.

What must never happen: a plain field the view declares is missing from the
list (its value would be lost from every cached item), or a field the view
does not declare is asked for.

Rust owner: crates/database/db/src/view/selection.rs `cache_selection`, used by
crates/database/db/src/view/ops.rs `DB::build_view_cache`.
-/
namespace ViewCacheSelection

-- A field the view declares: its name and whether it is a relation.
structure Field where
  name : String
  relation : Bool

-- One entry of the field list: a name and, for a relation, its inner list.
structure Entry where
  name : String
  inner : Option String

-- Whether a field is one of the node's own (its name starts with `_`).
def internal (f : Field) : Bool :=
  f.name.startsWith "_"

-- The entry for one view field: a plain field is selected by name; a relation
-- field only when the source gives it an inner selection (`innerOf`).
def entry (innerOf : String → Option String) (f : Field) : Option Entry :=
  if f.relation then (innerOf f.name).map (fun i => { name := f.name, inner := some i })
  else some { name := f.name, inner := none }

-- The field list: the entries of the view's fields that are not the node's own.
def selection (innerOf : String → Option String) (fields : List Field) : List Entry :=
  (fields.filter (fun f => !internal f)).filterMap (entry innerOf)

-- Every plain field the view declares, other than the node's own, is selected,
-- whatever the source query names.
theorem plain_field_selected (innerOf : String → Option String) (fields : List Field)
    (f : Field) (declared : f ∈ fields) (plain : f.relation = false)
    (own : internal f = false) :
    { name := f.name, inner := none : Entry } ∈ selection innerOf fields := by
  -- The field passes the filter, and its entry is the plain one.
  refine List.mem_filterMap.mpr ⟨f, List.mem_filter.mpr ⟨declared, by simp [own]⟩, ?_⟩
  -- A plain field's entry is its name with no inner list.
  simp [entry, plain]

-- Every selected entry names a field the view declares.
theorem selected_is_declared (innerOf : String → Option String) (fields : List Field)
    (e : Entry) (chosen : e ∈ selection innerOf fields) :
    ∃ f ∈ fields, f.name = e.name := by
  -- Unpack the filterMap: e came from some field that passed the filter.
  obtain ⟨f, kept, made⟩ := List.mem_filterMap.mp chosen
  -- That field is in the view's list.
  refine ⟨f, (List.mem_filter.mp kept).1, ?_⟩
  -- Both kinds of entry carry the field's own name.
  unfold entry at made
  split at made
  · -- A relation entry: the name is the field's.
    cases h : innerOf f.name <;> simp [h] at made
    exact made ▸ rfl
  · -- A plain entry: the name is the field's.
    cases made
    rfl

end ViewCacheSelection
