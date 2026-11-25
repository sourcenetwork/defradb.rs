/-!
# A deleted document's history keeps its access control

The story, in plain words. An owner adds a document to a collection with an
access policy, and the node keeps an access-control object for it: the
owner, and the actors the owner grants (a reader). Anyone else (a foreign
actor) is denied. An object the node does not hold reads as public.

A delete removes the document's rows but leaves its history (its commits and
blocks) on the node. The delete keeps the object, so the history is read
under the same rule as before. A purge removes the history and the object
together; the same content added again afterwards is a new document whose
creator has granted nobody. A deleted DocID cannot be added again while its
history is there.

What must never happen: a delete opens the history to the foreign actor, a
revoked reader still reads it, or a document added again after a purge
inherits a grant.

Code this models:
* `archive_deleted_doc` in `crates/database/db/src/write/autocommit/helpers.rs`
  (a delete keeps the object),
* `release_purged_doc_objects` in `crates/database/db/src/collection/acp.rs`
  (a purge removes it; a truncate keeps it, as Go does),
* `register_created_doc` in `crates/database/db/src/write/autocommit/helpers.rs`
  (a deleted DocID is refused),
* `DirectChecker::object_access` in `crates/engine/acp/src/read_access.rs`
  (the read rule).
-/
namespace DeletedHistory

-- The three kinds of actor that may try to read.
inductive Actor where
  -- The document's owner.
  | owner
  -- An actor the owner may grant `reader`.
  | reader
  -- An actor with no relationship to the document.
  | foreign
  deriving DecidableEq, Repr

-- Where the document is in its life.
inductive Phase where
  -- Rows and history are on the node.
  | live
  -- The rows are gone; the history is still on the node.
  | deleted
  -- Purged: rows, history and DocID mappings are gone.
  | gone
  deriving DecidableEq, Repr

-- What the node holds for one DocID.
structure State where
  -- Where the document is.
  phase : Phase
  -- Whether the node holds the document's access-control object.
  registered : Bool
  -- Whether that object grants the reader.
  readerGranted : Bool
  deriving DecidableEq, Repr

-- Whether a document in this phase still has its history on the node.
def hasHistory : Phase → Bool
  -- A purged document has none.
  | .gone => false
  -- Live and deleted documents keep theirs.
  | _ => true

-- Whom the object lets read, when the node holds it.
def objectAllows (s : State) : Actor → Bool
  -- The owner always.
  | .owner => true
  -- The reader once granted.
  | .reader => s.readerGranted
  -- Nobody else.
  | .foreign => false

-- The read rule: whether actor `a` may read the document's history in
-- state `s`. No object reads as public; otherwise the object decides.
def canRead (s : State) (a : Actor) : Bool :=
  -- Nothing to read once the history is gone; then no object, or the object's say.
  hasHistory s.phase && (!s.registered || objectAllows s a)

-- A committed delete: the rows go, the object stays as it is.
def delete (s : State) : State :=
  -- Only the phase changes.
  { s with phase := .deleted }

-- What the node did before this fix: the delete also removed the object.
def deleteAndUnregister (_ : State) : State :=
  -- The phase changes and the object and its grants go.
  { phase := .deleted, registered := false, readerGranted := false }

-- The owner revokes the reader; this needs the object's owner relation.
def revoke (s : State) : State :=
  -- The grant goes, if there is an object to take it from.
  { s with readerGranted := if s.registered then false else s.readerGranted }

-- A purge removes the history and, with it, the object.
def purge (_ : State) : State :=
  -- Nothing is left for this DocID.
  { phase := .gone, registered := false, readerGranted := false }

-- The owner adds the same content again. Refused (none) while a deleted
-- document's history is there; otherwise a new document whose creator has
-- granted nobody, registered for that creator (an object already there is
-- kept as it is).
def recreate (s : State) : Option State :=
  -- The answer depends only on where the document is.
  match s.phase with
  -- Registering keeps any object still held; a purge left none.
  | .gone => some { phase := .live, registered := true, readerGranted := s.registered && s.readerGranted }
  -- A live or deleted DocID cannot be added again.
  | _ => none

-- A registered live document, as its create leaves it, possibly with the reader granted.
def created (granted : Bool) : State :=
  -- Live, registered, with or without the reader's grant.
  { phase := .live, registered := true, readerGranted := granted }

/-- A delete changes nobody's access to the document's history. -/
theorem delete_keeps_access (s : State) (a : Actor) (h : s.phase = .live) :
    canRead (delete s) a = canRead s a := by
  -- Both states have history and the same object, so the rule answers alike.
  cases a <;> simp [delete, canRead, hasHistory, objectAllows, h]

/-- After a delete, the foreign actor still cannot read a protected document's history. -/
theorem deleted_history_hidden_from_foreign (granted : Bool) :
    canRead (delete (created granted)) .foreign = false := by
  -- The object is still there and names no foreign grant.
  simp [delete, created, canRead, hasHistory, objectAllows]

/-- The owner still reads its deleted document's history. -/
theorem owner_reads_deleted_history (granted : Bool) :
    canRead (delete (created granted)) .owner = true := by
  -- The object still names the owner.
  simp [delete, created, canRead, hasHistory, objectAllows]

/-- A reader granted before the delete still reads after it. -/
theorem reader_keeps_grant_after_delete :
    canRead (delete (created true)) .reader = true := by
  -- The grant is part of the object the delete kept.
  simp [delete, created, canRead, hasHistory, objectAllows]

/-- Revoking the reader after the delete denies it. -/
theorem revoke_after_delete_denies_reader (granted : Bool) :
    canRead (revoke (delete (created granted))) .reader = false := by
  -- The object is there, so the revocation takes the grant away.
  simp [revoke, delete, created, canRead, hasHistory, objectAllows]

/-- The old behaviour opened the history: removing the object on delete lets the foreign actor read. -/
theorem unregister_on_delete_leaks (granted : Bool) :
    canRead (deleteAndUnregister (created granted)) .foreign = true := by
  -- With no object the rule reads the history as public.
  simp [deleteAndUnregister, canRead, hasHistory, objectAllows]

/-- A deleted DocID cannot be added again while its history is there. -/
theorem recreate_refused_while_deleted (granted : Bool) :
    recreate (delete (created granted)) = none := by
  -- The deleted phase is not `gone`, so the create is refused.
  simp [recreate, delete, created]

/-- After a purge, the same DocID added again takes no grant from the purged document. -/
theorem purged_then_recreated_has_no_reader (s : State) :
    (recreate (purge s)).map (fun t => canRead t .reader) = some false := by
  -- The purge left no object, so the new one starts with no grant.
  simp [recreate, purge, canRead, hasHistory, objectAllows]

/-- Without the purge removing the object, a document added again would inherit the old grant. -/
theorem kept_object_carries_the_grant :
    (recreate { phase := .gone, registered := true, readerGranted := true }).map
      (fun t => canRead t .reader) = some true := by
  -- The object still held keeps its reader grant.
  simp [recreate, canRead, hasHistory, objectAllows]

end DeletedHistory
