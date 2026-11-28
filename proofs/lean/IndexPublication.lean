-- A collection can change versions while indexes are being built. The cache
-- follows the chosen version and keeps a fresher build status for an index it knows.
-- A backfill can settle only for its own collection. A new index ID must clear
-- both the high-water mark left by prior writes and the IDs in the definition.
import Std

-- Collection::with_current_index_actions looks up the cached definition before
-- taking its status. The outer Option records whether the cache knows the index.
def indexStatus {Status : Type} (cached : Option (Option Status))
    (loaded : Option Status) : Option Status :=
  -- An existing index with no action is already built.
  cached.getD loaded

-- A stale load cannot turn an index known to be built back into a building index.
theorem knownBuiltStaysBuilt {Status : Type} (loaded : Option Status) :
    indexStatus (some none) loaded = none := by
  -- The outer Some makes the lookup choose the cache.
  rfl

-- IndexManager::next_index_id picks the successor of the shared floor or the
-- largest existing ID, whichever is greater. Checked addition rejects overflow.
def nextIndexId (floor highest : Nat) : Nat :=
  -- The successor is strictly larger than either input.
  max floor highest + 1

-- A successfully allocated ID is larger than any existing or retired ID.
theorem newIndexClearsBoth (floor highest : Nat) :
    floor < nextIndexId floor highest ∧ highest < nextIndexId floor highest := by
  -- The maximum is at least both inputs; adding one makes both inequalities strict.
  simp only [nextIndexId]
  omega

-- ensure_backfill_index_unchanged and FailBackfillAttempt require this identity
-- match before looking at the index number.
def sameBackfillTarget (planned actual : String) (plannedIndex actualIndex : Nat) : Prop :=
  -- An index number is meaningful only within its collection.
  planned = actual ∧ plannedIndex = actualIndex

-- A replacement collection cannot settle the old index even when IDs coincide.
theorem replacementCannotSettle (planned actual : String) (index : Nat)
    (replaced : planned ≠ actual) : ¬ sameBackfillTarget planned actual index index := by
  -- Settlement would prove the two collections equal, contradicting replacement.
  intro matched
  exact replaced matched.1

-- Collection::with_current_index_actions uses cached index definitions only
-- for the same version. The version switch uses the incoming list.
def indexDefinitions {Index : Type} (sameVersion : Bool)
    (cached incoming : List Index) : List Index :=
  -- A switch takes its definitions entirely from its target.
  if sameVersion then cached else incoming

-- A different version's cached definition cannot replace the target version.
theorem switchUsesTarget {Index : Type} (cached incoming : List Index) :
    indexDefinitions false cached incoming = incoming := by
  -- With a different version, the target list is returned directly.
  rfl
