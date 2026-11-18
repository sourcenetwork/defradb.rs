namespace EnrollmentGenesis

/-- Local initial read access can be registered only for the addressee in a
verified owner's immutable creation instruction at the first registration. -/
def initialReader (first verified configured : Bool) (address receiver : Option String) : Bool :=
  first && verified && configured && receiver.isSome && address == receiver

theorem replay_keeps_revocation (verified configured : Bool) (address receiver : Option String) :
    initialReader false verified configured address receiver = false := by
  simp [initialReader]

theorem ordinary_delivery_keeps_private (first verified configured : Bool) (receiver : String) :
    initialReader first verified configured none (some receiver) = false := by
  simp [initialReader]

/-- Discovery collections explicitly publish their signed creation to all readers. -/
def publicReader (first verified configuredPublic : Bool) : Bool :=
  first && verified && configuredPublic

theorem public_replay_keeps_revocation (verified configuredPublic : Bool) :
    publicReader false verified configuredPublic = false := by
  simp [publicReader]

end EnrollmentGenesis
