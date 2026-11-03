/-!
# CustomCrdtCode: a custom CRDT type leaves every existing definition's bytes alone

The story, in plain words. A field definition says which CRDT type the field
uses. On the wire, and in the hash that names the definition, the type is a
small number, its code: 0 for none, 1 for LWW, up to 7 for JSON. A custom
type, one a node registers by name, takes code 8 and adds its name in a
second key, `customCrdt`. A definition without that key is written exactly as
before, so leaving it out is what keeps old definitions, and their hashes,
the same.

What must never happen:
* an existing type written differently than before (`native_unchanged`), which
  would change the hash of every definition peers already share;
* a type read back as another (`decode_encode`), so two types sharing one
  encoding (`encode_injective`);
* a custom type taken for an existing one, or the reverse (`custom_not_native`).

The node keeps a code it does not know as `unknown`, so it can still name it
in an error; it only ever builds one for a code above the known ones.

Rust anchors: `CType::to_u8`, `CType::custom_name` and `CType::from_definition`
in `crates/schema/src/ctype.rs`; `field_to_delta_with_priority` in
`crates/schema/src/cid.rs`; `FieldDefinitionDeltaPayload::custom_crdt` in
`crates/defra-core/src/block_delta.rs` (issue #2183).
-/

namespace CustomCrdtCode

-- The CRDT types a field definition can name.
inductive CType where
  -- No CRDT: relations and other special fields.
  | none
  -- Last-write-wins register.
  | lww
  -- Embedded object.
  | object
  -- The document as a whole.
  | composite
  -- Counter that goes up and down.
  | pnCounter
  -- Counter that only goes up.
  | pCounter
  -- Collaborative text.
  | text
  -- JSON, a register per path.
  | json
  -- A type registered on the node under this name.
  | custom (name : String)
  -- A code this node does not know, kept to name it in an error.
  | unknown (code : Nat)
  deriving DecidableEq

-- The code each type had before custom types existed; custom has none.
def oldCode : CType → Option Nat
  -- The eight existing types keep their numbers.
  | .none => some 0
  | .lww => some 1
  | .object => some 2
  | .composite => some 3
  | .pnCounter => some 4
  | .pCounter => some 5
  | .text => some 6
  | .json => some 7
  -- A code the node did not know was written as it came.
  | .unknown c => some c
  -- Custom types are new.
  | .custom _ => none

-- What a definition carries for a type: its code, and a custom type's name.
-- `none` for the name means the `customCrdt` key is left out of the bytes.
def encode : CType → Nat × Option String
  -- An existing type: its code and no name.
  | .none => (0, none)
  | .lww => (1, none)
  | .object => (2, none)
  | .composite => (3, none)
  | .pnCounter => (4, none)
  | .pCounter => (5, none)
  | .text => (6, none)
  | .json => (7, none)
  | .unknown c => (c, none)
  -- A custom type: code 8 and its name.
  | .custom n => (8, some n)

-- Reading a code alone, as before custom types existed.
def fromCode : Nat → CType
  -- The eight known codes.
  | 0 => .none
  | 1 => .lww
  | 2 => .object
  | 3 => .composite
  | 4 => .pnCounter
  | 5 => .pCounter
  | 6 => .text
  | 7 => .json
  -- Anything else is kept as unknown.
  | c => .unknown c

-- `valid` says which names a custom type may take (the Rust `CrdtName::new`:
-- lowercase letters, digits and `_`, not a native type's name). The proofs
-- hold for any such rule.
variable (valid : String → Bool)

-- Reading a definition's code and name back into a type.
def decode : Nat × Option String → CType
  -- No name: the code alone, exactly as before.
  | (c, none) => fromCode c
  -- Code 8 with a name the node accepts: that custom type.
  | (c, some n) => if c = 8 ∧ valid n = true then .custom n else .unknown c

-- The types a node can hold: a custom type has a valid name, and an unknown
-- one a code above every known code.
def WellFormed : CType → Prop
  -- A custom name follows the naming rule.
  | .custom n => valid n = true
  -- A node builds unknown only for codes it does not know.
  | .unknown c => 8 ≤ c
  -- The eight existing types are always fine.
  | _ => True

/-- A code above the known ones reads back as unknown. -/
theorem fromCode_unknown (c : Nat) (h : 8 ≤ c) : fromCode c = .unknown c := by
  -- Write the code as 8 plus something: it matches none of 0 to 7.
  obtain ⟨k, rfl⟩ : ∃ k, c = k + 8 := ⟨c - 8, by omega⟩
  rfl

/-- Every existing type is written exactly as before: its old code, and no
`customCrdt` key. -/
theorem native_unchanged (c : CType) (h : oldCode c ≠ Option.none) :
    encode c = ((oldCode c).getD 0, Option.none) := by
  -- Check each type in turn; custom has no old code, so it is excluded.
  cases c <;> simp_all [encode, oldCode]

/-- A custom type never takes an existing type's encoding: it is the only
type that writes a name. -/
theorem custom_not_native (n : String) (c : CType) (h : oldCode c ≠ Option.none) :
    encode (.custom n) ≠ encode c := by
  -- Every other type writes no name; the custom type writes one.
  cases c <;> simp_all [encode, oldCode]

/-- Reading back what a well-formed type writes gives that type. -/
theorem decode_encode (c : CType) (h : WellFormed valid c) :
    decode valid (encode c) = c := by
  -- Each type in turn; an unknown code is above 7, so it reads as unknown.
  cases c with
  | custom n => simp_all [WellFormed, encode, decode]
  | unknown k =>
    -- Codes 0 to 7 are known, so a code of 8 or more falls through.
    simp only [WellFormed] at h
    exact fromCode_unknown k h
  | _ => rfl

/-- Two well-formed types never share an encoding. -/
theorem encode_injective (a b : CType) (ha : WellFormed valid a)
    (hb : WellFormed valid b) (h : encode a = encode b) : a = b := by
  -- Read both back: equal encodings read back as equal types.
  rw [← decode_encode valid a ha, ← decode_encode valid b hb, h]

/-- The existing codes still read back as the types they named. -/
theorem old_codes_read_back (c : CType) (h : oldCode c ≠ Option.none)
    (hw : WellFormed valid c) : decode valid ((oldCode c).getD 0, Option.none) = c := by
  -- The old bytes are the new bytes, which read back as the type.
  rw [← native_unchanged c h]
  exact decode_encode valid c hw

end CustomCrdtCode
