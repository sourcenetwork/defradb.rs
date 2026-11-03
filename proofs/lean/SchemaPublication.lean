/- A schema's disk commit may precede its ordered cache publication.
   After successful cache publication, the caller receives success only once
   that callback completes; the existing owner reports callback panics.
   Models OnCommit.OrderedAndWait and the existing ordered-event owner. -/
namespace SchemaPublication

/-- The schema's storage, predecessor, publication and reply facts. -/
structure State where
  durable : Bool
  earlierDone : Bool
  visible : Bool
  returned : Bool

/-- The four independently scheduled steps in the publication handshake. -/
inductive Action where
  | commit | releaseEarlier | publish | reply

/-- A reply implies the schema cache has been published. -/
def valid (s : State) : Prop := s.returned = true → s.visible = true

/-- Before the operation starts, no schema or reply has been published. -/
def initial : State := ⟨false, false, false, false⟩

/-- The starting state has no premature reply. -/
theorem initial_valid : valid initial := by simp [valid, initial]

/-- The ordered callback releases a reply only after publishing the cache. -/
def step (a : Action) (s : State) : State :=
  match a with
  | .commit => { s with durable := true }
  | .releaseEarlier => { s with earlierDone := true }
  | .publish => if s.durable && s.earlierDone then { s with visible := true } else s
  | .reply => if s.durable && s.visible then { s with returned := true } else s

/-- Every scheduled step preserves immediate visibility after a reply. -/
theorem preserves (s : State) (a : Action) (h : valid s) : valid (step a s) := by
  cases s with
  | mk d e v r =>
    cases d <;> cases e <;> cases v <;> cases r <;> cases a <;>
      simp_all [valid, step]

end SchemaPublication
