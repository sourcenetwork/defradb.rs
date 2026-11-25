/-! The floor on a certified assignment read, in
`crates/engine/vera/src/vera_rs/provider/assignments.rs` `check_position`.
Terms take precedence over heights; at the same position the hash must match.
Hash parsing and certificate verification are outside this model. -/
namespace AssignmentPosition

-- A verified block's lexicographic position and its identity.
structure Position where
  term : Nat
  height : Nat
  hash : Nat

-- A later coordinate passes; an equal coordinate needs the same hash.
def admissible (current minimum : Position) : Prop :=
  current.term > minimum.term ∨
    (current.term = minimum.term ∧ current.height > minimum.height) ∨
    (current.term = minimum.term ∧ current.height = minimum.height ∧ current.hash = minimum.hash)

-- The comparison can be evaluated for concrete positions.
instance (current minimum : Position) : Decidable (admissible current minimum) :=
  inferInstanceAs (Decidable (_ ∨ _ ∨ _))

-- A missing floor allows the response; otherwise apply the position rule.
def accepts (current : Position) (minimum : Option Position) : Bool :=
  match minimum with
  | none => true
  | some floor => decide (admissible current floor)

-- An absent floor imposes no position restriction.
theorem no_floor (current : Position) : accepts current none = true := rfl

-- The check accepts precisely the admissible positions.
theorem accepted_iff (current minimum : Position) :
    accepts current (some minimum) = true ↔ admissible current minimum := by
  simp [accepts]

-- An earlier term is rejected even when its height is greater.
theorem stale_term_rejected (current minimum : Position) (h : current.term < minimum.term) :
    accepts current (some minimum) = false := by
  simp [accepts, admissible]
  omega

-- Within the same term, a lower height is rejected.
theorem stale_height_rejected (current minimum : Position)
    (term : current.term = minimum.term) (height : current.height < minimum.height) :
    accepts current (some minimum) = false := by
  simp [accepts, admissible, term]
  omega

-- A different block at the floor's coordinates is rejected.
theorem same_height_hash_rejected (current minimum : Position)
    (term : current.term = minimum.term) (height : current.height = minimum.height)
    (hash : current.hash ≠ minimum.hash) : accepts current (some minimum) = false := by
  simp [accepts, admissible, term, height, hash]

-- The floor itself is accepted.
theorem same_position_accepted (current : Position) : accepts current (some current) = true := by
  simp [accepts, admissible]

-- A later term is accepted even when its height is lower.
theorem later_term_accepted (current minimum : Position) (h : current.term > minimum.term) :
    accepts current (some minimum) = true := by
  simp [accepts, admissible, h]

-- A later height in the same term is accepted regardless of its hash.
theorem later_height_accepted (current minimum : Position)
    (term : current.term = minimum.term) (height : current.height > minimum.height) :
    accepts current (some minimum) = true := by
  simp [accepts, admissible, term, height]

end AssignmentPosition
