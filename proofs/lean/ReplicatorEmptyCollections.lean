/-!
The story: the node keeps a registry of replicators, one entry per peer, each
with the list of collections it replicates. An operator may add a replicator
with no collections at all; Go keeps it, lists it and restores it on start.
Before #2702 defradb.rs dropped such an entry when storing it and when
loading it back, so the replicator was never listed.

Who acts: `store_info`, which stores one record; `load_from_infos`, which
rebuilds the registry from the persisted records; `is_any_replicator`, which
grants access; and `replicator_in_collection`, which picks push targets.

What must never happen: a stored record is missing from the registry, after
the store or after a load; a peer with no collections counts as a replicator
of anything; or a write to a collection is pushed to a peer that does not
list it.

Rust owner: crates/database/p2p/src/bitswap/registry.rs `store_info`,
`ReplicatorRegistry::load_from_infos`, `ReplicatorRegistry::is_any_replicator`;
crates/database/p2p/src/sync/coordinator/broadcast.rs `replicator_in_collection`.
-/

namespace ReplicatorEmptyCollections

-- One persisted replicator record: a peer and the collections it replicates.
structure Info where
  -- The peer's ID.
  peer : Nat
  -- The collection IDs, possibly none.
  cols : List Nat

-- The registry: each peer's collections, or `none` when it has no entry.
abbrev Registry := Nat → Option (List Nat)

-- The registry with no replicators.
def empty : Registry := fun _ => none

-- Set one peer's entry, leaving every other peer as it was.
def update (reg : Registry) (p : Nat) (v : Option (List Nat)) : Registry :=
  fun q => if q = p then v else reg q

-- `store_info` after the fix: always insert the record.
def storeInfo (reg : Registry) (i : Info) : Registry :=
  update reg i.peer (some i.cols)

-- `store_info` before the fix: a record with no collections removes the peer.
def storeInfoOld (reg : Registry) (i : Info) : Registry :=
  update reg i.peer (if i.cols.isEmpty then none else some i.cols)

-- `load_from_infos`: store every persisted record into an empty registry.
def load (infos : List Info) : Registry :=
  infos.foldl storeInfo empty

-- `is_any_replicator`: the peer has an entry with at least one collection.
def isAnyReplicator (reg : Registry) (p : Nat) : Bool :=
  match reg p with
  | some cs => !cs.isEmpty
  | none => false

-- `replicator_in_collection` after the fix: membership only.
def inCollection (cols : List Nat) (c : Nat) : Bool :=
  cols.contains c

-- `replicator_in_collection` before the fix: an empty list meant every collection.
def inCollectionOld (cols : List Nat) (c : Nat) : Bool :=
  cols.isEmpty || cols.contains c

-- Storing a record leaves exactly that record in the registry.
theorem stored_is_listed (reg : Registry) (i : Info) :
    storeInfo reg i i.peer = some i.cols := by
  simp [storeInfo, update]

-- A replicator stored with no collections grants nothing.
theorem empty_grants_nothing (reg : Registry) (p : Nat) :
    isAnyReplicator (storeInfo reg ⟨p, []⟩) p = false := by
  simp [isAnyReplicator, storeInfo, update]

-- A peer with no collections is picked for no collection's pushes.
theorem empty_gets_no_push (c : Nat) : inCollection [] c = false := by
  simp [inCollection]

-- A peer is picked for a collection's pushes only if it lists that collection.
theorem push_only_to_members (cols : List Nat) (c : Nat) (h : inCollection cols c = true) :
    c ∈ cols := by
  simpa [inCollection] using h

-- Loading more records never removes a peer that already has an entry.
theorem foldl_keeps (infos : List Info) :
    ∀ (reg : Registry) (q : Nat), (reg q).isSome → ((infos.foldl storeInfo reg) q).isSome := by
  induction infos with
  | nil => intro reg q h; simpa using h
  | cons i rest ih =>
    intro reg q h
    simp only [List.foldl]
    apply ih
    by_cases hq : q = i.peer <;> simp [storeInfo, update, hq, h]

-- After a load, every persisted record's peer has an entry, the ones with no
-- collections included.
theorem load_lists_every_record (infos : List Info) :
    ∀ (reg : Registry) (i : Info), i ∈ infos → ((infos.foldl storeInfo reg) i.peer).isSome := by
  induction infos with
  | nil => intro reg i h; cases h
  | cons j rest ih =>
    intro reg i h
    simp only [List.foldl]
    rcases List.mem_cons.mp h with rfl | h
    · apply foldl_keeps
      simp [storeInfo, update]
    · exact ih _ i h

-- The same, from the empty registry a restart starts with.
theorem restart_lists_every_record (infos : List Info) (i : Info) (h : i ∈ infos) :
    (load infos i.peer).isSome :=
  load_lists_every_record infos empty i h

-- The old store rule loses a replicator added with no collections (#2702).
theorem old_store_loses_empty (reg : Registry) (p : Nat) :
    storeInfoOld reg ⟨p, []⟩ p = none := by
  simp [storeInfoOld, update]

-- The old push rule sends every collection's writes to a peer with none.
theorem old_push_sends_everything (c : Nat) : inCollectionOld [] c = true := by
  simp [inCollectionOld]

end ReplicatorEmptyCollections
