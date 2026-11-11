/-!
# A saved document lands under the collection that owns it

The story, in plain words. A document belongs to one collection, and its
stored value lives under that collection's key. A document can arrive stamped
with a version other than the one the node saves it through, for example a
replicated document at a newer version. The save then looks that version up,
to store the document with that version's field layout.

Sometimes the version the node holds under that ID is not a definition of this
collection at all: a migration registered before its source was known leaves a
placeholder in an "orphan" collection. The old save used whatever it found, so
the document was stored under the orphan's key, and a query of the collection
it belongs to never saw it (issue #2517). The save now uses the version only
when it is a real definition of the same collection; otherwise it stores the
document as for an unknown version, under its own collection's key. This file
proves the stored key always names the owning collection, that a placeholder
never lays the document out (even one in the same collection, which a
migration makes when its source is known: it has no fields), and shows the old
choice did not.

Rust owner: crates/db/src/collection/crud.rs
`Collection::save_with_datastore_priorities`. The Rust checks are the testo
scenarios collection_version::migrations::query::with_p2p::
collection_migration_query_with_p2_p_replicated_doc_at_much_newer_schema_version_with_schema_history_gap
(an orphan placeholder) and
collection_migration_query_with_p2_p_replicated_doc_at_newer_schema_version (a
placeholder in the same collection).
-/

namespace DocStoreTarget

-- A collection version as the save sees it: the collection it belongs to and
-- whether it is a placeholder.
structure Version where
  collectionId : String
  placeholder : Bool

-- Whether the found version may lay the document out: a real definition of the
-- same collection.
def usable (self found : Version) : Bool :=
  !found.placeholder && found.collectionId == self.collectionId

-- The version the save stores through: the found one when usable, else itself.
def storeVia (self : Version) (found : Option Version) : Version :=
  match found with
  | some v => if usable self v then v else self
  | none => self

-- The collection whose key the stored value goes under.
def storedUnder (self : Version) (found : Option Version) : String :=
  (storeVia self found).collectionId

-- Whatever the lookup finds, the value is stored under the owning collection.
theorem stored_under_owner (self : Version) (found : Option Version) :
    storedUnder self found = self.collectionId := by
  -- Split on what the lookup found, then on whether it is usable.
  cases found with
  | none => rfl
  | some v =>
      simp only [storedUnder, storeVia]
      split
      · -- A usable version names the same collection.
        rename_i h
        simp [usable] at h
        exact h.2
      · rfl

-- A placeholder never lays the document out: the save stores it through its
-- own version, whatever collection the placeholder names.
theorem placeholder_never_lays_out (self found : Version)
    (h : found.placeholder = true) : storeVia self (some found) = self := by
  -- A placeholder is not usable, so the save keeps its own version.
  simp [storeVia, usable, h]

-- The old choice: any found version.
def oldStoredUnder (self : Version) (found : Option Version) : String :=
  (found.getD self).collectionId

-- The users collection, and the orphan placeholder a migration left.
def users : Version := ⟨"users", false⟩
def orphan : Version := ⟨"OrphanCollectionID", true⟩

-- The old choice stored the document under the orphan.
theorem old_stores_under_orphan : oldStoredUnder users (some orphan) ≠ users.collectionId := by
  decide

end DocStoreTarget
