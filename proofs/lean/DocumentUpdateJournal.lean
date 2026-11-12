/-!
One committed document block may be observed repeatedly, including through
replication retries. The journal records it once per collection and document,
but distinct blocks for the same document remain distinct. A reader only sees
rows for documents it is authorized to read.

Rust owners: crates/db/src/event/updates.rs and the document update and merge
writers that append the journal entry in the document transaction.
-/

namespace DocumentUpdateJournal

-- An update belongs to one collection, one document, and one committed block.
structure Update where
  collection : Nat
  document : String
  block : String
  deriving DecidableEq, Repr

-- A repeated committed block does not add another row.
def appendOnce (journal : List Update) (update : Update) : List Update :=
  if update ∈ journal then journal else journal ++ [update]

-- A reader sees only rows whose documents its authorization permits.
def visible (allowed : Update → Bool) (journal : List Update) : List Update :=
  journal.filter allowed

-- Retrying the same committed update leaves the journal unchanged.
theorem append_idempotent (journal : List Update) (update : Update) :
    appendOnce (appendOnce journal update) update = appendOnce journal update := by
  by_cases h : update ∈ journal <;> simp [appendOnce, h]

-- A different block on the same document is not treated as the old update.
theorem different_blocks_are_distinct (collection : Nat) (document a b : String)
    (h : a ≠ b) : Update.mk collection document a ≠ Update.mk collection document b := by
  intro same
  exact h (congrArg Update.block same)

-- A row that fails the access check cannot appear in the reader's page.
theorem unauthorized_not_visible (allowed : Update → Bool) (journal : List Update)
    (update : Update) (h : allowed update = false) : update ∉ visible allowed journal := by
  simp [visible, h]

end DocumentUpdateJournal
