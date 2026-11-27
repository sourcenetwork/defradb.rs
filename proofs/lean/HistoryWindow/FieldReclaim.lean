/-!
# HistoryWindow.FieldReclaim: which field blocks a history prune deletes

The story, in plain words. Once a history prune has chosen the composites it
keeps, it looks at every field block of the document that the priority index
names. A field block stays when a kept composite links it, or when it is the
live head of its field, or when another document still owns it. Every other
field block goes, whatever composite once linked it: one an earlier prune
kept only because it was its field's head goes once a newer write takes its
place.

What must never happen:
* a block a kept composite links is deleted (`linked_kept`);
* a field's live head is deleted (`field_head_kept`);
* a block another document owns is deleted (`shared_kept`).
What must happen: a field block nothing keeps is deleted (`unkept_reclaimed`),
and a head that stops being a head is deleted by the next prune once nothing
else keeps it (`superseded_head_reclaimed`).

Rust anchor: `prune_doc_in` in
`crates/database/db/src/collection/ops/prune_history.rs`, with
`delete_owned_block` in `crates/database/db/src/block/cleanup.rs`.
-/

namespace HistoryWindow

-- Whether the prune deletes a field block, given whether a kept composite
-- links it, whether it is its field's live head, and whether another
-- document owns it: only when none of the three holds.
def fieldDeleted (linked head ownedElsewhere : Bool) : Bool :=
  !linked && !head && !ownedElsewhere

-- A field block a kept composite links is never deleted.
theorem linked_kept (head ownedElsewhere : Bool) :
    fieldDeleted true head ownedElsewhere = false := by
  -- `!true` is `false`, and `false && _` is `false`.
  simp [fieldDeleted]

-- A field's live head is never deleted.
theorem field_head_kept (linked ownedElsewhere : Bool) :
    fieldDeleted linked true ownedElsewhere = false := by
  -- The head test is `!true`, which is `false`, so the conjunction is `false`.
  cases linked <;> simp [fieldDeleted]

-- A field block another document owns is never deleted.
theorem shared_kept (linked head : Bool) :
    fieldDeleted linked head true = false := by
  -- The ownership test is `!true`, which is `false`, so the conjunction is `false`.
  cases linked <;> cases head <;> simp [fieldDeleted]

-- A field block nothing keeps is deleted.
theorem unkept_reclaimed : fieldDeleted false false false = true := by
  -- Every test is `!false`, which is `true`.
  simp [fieldDeleted]

-- A block kept the first time only as its field's head is deleted the next
-- time once it is no head any more, when no kept composite links it and no
-- other document owns it: the decision depends only on the three facts as
-- they stand now, not on why it was kept before.
theorem superseded_head_reclaimed (before : Bool)
    (_kept : fieldDeleted false true false = before) :
    fieldDeleted false false false = true := by
  -- What happened before does not enter the decision.
  simp [fieldDeleted]

end HistoryWindow
