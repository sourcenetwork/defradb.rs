/-!
Explicit selected-document transfer can precede continuous replication. Such a
transfer owns only its selected heads and reports failure if they are not all
acknowledged. Initial replay instead requires an installed replicator; removal
cannot turn it into an independent transfer or manufacture durable retry work.
The existing rooted grant and retry-registration owners retain their contracts.
-/
namespace DocumentPush

inductive Ownership where
  | selected
  | replicator
  deriving DecidableEq, Repr

/-- Selection does not install a replicator or change a captured replay's owner. -/
def ownership (explicit present : Bool) : Option Ownership :=
  if present then some .replicator else if explicit then some .selected else none

/-- Every dispatched head remains inside the selection and owns a serving grant. -/
def canSend (owner : Ownership) (present insideSelection hasGrant : Bool) : Bool :=
  insideSelection && hasGrant && (owner == .selected || present)

inductive Completion where
  | acknowledged
  | retainedRetry
  | failed
  deriving DecidableEq, Repr

/-- A retained retry is an outcome only when the durable retry owner kept it. -/
def completion (owner : Ownership) (allHeadsAcknowledged retryRetained : Bool) : Completion :=
  if allHeadsAcknowledged then .acknowledged
  else if owner == .replicator && retryRetained then .retainedRetry else .failed

theorem enrollment_without_subscription : ownership true false = some .selected := rfl

theorem absent_replay_is_refused : ownership false false = none := rfl

theorem installed_selection_keeps_replay_owner (explicit : Bool) :
    ownership explicit true = some .replicator := rfl

theorem removed_replay_cannot_send (insideSelection hasGrant : Bool) :
    canSend .replicator false insideSelection hasGrant = false := by
  simp [canSend]

theorem unselected_head_cannot_send (owner : Ownership) (present hasGrant : Bool) :
    canSend owner present false hasGrant = false := by
  simp [canSend]

theorem ungranted_head_cannot_send (owner : Ownership) (present insideSelection : Bool) :
    canSend owner present insideSelection false = false := by
  simp [canSend]

theorem selected_failure_is_not_deferred (retryRetained : Bool) :
    completion .selected false retryRetained = .failed := rfl

theorem replay_without_retry_cannot_defer :
    completion .replicator false false = .failed := rfl

/-- The executable owner derives inputs and expectations for native transfer cases. -/
def emit : IO Unit := do
  for explicit in [false, true] do
    for present in [false, true] do
      for acknowledged in [false, true] do
        let owner := ownership explicit present
        let outcome := owner.map fun own => completion own acknowledged present
        let tag := match outcome with
          | none => "refused"
          | some .acknowledged => "acknowledged"
          | some .retainedRetry => "retainedRetry"
          | some .failed => "failed"
        IO.println (String.intercalate "\t"
          [toString explicit, toString present, toString acknowledged, tag])

end DocumentPush

def main : IO Unit := DocumentPush.emit
