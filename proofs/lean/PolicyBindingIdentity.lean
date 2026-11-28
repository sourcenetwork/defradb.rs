/-!
A field's identity and its policy relation binding.

A collection's identity is a hash over the definitions of its fields. A
schema author may bind a relation field to a policy relation
(`author: Author @immutable @policy(relation: "author")`), which decides who
may create documents in the collection. Nodes must agree on that, so a
governed collection (one with `@governed(root:)`) writes the binding into the
field's definition, as it writes the field's immutability.

What must never happen: a collection that is not governed, or a field with no
binding, gets a different definition than it had before bindings existed (it
would become a different collection, and every node holding it would stop
being its replica); or two governed fields that differ only in their binding
get the same definition (two nodes would think they agree on who may create
while they do not).

Rust owner: `field_to_delta_with_priority` in
`crates/foundation/schema/src/cid.rs`, writing the `policyRelation` entry of
`FieldDefinitionDeltaPayload` in `crates/foundation/defra-core/src/block_delta.rs`
(skipped when absent).

The model is over the entries of the definition, in order, not over bytes:
that the encoder writes an entry list the same way every time is the CBOR
encoder's, assumed here.
-/
namespace PolicyBindingIdentity

/-- One entry of a field definition: a key and its value. -/
structure Entry where
  -- The key, such as `name` or `policyRelation`.
  key : String
  -- The value, written as text in this model.
  value : String
  deriving DecidableEq, Repr

/-- What the model knows about a field. -/
structure Field where
  -- The entries every field writes: name, CRDT, kind, and the rest.
  base : List Entry
  -- Whether the field is `@immutable`.
  immutable : Bool
  -- The policy relation it is bound to, if any.
  binding : Option String
  deriving DecidableEq, Repr

/-- The immutability entry: written only for a governed, immutable field. -/
def immutableEntry (governed : Bool) (f : Field) : List Entry :=
  -- Present only when both hold; otherwise nothing is written.
  if governed && f.immutable then [⟨"immutable", "true"⟩] else []

/-- The definition before bindings existed. -/
def legacy (governed : Bool) (f : Field) : List Entry :=
  -- The common entries, then the immutability entry.
  f.base ++ immutableEntry governed f

/-- The binding entry: written only for a governed field with a binding. -/
def bindingEntry (governed : Bool) (f : Field) : List Entry :=
  -- Match on whether the collection is governed and the field is bound.
  match governed, f.binding with
  -- A governed, bound field writes its relation.
  | true, some relation => [⟨"policyRelation", relation⟩]
  -- Anything else writes nothing.
  | _, _ => []

/-- The definition the Rust code writes today. -/
def encode (governed : Bool) (f : Field) : List Entry :=
  -- Everything the legacy definition had, then the binding entry.
  legacy governed f ++ bindingEntry governed f

/-- A field without a binding gets the definition it had before bindings
existed, governed or not. -/
theorem unbound_is_legacy (governed : Bool) (f : Field) (h : f.binding = none) :
    encode governed f = legacy governed f := by
  -- With no binding the binding entry is empty, governed or not.
  cases governed <;> simp [encode, bindingEntry, h]

/-- A collection that is not governed gets the definition it had before
bindings existed, whatever its fields are bound to. -/
theorem ungoverned_is_legacy (f : Field) :
    encode false f = legacy false f := by
  -- Not governed: the binding entry is empty for every binding.
  cases hb : f.binding <;> simp [encode, bindingEntry, hb]

/-- Two governed fields that differ only in their binding get different
definitions. -/
theorem governed_commits_to_binding (f g : Field)
    (hbase : f.base = g.base) (himm : f.immutable = g.immutable)
    (hdiff : f.binding ≠ g.binding) :
    encode true f ≠ encode true g := by
  -- Suppose the two definitions were equal.
  intro heq
  -- The legacy parts are equal, since the fields agree outside the binding.
  have hleg : legacy true f = legacy true g := by
    simp [legacy, immutableEntry, hbase, himm]
  -- Removing the equal legacy prefix leaves the binding entries equal.
  have hbind : bindingEntry true f = bindingEntry true g := by
    unfold encode at heq
    rw [hleg] at heq
    exact List.append_cancel_left heq
  -- Equal binding entries mean equal bindings, against the assumption.
  apply hdiff
  cases hf : f.binding <;> cases hg : g.binding <;>
    simp_all [bindingEntry]

/-- The bug the governed gate prevents: writing the binding whatever the
collection is. -/
def encodeUngated (governed : Bool) (f : Field) : List Entry :=
  -- The binding entry is written as if every collection were governed.
  legacy governed f ++ bindingEntry true f

/-- Without the gate, an ungoverned bound field gets a new definition: its
collection would change identity. -/
theorem ungated_moves_an_ungoverned_identity :
    encodeUngated false ⟨[⟨"name", "_authorID"⟩], false, some "author"⟩ ≠
      legacy false ⟨[⟨"name", "_authorID"⟩], false, some "author"⟩ := by
  -- The two entry lists have different lengths, so they differ.
  decide

end PolicyBindingIdentity
