/-!
Which issuer spellings a node issues a vera.rs delegation under.

A host gives the node a signer, which holds one key. A caller asks the node to
issue a delegation under an issuer DID, and the node writes that DID into the
token exactly as given. vera.rs then reads the issuer and checks the token's
signature against the key it names.

A `did:key` is spelled by its key type, its key, the multibase it is written
in (base58btc `z…` or base64url `u…`), and, for a curve point, whether the
point is written compressed or uncompressed. vera.rs takes every one of the
four spellings of a secp256k1 key, and only the base58btc spelling of the
compressed point of a P-256 key or of the one encoding of an Ed25519 key.

The node asks vera.rs's own `canonical_actor` which key an issuer names, and
issues exactly when it is the signer's key.

What must never happen: the node issues a token whose issuer vera.rs refuses
or names another key, or the node refuses an issuer vera.rs takes for the
signer's key (the bug: only the two base58btc spellings of a secp256k1 key
were taken).

The provider writes the issuer it sends, and keeps every actor, in base58btc
whatever multibase names the signer (`defra` and `wire` below).

Rust owner: `ActorKey::names` in
`crates/engine/vera-delegation/src/signer/admission.rs`, and `defra` and
`wire` in `crates/engine/vera-delegation/src/did_spelling.rs`; `names` is called by
`create_scoped_bearer_token_at` in `crates/engine/vera-delegation/src/bearer.rs`.
The function it asks is `canonical_actor` in vera.rs `vera-crypto`.

