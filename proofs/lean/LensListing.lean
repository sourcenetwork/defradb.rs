/-!
# Listing lenses keeps every module of every lens

The story, in plain words. A lens is a list of modules, applied in order. The
database keeps each lens under its ID. When a caller lists the lenses, it gets
each ID back with that lens's modules, as Go's `model.Lens` does. The old
listing kept only each lens's first module, so a lens with two modules came
back with one. This file proves the listing loses nothing: every stored lens
comes back once, under its own ID, with all its modules in the same order.

Rust owner: crates/lens/src/wasm.rs `WasmTransformStore::list` and
crates/lens/src/store.rs `list`, which map each `(id, config)` to
`(id, config.lenses)`; crates/db/src/txn/registry/lifecycle.rs `list_lenses_as`
serializes the result. The Rust check is
tools/integration-test/tests/collection_version (ListLenses scenarios).
-/

namespace LensListing

-- A module is anything the store keeps for one step of a lens; its contents
-- do not matter to the listing.
variable {Module : Type}

-- A stored lens: its ID and its modules, in order.
structure Stored (Module : Type) where
  id : String
  modules : List Module

-- The listing: each stored lens becomes its ID paired with all its modules.
def listed (store : List (Stored Module)) : List (String × List Module) :=
  store.map (fun s => (s.id, s.modules))

-- The listing has exactly one entry per stored lens.
theorem listed_length (store : List (Stored Module)) :
    (listed store).length = store.length := by
  simp [listed]

-- The i-th entry of the listing is the i-th stored lens's ID with all of its
-- modules, in the same order: nothing is dropped or reordered.
theorem listed_keeps_modules (store : List (Stored Module)) (i : Nat)
    (h : i < store.length) :
    (listed store)[i]'(by simp [listed]; exact h) =
      ((store[i]'h).id, (store[i]'h).modules) := by
  simp [listed]

-- The old listing, which kept only the first module of each lens.
def firstOnly (store : List (Stored Module)) : List (String × List Module) :=
  store.map (fun s => (s.id, s.modules.take 1))

-- The old listing really lost data: a lens with two modules lists as one.
theorem firstOnly_drops (m1 m2 : Module) :
    firstOnly [{ id := "l", modules := [m1, m2] }] ≠
      listed [{ id := "l", modules := [m1, m2] }] := by
  simp [firstOnly, listed]

end LensListing
