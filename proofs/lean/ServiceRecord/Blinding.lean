/-!
# ServiceRecord.Blinding: which collection key a node advertises

The story, in plain words. A record says which collections a node holds.
Collection ids come from the schema, so anyone with the same schema can work
them out. By default a node advertises a blinded key instead: a keyed hash
of the id under the network secret, which only nodes sharing the secret can
match. A collection made public is advertised by its plain id. A node with
no network secret advertises no key for a collection that is not public.

What must never happen:
* a blinded key and a plain key encode to the same bytes, so one is matched
  as the other (`encodeKey_injective`, `blinded_ne_plain`);
* a node without a secret advertises a non-public collection
  (`no_secret_not_advertised`);
* two nodes sharing a secret disagree on a collection's key
  (`same_secret_same_key`), or two collections share one blinded key while
  the hash keeps distinct inputs distinct (`distinct_ids_distinct_keys`).

The keyed hash is cryptography and is not proved here: it is a parameter
`mac`, and only the property the Rust relies on (distinct inputs, distinct
outputs, for a fixed secret) is assumed where a theorem needs it.

Rust anchors: `blind`, `advertised_key`, `CollectionKey::encode` in
`crates/p2p/src/iroh/records/key.rs`.
-/

namespace ServiceRecord

-- A collection key as a record carries it.
inductive Key where
  -- The keyed hash of the collection id.
  | blinded (hash : List Nat)
  -- The collection id itself, for a public collection.
  | plain (id : List Nat)
  deriving DecidableEq

-- The bytes a key is written as: a tag byte, then its contents.
def encodeKey : Key → List Nat
  -- Tag 0 for a blinded key;
  | .blinded hash => 0 :: hash
  -- tag 1 for a plain one.
  | .plain id => 1 :: id

-- Promise: two keys with the same bytes are the same key.
theorem encodeKey_injective (a b : Key)
    -- Given both keys write the same bytes,
    (h : encodeKey a = encodeKey b) :
    -- they are equal.
    a = b := by
  -- Compare tags first, then contents.
  cases a <;> cases b <;> simp_all [encodeKey]

-- Promise: a blinded key never writes the same bytes as a plain one.
theorem blinded_ne_plain (hash id : List Nat) :
    encodeKey (.blinded hash) ≠ encodeKey (.plain id) := by
  -- The tag bytes differ.
  simp [encodeKey]

-- The bytes the hash is taken over: a fixed label, then the id. The label
-- keeps these hashes apart from any other use of the network secret.
def label : List Nat := [100, 115, 114, 49]

-- The blinded key for a collection id under a secret.
def blind (mac : List Nat → List Nat → List Nat) (secret id : List Nat) : Key :=
  .blinded (mac secret (label ++ id))

-- The key a node advertises for a collection: its plain id when public,
-- its blinded key when the node has a secret, and none otherwise.
def advertisedKey (mac : List Nat → List Nat → List Nat)
    (secret : Option (List Nat)) (public : Bool) (id : List Nat) : Option Key :=
  if public then some (.plain id) else secret.map (fun s => blind mac s id)

-- Promise: without a secret, a collection that is not public is not advertised.
theorem no_secret_not_advertised (mac : List Nat → List Nat → List Nat) (id : List Nat) :
    advertisedKey mac none false id = none := by
  -- Not public, and no secret to blind with.
  rfl

-- Promise: a public collection is advertised by its plain id, secret or not.
theorem public_is_plain (mac : List Nat → List Nat → List Nat)
    (secret : Option (List Nat)) (id : List Nat) :
    advertisedKey mac secret true id = some (.plain id) := by
  -- The public branch is taken first.
  rfl

-- Promise: two nodes with the same secret advertise the same key for a
-- collection, so they can match each other.
theorem same_secret_same_key (mac : List Nat → List Nat → List Nat)
    (secret id : List Nat) :
    advertisedKey mac (some secret) false id = some (blind mac secret id) := by
  -- Both blind the same id under the same secret.
  rfl

-- Promise: when the hash keeps distinct inputs distinct, two collections
-- never share a blinded key.
theorem distinct_ids_distinct_keys (mac : List Nat → List Nat → List Nat)
    (secret a b : List Nat)
    -- Given the hash, for this secret, gives distinct inputs distinct outputs,
    (hmac : ∀ x y, mac secret x = mac secret y → x = y)
    -- and the two ids differ,
    (hne : a ≠ b) :
    -- their blinded keys differ.
    blind mac secret a ≠ blind mac secret b := by
  intro h
  -- Equal keys mean equal hashes,
  simp only [blind, Key.blinded.injEq] at h
  -- equal hashes mean equal inputs,
  have := hmac _ _ h
  -- and the label is the same on both, so the ids are equal.
  simp only [List.append_cancel_left_eq] at this
  exact hne this

end ServiceRecord
