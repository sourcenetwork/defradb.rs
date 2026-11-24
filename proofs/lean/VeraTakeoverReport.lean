/-
The outcome report for a Vera deployment takeover.

After a takeover, the node sends every journaled submission the new term no
longer holds. Each replay is put in exactly one of three places: the
deployment holds it again, the node owes it again, or it cannot be restored.
The embedder event is that report, not a second classification.

What must hold:
  * the three lists contain every replayed submission once, in replay order;
  * every item in a list has the kind named by that list;
  * projecting the report to the event keeps the three lists, their order,
    the actor and the unrestorable reason, and the closed reason.

Rust owner: `TakeoverReport`, `Applied` and `Unrestorable` in
`crates/engine/vera/src/vera_rs/follow/report.rs`; `AcpTakeoverReconciledData`
in `crates/foundation/events/src/event.rs`; the projection in
`crates/engine/vera/src/vera_rs/provider/terms.rs`.
The temporal side is `proofs/tla/TakeoverReport.tla`.
-/
namespace VeraTakeoverReport

/-- Where a replayed submission is held. -/
inductive Kind where
  | applied
  | owed
  | unrestorable
  deriving DecidableEq

/-- What kind of policy command the replay was. -/
inductive Command where
  | registration
  | relationship
  | policyCreation
  | archive
  deriving DecidableEq

/-- One replayed submission, with the fields the event can carry. -/
structure Submission where
  command : Command
  policyId : String
  resource : String
  object : String
  actor : Option String
  relation : Option String
  subject : Option String
  add : Option Bool
  reason : Option String
  kind : Kind

/-- A submission as the embedder event carries it. -/
structure EventSubmission where
  command : Command
  policyId : String
  resource : String
  object : String
  actor : Option String
  relation : Option String
  subject : Option String
  add : Option Bool
  reason : Option String

/-- One classified replay, as the code holds it before projecting. -/
structure Report where
  term : Nat
  cutHeight : Nat
  cutBlock : String
  replayed : List Submission
  closed : Option String

/-- Associativity of `Or`, kept local because the core proof environment
does not export it. -/
theorem or_assoc (p q r : Prop) : (p ∨ q) ∨ r ↔ p ∨ (q ∨ r) := by
  constructor
  · intro either
    cases either with
    | inl either =>
      cases either with
      | inl left => exact .inl left
      | inr middle => exact .inr (.inl middle)
    | inr right => exact .inr (.inr right)
  · intro either
    cases either with
    | inl left => exact .inl (.inl left)
    | inr either =>
      cases either with
      | inl middle => exact .inl (.inr middle)
      | inr right => exact .inr right

/-- Commutativity of `Or`. -/
theorem or_comm (p q : Prop) : p ∨ q ↔ q ∨ p := by
  constructor
  · intro either
    cases either with
    | inl left => exact .inr left
    | inr right => exact .inl right
  · intro either
    cases either with
    | inl left => exact .inr left
    | inr right => exact .inl right

/-- The embedder event projected from one report. -/
structure Event where
  term : Nat
  cutHeight : Nat
  cutBlock : String
  closed : Option String
  appliedAgain : List EventSubmission
  owedAgain : List EventSubmission
  unrestorable : List EventSubmission

/-- The report's partition: applied, owed, then unrestorable, each in the
replay order they were classified. -/
def partition (entries : List Submission) :
    List Submission × List Submission × List Submission :=
  match entries with
  | [] => ([], [], [])
  | entry :: rest =>
    let (applied, owed, unrestorable) := partition rest
    match entry.kind with
    | .applied => (entry :: applied, owed, unrestorable)
    | .owed => (applied, entry :: owed, unrestorable)
    | .unrestorable => (applied, owed, entry :: unrestorable)

/-- The event fields one submission carries. -/
def projected (entry : Submission) : EventSubmission :=
  {
    command := entry.command
    policyId := entry.policyId
    resource := entry.resource
    object := entry.object
    actor := entry.actor
    relation := entry.relation
    subject := entry.subject
    add := entry.add
    reason := entry.reason
  }

