/-!
# A collection added on its own has the identity it has in a batch

A collection's ID is a hash over its name and its fields. A relation field
commits to the ID of the collection it points to. Someone adds collections
with SDL, in one call or in several: `type Author {…}` and
`type Book { author: Author }` together, or `Author` first and `Book` in a
later call, when `Author` is already stored.

Who acts: a caller adding collections, one call after another. What can
happen: a relation names a type declared earlier in the same call, a type the
call declares further on, a type the store already holds, or none of these. What must never happen: the same
declarations give a collection another ID because they arrived in more calls,
or a relation field reaches a mutation input as an object type.

This models `SdlParser::resolve_field_kind` in
`crates/engine/query/src/sdl_parse/builder_field_kinds.rs`, the first pass of
`SdlParser::build_collections` in `crates/engine/query/src/sdl_parse/builder.rs`
(which assigns IDs in dependency order), `DB::add_collection` and
`DB::known_collections` in `crates/database/db/src/collection/ops/add/mod.rs`
(which hand the stored collections' IDs to the parser), and
`field_kind_to_input_type_ref` in
`crates/engine/query/src/runner/introspection/input_types.rs`.

The model leaves out Go's head-store prefix scan, which the parser simulates
for types of one call whose names share a prefix: that depends on the call's
whole composition and is covered by `defra.collection.deterministic_id`.
-/
namespace RelationTargetIdentity

-- What a field's kind is once the parser has resolved it.
inductive Kind where
  -- A scalar, by its kind number.
  | scalar : Nat → Kind
  -- A relation that commits to its target collection's ID.
  | relation : Nat → Kind
  -- A relation left with only its target's name: it commits to no ID.
  | named : String → Kind
  deriving DecidableEq

-- What a field declares in SDL.
inductive Decl where
  -- A scalar field, by its kind number.
  | scalar : Nat → Decl
  -- A relation to the type with this name.
  | to : String → Decl

-- The collection IDs a party knows, by type name.
abbrev Known := String → Option Nat

-- Nothing known.
def nothing : Known :=
  -- Every name is unknown.
  fun _ => none

-- What is known from `first`, and from `second` for every name `first` lacks.
def over (first second : Known) : Known :=
  -- Look in `first`, then in `second`.
  fun name => match first name with
    -- `first` knows the name.
    | some id => some id
    -- Only `second` may know it.
    | none => second name

-- `known`, plus one more collection.
def extend (known : Known) (name : String) (id : Nat) : Known :=
  -- The new name answers with its ID; every other name is unchanged.
  fun other => if other = name then some id else known other

-- What a call sees of a name: the ID it gave the type if it has processed
-- it, nothing if the call declares the type and has not processed it yet,
-- and otherwise what the store holds. `declared` says which names the call's
-- SDL declares; the middle case is the guard `!self.type_defs.contains_key`
-- in `resolve_field_kind`, which keeps a type of the SDL from being taken
-- for a stored collection of the same name.
def seen (declared : String → Bool) (batch stored : Known) : Known :=
  -- Look in the call first.
  fun name => match batch name with
    -- The call has processed the type.
    | some id => some id
    -- Not processed: a declared name is not looked up in the store.
    | none => if declared name then none else stored name

-- The parser's resolution (`resolve_field_kind`): a relation commits to its
-- target's ID when the call sees one for it.
def resolve (declared : String → Bool) (batch stored : Known) : Decl → Kind
  -- A scalar stays a scalar.
  | .scalar kind => .scalar kind
  -- A relation looks its target up as the call sees it.
  | .to name => match seen declared batch stored name with
    -- A known target: the relation commits to its ID.
    | some id => .relation id
    -- An unknown target: only the name is left.
    | none => .named name

-- A call that declares no name.
def undeclared : String → Bool :=
  -- No name is declared.
  fun _ => false

-- The resolution before the fix: a stored target was never looked up.
def resolveOld (batch : Known) : Decl → Kind :=
  -- The same resolution with an empty store.
  resolve undeclared batch nothing

-- A declared collection: its name and its fields, each a name and a declaration.
structure Coll where
  -- The type's name.
  name : String
  -- The type's fields.
  fields : List (String × Decl)

-- A collection's ID: a hash (any function) of its name and resolved fields.
def idOf (hash : String → List (String × Kind) → Nat) (declared : String → Bool)
    (batch stored : Known) (c : Coll) : Nat :=
  -- Resolve every field, then hash.
  hash c.name (c.fields.map fun field => (field.1, resolve declared batch stored field.2))

-- One `add_collection` call: the declarations are taken in dependency order,
-- each gets its ID, and the next one sees it (`all_collection_ids` in the
-- first pass of `build_collections`). The result is the IDs given, in order.
def ids (hash : String → List (String × Kind) → Nat) (declared : String → Bool)
    (stored : Known) : Known → List Coll → List Nat
  -- Nothing left to add.
  | _, [] => []
  -- Give this collection its ID, then go on with it known to the call.
  | batch, c :: rest =>
      idOf hash declared batch stored c
        :: ids hash declared stored
            (extend batch c.name (idOf hash declared batch stored c)) rest

-- What the call knows once it has added `cs`.
def after (hash : String → List (String × Kind) → Nat) (declared : String → Bool)
    (stored : Known) : Known → List Coll → Known
  -- Nothing was added.
  | batch, [] => batch
  -- Add this collection, then the others.
  | batch, c :: rest =>
      after hash declared stored
        (extend batch c.name (idOf hash declared batch stored c)) rest

-- A field's kind depends only on what the call sees, not on whether an ID
-- was learned in this call or read from the store.
theorem resolve_depends_on_what_is_seen
    (declared₁ declared₂ : String → Bool) (batch₁ stored₁ batch₂ stored₂ : Known)
    (same : seen declared₁ batch₁ stored₁ = seen declared₂ batch₂ stored₂) (d : Decl) :
    resolve declared₁ batch₁ stored₁ d = resolve declared₂ batch₂ stored₂ d := by
  -- A scalar ignores what is seen; a relation reads only that.
  cases d with
  -- Both sides are the scalar.
  | scalar kind => rfl
  -- Both sides read the same lookup.
  | to name => simp only [resolve, same]

-- So a collection's ID depends only on what the call sees.
theorem id_depends_on_what_is_seen
    (hash : String → List (String × Kind) → Nat)
    (declared₁ declared₂ : String → Bool) (batch₁ stored₁ batch₂ stored₂ : Known)
    (same : seen declared₁ batch₁ stored₁ = seen declared₂ batch₂ stored₂) (c : Coll) :
    idOf hash declared₁ batch₁ stored₁ c = idOf hash declared₂ batch₂ stored₂ c := by
  -- Unfold the ID to the hash of the resolved fields.
  unfold idOf
  -- Every field resolves the same on both sides.
  have fields :
      (c.fields.map fun field => (field.1, resolve declared₁ batch₁ stored₁ field.2))
        = (c.fields.map fun field => (field.1, resolve declared₂ batch₂ stored₂ field.2)) := by
    -- Field by field, by the theorem above.
    apply List.map_congr_left
    -- One field.
    intro field _
    -- Its kind is the same on both sides.
    rw [resolve_depends_on_what_is_seen declared₁ declared₂ batch₁ stored₁ batch₂ stored₂ same]
  -- The hashes of equal inputs are equal.
  rw [fields]

-- Processing one more collection keeps two equal views equal.
theorem extend_keeps_views_equal
    (declared₁ declared₂ : String → Bool) (batch₁ stored₁ batch₂ stored₂ : Known)
    (same : seen declared₁ batch₁ stored₁ = seen declared₂ batch₂ stored₂)
    (name : String) (id : Nat) :
    seen declared₁ (extend batch₁ name id) stored₁
      = seen declared₂ (extend batch₂ name id) stored₂ := by
  -- Compare the two views name by name.
  funext other
  -- What the two views said about this name before.
  have before : seen declared₁ batch₁ stored₁ other = seen declared₂ batch₂ stored₂ other := by
    -- They were equal for every name.
    rw [same]
  -- The new name answers with its ID on both sides; any other name is as before.
  by_cases new : other = name
  -- The new name.
  · simp [seen, extend, new]
  -- Any other name.
  · simpa [seen, extend, new] using before

-- Two calls that see the same at the start give the same IDs.
theorem ids_depend_on_what_is_seen
    (hash : String → List (String × Kind) → Nat) (declared₁ declared₂ : String → Bool)
    (cs : List Coll) :
    ∀ (batch₁ stored₁ batch₂ stored₂ : Known),
      seen declared₁ batch₁ stored₁ = seen declared₂ batch₂ stored₂ →
      ids hash declared₁ stored₁ batch₁ cs = ids hash declared₂ stored₂ batch₂ cs := by
  -- By induction on the declarations left to add.
  induction cs with
  -- Nothing left: both give no IDs.
  | nil => intro _ _ _ _ _; rfl
  -- One declaration, then the rest.
  | cons c rest ih =>
      -- Name the two starting views.
      intro batch₁ stored₁ batch₂ stored₂ same
      -- This collection gets the same ID on both sides.
      have head :=
        id_depends_on_what_is_seen hash declared₁ declared₂ batch₁ stored₁ batch₂ stored₂ same c
      -- Unfold one step of both calls.
      simp only [ids]
      -- Use the equal ID everywhere.
      rw [head]
      -- The rest follows by induction, from views that are still equal.
      rw [ih _ _ _ _
        (extend_keeps_views_equal declared₁ declared₂ batch₁ stored₁ batch₂ stored₂ same
          c.name _)]

-- Two calls that start with the same processed types and see the same end
-- with the same processed types.
theorem after_depends_on_what_is_seen
    (hash : String → List (String × Kind) → Nat) (declared₁ declared₂ : String → Bool)
    (cs : List Coll) :
    ∀ (batch stored₁ stored₂ : Known),
      seen declared₁ batch stored₁ = seen declared₂ batch stored₂ →
      after hash declared₁ stored₁ batch cs = after hash declared₂ stored₂ batch cs := by
  -- By induction on the declarations left to add.
  induction cs with
  -- Nothing left: both end where they started.
  | nil => intro _ _ _ _; rfl
  -- One declaration, then the rest.
  | cons c rest ih =>
      -- Name the starting view.
      intro batch stored₁ stored₂ same
      -- This collection gets the same ID on both sides.
      have head :=
        id_depends_on_what_is_seen hash declared₁ declared₂ batch stored₁ batch stored₂ same c
      -- Unfold one step of both calls.
      simp only [after]
      -- Use the equal ID everywhere.
      rw [head]
      -- The rest follows by induction, from views that are still equal.
      exact ih _ _ _
        (extend_keeps_views_equal declared₁ declared₂ batch stored₁ batch stored₂ same
          c.name _)

-- One call over `xs ++ ys` gives `xs` its IDs, then `ys` the IDs it gets
-- with `xs` processed.
theorem ids_append
    (hash : String → List (String × Kind) → Nat) (declared : String → Bool)
    (stored : Known) (xs ys : List Coll) :
    ∀ batch : Known,
      ids hash declared stored batch (xs ++ ys)
        = ids hash declared stored batch xs
            ++ ids hash declared stored (after hash declared stored batch xs) ys := by
  -- By induction on the first part.
  induction xs with
  -- No first part: the call is the second part.
  | nil => intro _; rfl
  -- One declaration of the first part, then the rest.
  | cons c rest ih =>
      -- Name what the call knows.
      intro batch
      -- Unfold one step, then use the induction hypothesis.
      simp only [List.cons_append, ids, after, ih]

-- When the store holds none of the names a call declares, the guard decides
-- nothing: the call sees its own types, then the store's.
theorem seen_of_new_names
    (declared : String → Bool) (batch stored : Known)
    (new_names : ∀ name, declared name = true → stored name = none) :
    seen declared batch stored = over batch stored := by
  -- Compare name by name.
  funext name
  -- Either the call declares the name or it does not.
  by_cases is_declared : declared name = true
  -- Declared: the store holds nothing under it, so skipping the store changes nothing.
  · simp [seen, over, is_declared, new_names name is_declared]
  -- Not declared: the store is looked up on both sides.
  · simp [seen, over, is_declared]

-- The headline. `xs` and `ys` are added in one call, or `xs` first and `ys`
-- in a second call that reads the IDs of `xs` from the store. When each
-- call declares only names its store does not hold yet (adding a name twice
-- is refused as "collection already exists"), every collection gets the
-- same ID both ways.
theorem added_alone_equals_added_together
    (hash : String → List (String × Kind) → Nat) (stored : Known) (xs ys : List Coll)
    (together first second : String → Bool)
    (new_together : ∀ name, together name = true → stored name = none)
    (new_first : ∀ name, first name = true → stored name = none)
    (new_second : ∀ name, second name = true →
      over (after hash first stored nothing xs) stored name = none) :
    ids hash together stored nothing (xs ++ ys)
      = ids hash first stored nothing xs
          ++ ids hash second (over (after hash first stored nothing xs) stored) nothing ys := by
  -- Split the one call into its two parts.
  rw [ids_append]
  -- At the start the one call and the first call see the same: the store.
  have start : seen together nothing stored = seen first nothing stored := by
    -- Neither declares a stored name.
    rw [seen_of_new_names together nothing stored new_together,
      seen_of_new_names first nothing stored new_first]
  -- So `xs` gets the same IDs in both.
  rw [ids_depend_on_what_is_seen hash together first xs _ _ _ _ start]
  -- And both have processed the same types once `xs` is added.
  rw [after_depends_on_what_is_seen hash together first xs _ _ _ start]
  -- `ys` then sees the same in the one call and in the second call.
  have rest : seen together (after hash first stored nothing xs) stored
      = seen second nothing (over (after hash first stored nothing xs) stored) := by
    -- The one call sees its processed types, then the store.
    rw [seen_of_new_names together _ stored new_together]
    -- The second call sees its store, which holds exactly that.
    rw [seen_of_new_names second nothing _ new_second]
    -- An empty call adds nothing to what its store holds.
    funext name
    -- `nothing` never answers, so the store's answer stands.
    simp [over, nothing]
  -- So `ys` gets the same IDs too.
  rw [ids_depend_on_what_is_seen hash together second ys _ _ _ _ rest]

-- The defect, as a witness: before the fix, a relation to a stored `Author`
-- resolved differently from the same relation inside a batch.
theorem old_alone_differs_from_batch (author : Nat) :
    resolveOld nothing (.to "Author")
      ≠ resolve undeclared (extend nothing "Author" author) nothing (.to "Author") := by
  -- Alone it is left named; in the batch it commits to the ID.
  simp [resolveOld, resolve, seen, undeclared, nothing, extend]

-- With the fix the kind commits to the stored target's ID.
theorem stored_target_is_committed
    (declared : String → Bool) (stored : Known) (name : String) (id : Nat)
    (not_declared : declared name = false) (held : stored name = some id) :
    resolve declared nothing stored (.to name) = .relation id := by
  -- The call has processed nothing and does not declare the name: the store answers.
  simp [resolve, seen, nothing, not_declared, held]

-- The guard: a relation to a type the call declares and has not processed
-- yet stays named, even when the store holds a collection of that name. The
-- second pass gives it the ID of the type the call declares.
theorem declared_namesake_stays_named
    (declared : String → Bool) (stored : Known) (name : String)
    (is_declared : declared name = true) :
    resolve declared nothing stored (.to name) = .named name := by
  -- Nothing is processed, and a declared name is not looked up in the store.
  simp [resolve, seen, nothing, is_declared]

-- The guard is needed, as a witness: without it (a call that forgets it
-- declares the name) the relation commits to the stored namesake's ID, which
-- is not the collection the SDL declares.
theorem without_the_guard_a_namesake_commits_to_the_stored_id
    (stored : Known) (name : String) (id : Nat) (held : stored name = some id) :
    resolve undeclared nothing stored (.to name) = .relation id := by
  -- With no name declared the store is looked up and answers.
  simp [resolve, seen, undeclared, nothing, held]

/-!
## A relation is an ID in a mutation input

`field_kind_to_input_type_ref` gives each field of `<Name>MutationInputArg`
its type. An input object may hold only input types, and a collection's
object type is not one: a single field typed as an object fails the build of
the whole GraphQL schema, for every collection of the node.

What must never happen: a field kind maps to an object type.
-/

-- The type a mutation input field can be given.
inductive InputType where
  -- A scalar, by its kind number.
  | scalar : Nat → InputType
  -- The `ID` scalar: a document ID.
  | id : InputType
  -- A collection's object type, which no input may hold.
  | object : String → InputType

-- Whether GraphQL accepts the type inside an input object.
def isInput : InputType → Bool
  -- A scalar is an input type.
  | .scalar _ => true
  -- `ID` is an input type.
  | .id => true
  -- An object type is not.
  | .object _ => false

-- The mutation input type of a field kind (`field_kind_to_input_type_ref`).
def inputOf : Kind → InputType
  -- A scalar keeps its kind.
  | .scalar kind => .scalar kind
  -- A relation takes the related document's ID.
  | .relation _ => .id
  -- So does a relation that was left named.
  | .named _ => .id

-- The mapping before the fix: a named relation became its object type.
def inputOfOld : Kind → InputType
  -- A scalar keeps its kind.
  | .scalar kind => .scalar kind
  -- A relation takes the related document's ID.
  | .relation _ => .id
  -- A named relation took the related collection's object type.
  | .named name => .object name

-- Every field kind gives a type GraphQL accepts in an input object.
theorem mutation_input_is_always_an_input_type (kind : Kind) :
    isInput (inputOf kind) = true := by
  -- Check the three kinds.
  cases kind <;> rfl

-- The defect, as a witness: a named relation gave a type GraphQL refuses.
theorem old_named_relation_is_not_an_input_type (name : String) :
    isInput (inputOfOld (.named name)) = false := by
  -- It mapped to the object type.
  rfl

end RelationTargetIdentity
