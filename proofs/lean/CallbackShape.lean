/-!
# Naming a callback retains the callables it accepts

A callback interface has a call signature and additional bounds, such as Send
or a borrow lifetime. Giving that shape a name must keep both constraints.
The named trait inherits the signature; its blanket implementation requires
the same signature and bounds. No closure is boxed or allocated by this step.

The model treats Rust signatures and bounds as arbitrary predicates. It proves
that the named interface accepts exactly the original callables, including
capturing closures. Rust parsing, higher-ranked lifetime checking and trait
resolution remain compiler boundaries, exercised by the callback UI fixtures.

Rust anchor: `Callbacks::check_poly_trait_ref` in
`tools/code-shape/src/callbacks.rs`. A direct callable bound on `Self` names
that interface; a bound on an associated type has a different callable
subject. This is a static property with no temporal or concurrent state.
-/

namespace CallbackShape

-- The signature and its additional constraints describe a callable value.
structure Shape (Callable : Type) where
  -- This predicate includes the arguments, result and borrow relationships.
  signature : Callable -> Prop
  -- This predicate includes any other bounds on the callable.
  bounds : Callable -> Prop

-- An anonymous interface requires the signature and the additional bounds.
def anonymous (shape : Shape Callable) (callable : Callable) : Prop :=
  And (shape.signature callable) (shape.bounds callable)

-- A blanket implementation admits every callable satisfying the signature.
def blanket (shape : Shape Callable) (callable : Callable) : Prop :=
  shape.signature callable

-- The use of the named trait retains the additional bounds at the use site.
def named (shape : Shape Callable) (callable : Callable) : Prop :=
  And (blanket shape callable) (shape.bounds callable)

-- Naming a shape neither loses accepted callables nor admits invalid ones.
theorem naming_preserves_callables (shape : Shape Callable) (callable : Callable) :
    Iff (named shape callable) (anonymous shape callable) := by
  -- Both interfaces expand to the same two requirements.
  rfl

end CallbackShape
