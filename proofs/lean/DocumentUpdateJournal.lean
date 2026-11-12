/-!
One committed document block may be observed repeatedly, including through
replication retries. The journal records it once per collection and document,
but distinct blocks for the same document remain distinct. A merged block that
left its document unchanged is not recorded at all. A reader pages the journal
in order, and sees only rows of documents that are alive and that it is
authorized to read.

Rust owners: crates/db/src/event/updates.rs (record, read, rows, once_key),
crates/db/src/event/updates/gate.rs (Gate::admits), and the document update
and merge writers that append the journal entry in the document transaction.
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

-- A merged block is recorded only when it changed its document.
def recordMerged (changed : Bool) (journal : List Update) (update : Update) : List Update :=
  if changed then appendOnce journal update else journal

-- A reader sees a row only when its document is authorized and not deleted.
def visible (allowed deleted : Update → Bool) (journal : List Update) : List Update :=
  journal.filter (fun update => allowed update && !deleted update)

-- A page holds at most `limit` rows, starting right after the checkpoint.
def page (journal : List Update) (after limit : Nat) : List Update :=
  (journal.drop after).take limit

-- The once key of one collection: the document's length in a fixed number of
-- bytes, then the document, then the block. `width` stands for the length
-- field, which Rust writes as eight big-endian bytes.
def onceKey (width : Nat → List Nat) (document block : List Nat) : List Nat :=
  width document.length ++ document ++ block

-- Retrying the same committed update leaves the journal unchanged.
theorem append_idempotent (journal : List Update) (update : Update) :
    appendOnce (appendOnce journal update) update = appendOnce journal update := by
  by_cases h : update ∈ journal <;> simp [appendOnce, h]

-- A different block on the same document is not treated as the old update.
theorem different_blocks_are_distinct (collection : Nat) (document a b : String)
    (h : a ≠ b) : Update.mk collection document a ≠ Update.mk collection document b := by
  intro same
  exact h (congrArg Update.block same)

-- A merged block that changed nothing leaves the journal as it was.
theorem superseded_adds_no_row (journal : List Update) (update : Update) :
    recordMerged false journal update = journal := by
  simp [recordMerged]

-- A row that fails the access check cannot appear in the reader's page.
theorem unauthorized_not_visible (allowed deleted : Update → Bool) (journal : List Update)
    (update : Update) (h : allowed update = false) :
    update ∉ visible allowed deleted journal := by
  simp [visible, h]

-- A row of a deleted document cannot appear either, whoever is reading.
theorem deleted_not_visible (allowed deleted : Update → Bool) (journal : List Update)
    (update : Update) (h : deleted update = true) :
    update ∉ visible allowed deleted journal := by
  simp [visible, h]

-- A page never holds more rows than its limit.
theorem page_within_limit (journal : List Update) (after limit : Nat) :
    (page journal after limit).length ≤ limit := by
  simp [page, List.length_take, Nat.min_le_left]

-- Two pages read one after the other hold exactly the rows of one larger
-- page: nothing between them is skipped, and nothing is read twice.
theorem pages_join (journal : List Update) (after first second : Nat) :
    page journal after first ++ page journal (after + first) second
      = page journal after (first + second) := by
  simp [page, List.take_add, List.drop_drop]

-- Two once keys are equal only for the same document and the same block, as
-- long as the length field has one size and tells lengths apart.
theorem once_key_injective (width : Nat → List Nat) (size : Nat)
    (fixed : ∀ n, (width n).length = size)
    (distinct : ∀ m n, width m = width n → m = n)
    (document block document' block' : List Nat)
    (same : onceKey width document block = onceKey width document' block') :
    document = document' ∧ block = block' := by
  unfold onceKey at same
  rw [List.append_assoc, List.append_assoc] at same
  have lengths : (width document.length).length = (width document'.length).length := by
    rw [fixed, fixed]
  have ⟨fields, rest⟩ := List.append_inj same lengths
  exact List.append_inj rest (distinct _ _ fields)

end DocumentUpdateJournal