The model is over an abstract key. Two string-level facts are assumed here:
different keys never share a spelling (the encoding's injectivity), and
DefraDB's `bearer::did_from_key` / `crypto::create_did_key` and vera.rs's
`ActorKey::canonical_did` write the same base58btc DID for a key. The crate
tests `an_ed25519_or_p256_signer_named_in_base64url_issues_under_its_canonical_did_only`
and `key_types::vera_rs_verifies_a_delegation_from_each_key_type_and_names_its_actor`
check both sides of that canonical-DID assumption.
-/
namespace VeraIssuerSpelling

/-- The key types a delegation may be issued with. -/
inductive KeyType where
  -- secp256k1, signing ES256K.
  | secp256k1
  -- Ed25519, signing EdDSA.
  | ed25519
  -- P-256, signing ES256.
  | p256
  deriving DecidableEq

/-- The multibase a `did:key` is written in. -/
inductive Base where
  -- `did:key:z…`
  | base58btc
  -- `did:key:u…`
  | base64url
  deriving DecidableEq

/-- How the key's bytes are written. An Ed25519 key has one encoding, written
here as `compressed`. -/
inductive Point where
  -- The compressed point, or Ed25519's one encoding.
  | compressed
  -- The uncompressed point.
  | uncompressed
  deriving DecidableEq

/-- One spelling of a `did:key`. -/
structure Spelling (Key : Type) where
  -- The type of the key it names.
  type : KeyType
  -- The key it names.
  key : Key
  -- The multibase it is written in.
  base : Base
  -- How the key's bytes are written.
  point : Point

variable {Key : Type}

/-- Whether vera.rs takes a spelling as an issuer: any spelling of a secp256k1
key, and only the base58btc compressed one of the other types. -/
def veraTakes : KeyType → Base → Point → Bool
  -- A secp256k1 key is taken in all four spellings.
  | .secp256k1, _, _ => true
  -- The other types are taken in base58btc of the compressed key only.
  | _, .base58btc, .compressed => true
  -- Every other spelling of them is refused.
  | _, _, _ => false

/-- What vera.rs's `canonical_actor` gives for an issuer: the key it names
when it takes the spelling, and nothing when it refuses it. -/
def canonicalActor (issuer : Spelling Key) : Option (KeyType × Key) :=
  -- Taken: the type and key, whatever the spelling. Refused: nothing.
  if veraTakes issuer.type issuer.base issuer.point then some (issuer.type, issuer.key) else none

/-- The node's check: an issuer names the signer's key `own` when vera.rs's
canonical actor for it is that key. -/
def Names (own : KeyType × Key) (issuer : Spelling Key) : Prop :=
  -- vera.rs answered, and its answer is the signer's key.
  canonicalActor issuer = some own

/-- The node issues under an issuer exactly when vera.rs takes the spelling
and it names the signer's key: nothing refused is issued, nothing taken is
refused. -/
theorem names_iff_vera_takes_own_key (own : KeyType × Key) (issuer : Spelling Key) :
    Names own issuer ↔
      veraTakes issuer.type issuer.base issuer.point = true ∧ (issuer.type, issuer.key) = own := by
  -- Unfold the check, then split on whether vera.rs takes the spelling.
  unfold Names canonicalActor
  -- Each side of the `if` gives the matching side of the statement.
  split <;> simp_all

/-- Every one of the four spellings of a secp256k1 key names that key, in
base58btc and in base64url alike. -/
theorem every_secp256k1_spelling_names_its_key (key : Key) (base : Base) (point : Point) :
    Names (.secp256k1, key) ⟨.secp256k1, key, base, point⟩ := by
  -- vera.rs takes every secp256k1 spelling, so the check computes to true.
  simp [Names, canonicalActor, veraTakes]

/-- A base64url spelling of an Ed25519 or P-256 key names nothing: vera.rs
refuses it, so the node does not issue under it. -/
theorem base64url_of_another_type_names_nothing
    (own : KeyType × Key) (type : KeyType) (key : Key) (point : Point)
    (other : type ≠ .secp256k1) :
    ¬ Names own ⟨type, key, .base64url, point⟩ := by
  -- Only secp256k1 is taken in base64url; the other two cases are refused.
  cases type <;> simp_all [Names, canonicalActor, veraTakes]

/-- No spelling of one key names another key. -/
theorem another_key_is_never_named (own : KeyType × Key) (issuer : Spelling Key)
    (different : (issuer.type, issuer.key) ≠ own) :
    ¬ Names own issuer :=
  -- The check would need the issuer's key to be the signer's.
  fun named => different ((names_iff_vera_takes_own_key own issuer).mp named).2

/-- The check before the fix: only the two base58btc spellings of a secp256k1
key, and the base58btc compressed spelling of the other types. -/
def oldNames (own : KeyType × Key) (issuer : Spelling Key) : Prop :=
  -- base58btc only, and for the other types the compressed point only.
  issuer.base = .base58btc ∧ (issuer.type = .secp256k1 ∨ issuer.point = .compressed) ∧
    (issuer.type, issuer.key) = own

/-- The old check refused a spelling vera.rs takes for the signer's key: the
base64url spelling of a secp256k1 key. This is the bug the fix removes. -/
theorem old_check_refused_a_taken_spelling (key : Key) :
    Names (.secp256k1, key) ⟨.secp256k1, key, .base64url, .compressed⟩ ∧
      ¬ oldNames (.secp256k1, key) ⟨.secp256k1, key, .base64url, .compressed⟩ := by
  -- vera.rs takes it, and the old check wants base58btc.
  simp [Names, canonicalActor, veraTakes, oldNames]

/-! The spellings the provider writes, over the multibase as well as the point.
`did_spelling::defra` and `did_spelling::wire` always write base58btc. -/

/-- The spelling DefraDB keeps for a key: base58btc, the uncompressed point of
a secp256k1 or P-256 key, and an Ed25519 key's one encoding. -/
def defra (did : Spelling Key) : Spelling Key :=
  -- The key and its type stay; the multibase and point are fixed per type.
  ⟨did.type, did.key, .base58btc, if did.type = .ed25519 then .compressed else .uncompressed⟩

/-- The spelling the provider sends vera.rs: base58btc, uncompressed for a
secp256k1 key, compressed for a P-256 key, and an Ed25519 key's one encoding. -/
def wire (did : Spelling Key) : Spelling Key :=
  -- Only a secp256k1 key is sent uncompressed; every key in base58btc.
  ⟨did.type, did.key, .base58btc, if did.type = .secp256k1 then .uncompressed else .compressed⟩

/-- Whatever spelling names the signer, the issuer the provider writes for it
is one vera.rs takes and names the signer's key: a signer named in base64url
issues as one named in base58btc does. -/
theorem wire_names_the_signer (did : Spelling Key) :
    Names (did.type, did.key) (wire did) := by
  -- By key type: each written spelling is one vera.rs takes.
  cases h : did.type <;> simp [Names, canonicalActor, veraTakes, wire, h]

/-- Two spellings are kept as one exactly when they name the same key of the
same type, in whichever multibase and point each is written. -/
theorem defra_one_spelling_per_key (a b : Spelling Key) :
    defra a = defra b ↔ a.type = b.type ∧ a.key = b.key := by
  -- Equal kept spellings agree on type and key, and type and key decide the rest.
  constructor
  -- One kept spelling: compare the two fields by field.
  · intro same
    -- The kept spellings are equal field by field.
    simp only [defra, Spelling.mk.injEq] at same
    -- Their first two fields are the type and the key.
    exact ⟨same.1, same.2.1⟩
  -- One type and key: the kept spellings compute to the same value.
  · intro ⟨type, key⟩
    -- Rewrite with the equal type and key and both sides agree.
    simp [defra, type, key]

/-- Keeping a spelling twice changes nothing. -/
theorem defra_idempotent (did : Spelling Key) : defra (defra did) = defra did := by
  -- The type and key are unchanged, so the fixed multibase and point repeat.
  simp [defra]

end VeraIssuerSpelling
