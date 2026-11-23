/-!
# Filter input validation skips variable positions

A request writes a filter as a literal, and may put a variable anywhere
inside it. graphql-go checks the literal's keys against the filter's input
type, treating every variable as valid: the variable's value is checked
later, by coercion. So the literal's errors must never depend on what a
variable holds. In the Rust code the filter's conditions already hold each
variable's value, and the walk skips the positions the parser recorded.

What must never happen: a variable's value adds or hides a literal error.

This models `walk::literal` in
`crates/engine/query/src/query_parse/filter_shape/walk.rs`, which
`filter_shape::check` (in `filter_shape/mod.rs`) runs on every filter
`validate_operation` sees: selects, relation selections and `GROUP`,
`_commits` and its `links`, mutation filters, and aggregate selectors parsed by
`parse_aggregate_target_obj` and `parse_aggregate_target_from_json` in
`query_parse/aggregates.rs`. `truncate_` is checked before
`Mutation::truncate_filter` folds its `docID` list in.
-/
namespace FilterInputShape

-- A filter value, with an object written as a list of fields.
inductive Value where
  -- A variable's position, holding the value it was given.
  | filled : String → Value → Value
  -- A plain value with no keys to check (a number, a string).
  | scalar : Value
  -- The end of an object's fields.
  | done : Value
  -- One field (its key and value), then the object's other fields.
  | field : String → Value → Value → Value

-- The literal check: every key the input type at `path` does not define is
-- an error at that key; a variable position is never looked into.
def walk (defined : List String → String → Bool) (path : List String) : Value → List (List String)
  -- A variable is assumed valid here.
  | .filled _ _ => []
  -- A plain value has no keys.
  | .scalar => []
  -- No fields left.
  | .done => []
  -- This key's error (if undefined), its value's errors, then the others'.
  | .field k v rest =>
      (if defined path k then [] else [path ++ [k]])
        ++ walk defined (path ++ [k]) v ++ walk defined path rest

-- Forget every variable's value, keeping only where the variable was.
def erase : Value → Value
  -- The position stays; the value is replaced by nothing to check.
  | .filled n _ => .filled n .done
  -- Plain values stay.
  | .scalar => .scalar
  -- The end stays.
  | .done => .done
  -- Fields are erased one by one.
  | .field k v rest => .field k (erase v) (erase rest)

-- The literal's errors are the same whatever its variables hold.
theorem literal_walk_skips_variable_positions
    (defined : List String → String → Bool) (path : List String) (v : Value) :
    walk defined path v = walk defined path (erase v) := by
  -- By induction on the value, at every path.
  induction v generalizing path with
  -- A variable: both sides are empty.
  | filled n x _ => rfl
  -- A plain value: both sides are empty.
  | scalar => rfl
  -- The end: both sides are empty.
  | done => rfl
  -- A field: its own error is unchanged, and both parts follow by induction.
  | field k x rest ihx ihrest =>
      simp only [walk, erase]
      rw [ihx, ihrest]

-- Two fillings of the same variable give the same literal errors.
theorem variable_value_never_matters
    (defined : List String → String → Bool) (path : List String) (n : String) (x y : Value) :
    walk defined path (.filled n x) = walk defined path (.filled n y) := by
  -- Neither side looks inside the variable.
  rfl

/-!
## A variable that fills a whole list is still checked

After the literal check, coercion checks each variable's value against the
input type at its position. `walk::shape_at` finds that type by following the
variable's path from the filter's root, and `filter_shape::check` skips the
variable when `shape_at` finds nothing. A variable can stand for a whole
`_and` or `_or` list (`_and: $c`): its path ends on a key whose value is a
list. If `shape_at` dropped that path, a bad list would never be checked.

What must never happen: a variable at a list key is skipped. This models
`shape_at` and `variable_list` in
`crates/engine/query/src/query_parse/filter_shape/walk.rs`. For the model, the
type a key leads to depends only on the key, which is all the theorem needs.
-/

-- One step down a filter path: a key of an object, or an element of a list.
inductive Step where
  -- An object key.
  | key : String → Step
  -- A list element, by its place.
  | index : Nat → Step

-- Where a key leads: nowhere (the type lacks it), one value, or a list.
inductive Lead where
  -- The type does not define the key.
  | unknown
  -- The key holds one value.
  | one
  -- The key holds a list of values.
  | listOf

-- Follows `path`, `inList` saying whether the last key led to a list. The
-- answer is whether the path ends on a list, or `none` when it leaves the
-- input types.
def shapeAt (lead : String → Lead) : Bool → List Step → Option Bool
  -- The path is used up: say whether it ended on a list.
  | inList, [] => some inList
  -- An element of the list the last key led to.
  | true, .index _ :: rest => shapeAt lead false rest
  -- An element where there is no list: outside the input types.
  | false, .index _ :: _ => none
  -- A key: follow where it leads.
  | _, .key k :: rest =>
      match lead k with
      -- An undefined key: outside the input types.
      | .unknown => none
      -- One value: carry on.
      | .one => shapeAt lead false rest
      -- A list: the next step may be one of its elements.
      | .listOf => shapeAt lead true rest

-- A path `shapeAt` follows, extended by a key that leads to a list, is still
-- followed, and ends on a list: the variable there is checked as a list.
theorem whole_list_variable_is_checked
    (lead : String → Lead) (k : String) (hk : lead k = .listOf) :
    ∀ (path : List Step) (b : Bool) (x : Bool),
      shapeAt lead b path = some x → shapeAt lead b (path ++ [.key k]) = some true := by
  -- By induction on the path, from any starting list flag.
  intro path
  induction path with
  -- An empty path: the one key leads to a list.
  | nil =>
      intro b x _
      cases b <;> simp [shapeAt, hk]
  -- A step, then the rest of the path.
  | cons s rest ih =>
      intro b x h
      cases s with
      -- A key: whichever way it leads, the rest follows by induction.
      | key k' =>
          cases hl : lead k' with
          -- An undefined key cannot be followed, so the premise is false.
          | unknown => cases b <;> simp [shapeAt, hl] at h
          -- One value: the rest starts outside a list.
          | one =>
              cases b <;> simp [shapeAt, hl] at h ⊢ <;> exact ih false x h
          -- A list: the rest starts inside it.
          | listOf =>
              cases b <;> simp [shapeAt, hl] at h ⊢ <;> exact ih true x h
      -- An element: only inside a list, and the rest follows by induction.
      | index i =>
          cases b with
          -- Outside a list the premise is false.
          | false => simp [shapeAt] at h
          -- Inside a list the rest starts outside one.
          | true =>
              simp [shapeAt] at h ⊢
              exact ih false x h

end FilterInputShape
