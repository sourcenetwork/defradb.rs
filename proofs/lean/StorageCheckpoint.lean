/-!
A stopped node has a catalog and content-addressed blocks in the same store.
Opening the copied store lets the next node look up those same bytes.
Regolith's checkpoint supplies the copy of each key's bytes.
`RegolithStore::checkpoint_to_new_directory` in
`crates/engine/storage/src/backends/regolith/store.rs` delegates that copy.
-/

-- Group the claims about copying a store together.
namespace StorageCheckpoint

-- Each store answers a key with its bytes or reports the key absent.
abbrev CheckpointRead := List UInt8 → Option (List UInt8)

-- Reading any keys, including definitions, genesis, and signature links,
-- gives the same answers after a checkpoint that preserved the store entries.
theorem checkpoint_reads
    (source destination : CheckpointRead) -- The original and copied stores.
    (copied : ∀ key, destination key = source key) -- The engine's copy contract.
    (keys : List (List UInt8)) : -- The keys a reader visits.
    keys.map destination = keys.map source := by -- The full lookup result agrees.
  induction keys with -- Read one key at a time.
  | nil => rfl -- Neither reader looks anything up.
  | cons key rest next => -- Compare the first answer, then the remaining answers.
      simp only [List.map_cons, copied key, next] -- Both comparisons are equalities.

-- A document ID derived from the bytes at the genesis key stays the same.
theorem checkpoint_genesis_id
    (source destination : CheckpointRead) -- The original and copied stores.
    (copied : ∀ key, destination key = source key) -- The engine's copy contract.
    (genesisKey : List UInt8) -- Where the original genesis is stored.
    (deriveId : Option (List UInt8) → Option String) : -- How a reader obtains an ID.
    deriveId (destination genesisKey) = deriveId (source genesisKey) := -- The ID agrees.
  congrArg deriveId (copied genesisKey) -- The same input gives the same ID.

-- Close the group of checkpoint claims.
end StorageCheckpoint