/-- The report as the embedder event. -/
def eventOf (report : Report) : Event :=
  {
    term := report.term
    cutHeight := report.cutHeight
    cutBlock := report.cutBlock
    closed := report.closed
    appliedAgain := (partition report.replayed).1.map projected
    owedAgain := (partition report.replayed).2.1.map projected
    unrestorable := (partition report.replayed).2.2.map projected
  }

/-- Partitioning keeps every submission: it is in one of the three lists
exactly when it was replayed; each list keeps the replay order of the
submissions it holds. -/
theorem partition_is_exact (entries : List Submission) :
    ∀ submission,
      submission ∈ (partition entries).1
        ∨ submission ∈ (partition entries).2.1
        ∨ submission ∈ (partition entries).2.2
        ↔ submission ∈ entries := by
  induction entries with
    | nil => intro submission; simp [partition]
    | cons entry rest ih =>
      simp only [partition]
      split
      · next _ heq => simp [ih, heq, or_assoc]
      · next _ heq =>
        intro submission
        rw [List.mem_cons]
        constructor
        · intro member
          cases member with
            | inl inFront =>
              exact List.Mem.tail _ ((ih submission).mp (Or.inl inFront))
            | inr nested =>
              cases nested with
                | inl inOwed =>
                  cases inOwed with
                    | inl equal => exact equal ▸ List.Mem.head _
                    | inr inMiddle =>
                      exact List.Mem.tail _ ((ih submission).mp (Or.inr (Or.inl inMiddle)))
                | inr inTail =>
                  exact List.Mem.tail _ ((ih submission).mp (Or.inr (Or.inr inTail)))
        · intro member
          cases member with
            | head => exact Or.inr (Or.inl (Or.inl rfl))
            | tail _ inRest =>
              cases (ih submission).mpr inRest with
                | inl inFront => exact Or.inl inFront
                | inr nested =>
                  cases nested with
                    | inl inMiddle => exact Or.inr (Or.inl (Or.inr inMiddle))
                    | inr inTail => exact Or.inr (Or.inr inTail)
      · next _ heq =>
        intro submission
        rw [List.mem_cons]
        constructor
        · intro member
          cases member with
            | inl inFront =>
              exact List.Mem.tail _ ((ih submission).mp (Or.inl inFront))
            | inr nested =>
              cases nested with
                | inl inMiddle =>
                  exact List.Mem.tail _ ((ih submission).mp (Or.inr (Or.inl inMiddle)))
                | inr tailEither =>
                  cases tailEither with
                    | inl equal => exact equal ▸ List.Mem.head _
                    | inr inTail =>
                      exact List.Mem.tail _ ((ih submission).mp (Or.inr (Or.inr inTail)))
        · intro member
          cases member with
            | head => exact Or.inr (Or.inr (Or.inl rfl))
            | tail _ inRest =>
              cases (ih submission).mpr inRest with
                | inl inFront => exact Or.inl inFront
                | inr nested =>
                  cases nested with
                    | inl inMiddle => exact Or.inr (Or.inl inMiddle)
                    | inr inTail => exact Or.inr (Or.inr (Or.inr inTail))

/-- Every applied entry is applied; the other lists name their own kinds. -/
theorem partition_kinds (entries : List Submission) :
    (partition entries).1.all (·.kind = .applied)
      /\ (partition entries).2.1.all (·.kind = .owed)
      /\ (partition entries).2.2.all (·.kind = .unrestorable) := by
  induction entries with
    | nil => simp [partition]
    | cons entry rest ih =>
      simp only [partition]
      split
      · next _ heq => simp [ih, heq]
      · next _ heq => simp [ih, heq]
      · next _ heq => simp [ih, heq]

/-- The event is exactly the report projected: the term, cut and closed
reason are kept, and each partition is mapped in its original order. -/
theorem event_projection_preserves_report (report : Report) :
    eventOf report =
      {
        term := report.term
        cutHeight := report.cutHeight
        cutBlock := report.cutBlock
        closed := report.closed
        appliedAgain := (partition report.replayed).1.map projected
        owedAgain := (partition report.replayed).2.1.map projected
        unrestorable := (partition report.replayed).2.2.map projected
      } := rfl

end VeraTakeoverReport
