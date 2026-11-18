/-!
# CatalogPin: a pinned version answers only for its own name, and clearing it restores the cache

The story, in plain words. A transaction's catalog answers every lookup of a
collection by name. A request may pin one version of a collection (one the
caller selected, possibly inactive) for its turn; while pinned, a lookup of
that collection's name answers the pinned version. The cache underneath is
never changed by the pin, so clearing it leaves lookups exactly as they were.

What must never happen: the pin answers for another collection's name, a
lookup of the pinned name answers the cached (active) version while pinned,
or clearing the pin leaves the catalog answering anything but what the cache
holds.

This file proves that a pinned lookup of the pinned name answers the pin
(`pinned_name_answers_pin`), that a lookup of any other name answers the
cache (`other_name_answers_cache`), and that with the pin cleared every
lookup answers the cache (`cleared_answers_cache`). The sequence of turns
is the TLA+ model `proofs/tla/VersionPin.tla`.

Rust anchors: `crates/db/src/txn/catalog.rs` `Catalog::get`, `Catalog::pin`,
`Catalog::unpin`.
-/

namespace CatalogPin

-- A collection version, named by its collection's name and a version number.
structure Version where
  -- The collection's name, what lookups ask by.
  name : String
  -- Which version of that collection.
  version : Nat
  deriving DecidableEq

-- `Catalog::get`: the pinned version if it carries the name asked for,
-- otherwise whatever the cache holds for that name.
def get (cache : String → Option Version) (pin : Option Version) (name : String) :
    Option Version :=
  match pin with
  -- A pin for this name answers.
  | some pinned => if pinned.name = name then some pinned else cache name
  -- No pin: the cache answers.
  | none => cache name

-- While pinned, a lookup of the pinned version's own name answers the pin.
theorem pinned_name_answers_pin (cache : String → Option Version) (pinned : Version) :
    get cache (some pinned) pinned.name = some pinned := by
  -- The names match, so the pin answers.
  simp [get]

-- While pinned, a lookup of any other name answers what the cache holds.
theorem other_name_answers_cache (cache : String → Option Version) (pinned : Version)
    (name : String) (h : pinned.name ≠ name) :
    get cache (some pinned) name = cache name := by
  -- The names differ, so the pin stands aside.
  simp [get, h]

-- With the pin cleared, every lookup answers exactly what the cache holds:
-- the pin left nothing behind.
theorem cleared_answers_cache (cache : String → Option Version) (name : String) :
    get cache none name = cache name := by
  -- No pin: the cache answers.
  rfl

end CatalogPin
