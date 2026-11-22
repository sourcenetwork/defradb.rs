namespace AssignmentPositionFloor

/-- The position a certified read reports. -/
structure Position where
  term : Nat
  height : Nat
  hash : String

/-- Promise kept by `check_position`: a read is accepted only at a later
term, at a later height in the same term, or at the exact certified floor. -/
def atOrAboveFloor (read floor : Position) : Prop :=
  read.term > floor.term
    ∨ (read.term = floor.term
      ∧ (read.height > floor.height
        ∨ (read.height = floor.height ∧ read.hash = floor.hash)))

/-- A caller may require evidence at or after a position it has already
seen. The guard refuses an older term, an older height, or a different block
at the same position. -/
theorem floor_rejects_replays {read floor : Position}
    (before : read.term < floor.term) : ¬ atOrAboveFloor read floor := by
  intro accepted
  rcases accepted with older | same
  · exact (Nat.lt_asymm before older).elim
  · obtain ⟨same_term, height⟩ := same
    exact (Nat.lt_irrefl floor.term (same_term ▸ before)).elim

theorem floor_accepts_a_new_height {read floor : Position}
    (same_term : read.term = floor.term)
    (later : read.height > floor.height) : atOrAboveFloor read floor := by
  exact Or.inr ⟨same_term, Or.inl later⟩

end AssignmentPositionFloor
