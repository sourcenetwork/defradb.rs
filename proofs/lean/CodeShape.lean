/-!
# Code shape: relocating bounds preserves their constraints

The code-shape rule changes where a declaration writes its bounds, not which
constraints it states. This model represents each bound by an identifier and
lets its meaning be any predicate at all. It models declared constraints;
it does not claim that Rust enforces bounds on ordinary type aliases.

`relocate_preserves_constraints` proves that moving the inline list into the
where list describes exactly the same constraints, including when either list is
empty or a constraint occurs more than once.

Rust anchor: `Bounds::check_generics` in
`tools/code-shape/src/bounds.rs`. The model assumes the compiler's AST
faithfully represents the source; `tools/code-shape/ui/bounds.rs` exercises
the compiler adapter on functions, structs, enums, traits, aliases and impls.
This is a static constraint property, with no temporal or concurrent state.
-/

namespace CodeShape

-- A declaration has constraints in two places in the source.
structure Bounds where
  -- These are written inside the angle brackets.
  inlineBounds : List Nat
  -- These are already written in a where clause.
  whereBounds : List Nat

-- A caller must satisfy every constraint in both lists.
def accepts (meaning : Nat -> Prop) (bounds : Bounds) : Prop :=
  And (forall constraint, List.Mem constraint bounds.inlineBounds -> meaning constraint)
    (forall constraint, List.Mem constraint bounds.whereBounds -> meaning constraint)

-- Relocation retains every bound and empties only the inline list.
def relocate (bounds : Bounds) : Bounds :=
  { inlineBounds := [], whereBounds := bounds.whereBounds ++ bounds.inlineBounds }

-- The original and relocated declarations accept exactly the same callers.
theorem relocate_preserves_constraints (meaning : Nat -> Prop) (bounds : Bounds) :
    Iff (accepts meaning (relocate bounds)) (accepts meaning bounds) := by
  -- Prove each direction, so neither version accepts an extra caller.
  constructor
  . intro relocated
    -- An original bound occurs in the corresponding half of the joined list.
    constructor
    . intro constraint included
      exact relocated.2 constraint (List.mem_append.mpr (Or.inr included))
    . intro constraint included
      exact relocated.2 constraint (List.mem_append.mpr (Or.inl included))
  . intro original
    -- The empty inline list imposes nothing; each joined bound was original.
    constructor
    . intro constraint included
      cases included
    . intro constraint included
      -- Membership identifies which of the two original lists supplied it.
      rcases List.mem_append.mp included with existing | moved
      . exact original.2 constraint existing
      . exact original.1 constraint moved

-- A completed relocation leaves no inline constraint for the rule to flag.
theorem relocate_clears_inline (bounds : Bounds) : (relocate bounds).inlineBounds = [] := by
  -- This follows directly from the construction, independently of the bounds.
  rfl

end CodeShape
