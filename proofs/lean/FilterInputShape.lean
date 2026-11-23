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
`validate_operation` sees: selects, relation selections,
`_commits`, mutation filters, and aggregate selectors parsed by
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

end FilterInputShape
