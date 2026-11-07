/-!
# A save never lands under an ID the caller did not name

The story, in plain words. A caller saves a document under an ID it names
(Go's `SaveDocument`). The save looks the ID up, deleted documents
included, and then either refuses or writes. Its write is an upsert keyed by
the ID: it updates a live document where it is and adds an absent one under
the ID, but it cannot see a deleted document, so writing over one adds the
content as a new document under another ID. This file proves that the
save's decision rules that out: a deleted document is refused, and every
document the save writes over is updated or added under the named ID.

`DocumentSave.tla` proves the other half: that the lookup and the write see
the same state, because they run in one transaction.

Rust owner: crates/query/src/documents/mod.rs `save` (the decision: the
`response::deleted` check before the upsert), crates/query/src/plan/mutation/upsert.rs
(what the upsert does with each state). The regression is
mutation::update::with_delete::update_save_deleted_doc_does_nothing.
-/

namespace DocumentSave

-- The three states the document under the saved ID can be in.
inductive Slot where
  | absent
  | live
  | deleted
  deriving DecidableEq, Repr

-- What the save decides after its lookup.
inductive Decision where
  | refuse
  | upsert
  deriving DecidableEq, Repr

-- What the upsert does to each state: update in place, add under the ID, or,
-- for a document it cannot see, add the content under another ID.
inductive Effect where
  | updated
  | added
  | forked
  deriving DecidableEq, Repr

-- The save's decision: refuse a deleted document, upsert anything else.
def decide : Slot → Decision
  | .deleted => .refuse
  | _ => .upsert

-- The upsert's effect on the state the lookup found.
def upsertEffect : Slot → Effect
  | .live => .updated
  | .absent => .added
  | .deleted => .forked

-- Promise: a deleted document is refused.
theorem decide_refuses_deleted : decide .deleted = .refuse := rfl

-- Promise: the save refuses only a deleted document.
theorem refuse_only_deleted (s : Slot) (h : decide s = .refuse) : s = .deleted := by
  -- Check each state: only `deleted` decides `refuse`.
  cases s <;> simp_all [decide]

-- Promise: whatever the save decides to write over, it never forks.
theorem save_never_forks (s : Slot) (h : decide s = .upsert) : upsertEffect s ≠ .forked := by
  -- `upsert` is decided only for `absent` and `live`, which update or add.
  cases s <;> simp_all [decide, upsertEffect]

-- Promise: a written save lands under the named ID, updated or added.
theorem save_under_given_id (s : Slot) (h : decide s = .upsert) :
    upsertEffect s = .updated ∨ upsertEffect s = .added := by
  -- The same case split: live updates, absent adds.
  cases s <;> simp_all [decide, upsertEffect]

end DocumentSave
