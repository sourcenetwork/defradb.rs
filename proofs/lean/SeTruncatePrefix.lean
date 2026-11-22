/-!
The story: searchable encryption keeps search records in the datastore, each
under its collection's `/se/` prefix and naming one document, held or only
received from a peer. An unfiltered truncate deletes the whole prefix of the
collection it empties.

Who acts: the truncate's `delete_prefix` over `DatastoreSE::collection_prefix`.

What must never happen: a record of the truncated collection survives, so an
encrypted search still finds a removed document (#2728); or a record of
another collection is removed with it.

Rust owner: crates/database/db/src/collection/ops/truncate/all.rs
`DB::truncate_metadata_in` (the `DatastoreSE::collection_prefix` delete);
crates/database/db/src/merge/se/serve.rs `store_artifacts`, which writes
received records under the same datastore prefix.
-/

namespace SeTruncatePrefix

-- One search record: the collection whose prefix it sits under, and the
-- document it names.
structure Record where
  -- The collection ID in the record's key prefix.
  collection : Nat
  -- The document the record names, held or not.
  doc : Nat

-- `delete_prefix` of one collection's prefix: keep every other record.
def deletePrefix (records : List Record) (c : Nat) : List Record :=
  records.filter (fun r => r.collection != c)

-- After the delete, no record of the truncated collection is left, for any
-- document, held or received.
theorem none_left (records : List Record) (c : Nat) :
    ∀ r ∈ deletePrefix records c, r.collection ≠ c := by
  intro r h
  simp [deletePrefix] at h
  exact h.2

-- Every record of another collection is kept.
theorem others_kept (records : List Record) (c : Nat) (r : Record)
    (h : r ∈ records) (hc : r.collection ≠ c) : r ∈ deletePrefix records c := by
  simp [deletePrefix, h, hc]

-- Deleting only the records of the documents the node holds, as a cleanup
-- that walks its own documents would, leaves a received record behind.
def deleteHeld (records : List Record) (c : Nat) (held : List Nat) : List Record :=
  records.filter (fun r => !(r.collection == c && held.contains r.doc))

-- A received record for a document the node does not hold survives that.
theorem held_only_misses_received (records : List Record) (c : Nat) (held : List Nat)
    (r : Record) (h : r ∈ records) (hdoc : r.doc ∉ held) : r ∈ deleteHeld records c held := by
  simp [deleteHeld, h, hdoc]

end SeTruncatePrefix
