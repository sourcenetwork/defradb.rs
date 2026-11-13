/-!
One spelling per actor on each side of the vera.rs boundary.

A secp256k1 or P-256 `did:key` can be written from the compressed or the
uncompressed public key. DefraDB derives the uncompressed one and compares
actors as strings. vera.rs takes every spelling of a secp256k1 key as one
actor and records and returns it under the compressed one; it takes a P-256
key under the compressed spelling only and compares it as written.

The provider therefore keeps and returns every such actor under DefraDB's
spelling (`defra`), compares a DID read from vera.rs with its own only after
respelling both, and writes a DID it sends in a spelling vera.rs takes
(`wire`). Other key types and identifiers that are not keys pass through.

What must never happen: one actor is split in two because vera.rs and DefraDB
spell it differently, two actors are taken for one, or vera.rs is sent a
spelling it refuses.

Rust owner: `defra`, `wire` and `same_actor` in
`crates/vera/src/vera_rs/did_spelling.rs`.

The model is over an abstract key. The constructors are distinct strings in
Rust; that two different keys never share a spelling is the encoding's
injectivity, assumed here.
-/
namespace VeraActorSpelling

/-- The curves whose points have two encodings. -/
inductive Curve where
  -- secp256k1: vera.rs takes a key of it in either encoding.
  | secp256k1
  -- P-256: vera.rs takes a key of it in the compressed encoding only.
  | p256
  deriving DecidableEq

/-- An identifier as the provider decodes it. -/
inductive Did (Key Other : Type) where
  -- A `did:key` written from the compressed point of `key`.
  | compressed (curve : Curve) (key : Key)
  -- A `did:key` written from the uncompressed point of `key`.
  | uncompressed (curve : Curve) (key : Key)
  -- Anything else: an Ed25519 key, or an identifier that is not a key.
  | other (id : Other)

variable {Key Other : Type}

/-- The spelling the provider uses for the actor `did` names. -/
def defra : Did Key Other → Did Key Other
  -- A compressed spelling becomes the uncompressed one of the same key.
  | .compressed curve key => .uncompressed curve key
  -- An uncompressed spelling and every other identifier stay as they are.
  | did => did

/-- The spelling the provider sends vera.rs for the actor `did` names:
DefraDB's for a secp256k1 key, the compressed one for a P-256 key. -/
def wire : Did Key Other → Did Key Other
  -- A secp256k1 key is sent in the spelling DefraDB keeps.
  | .compressed .secp256k1 key => .uncompressed .secp256k1 key
  -- A P-256 key is sent compressed, the one spelling vera.rs takes.
  | .uncompressed .p256 key => .compressed .p256 key
  -- What is already in its sent spelling, and every other identifier, stays.
  | did => did

/-- The spellings vera.rs takes for an actor: any of a secp256k1 key, the
compressed one of a P-256 key, and every other identifier as written. -/
def VeraTakes : Did Key Other → Prop
  -- An uncompressed P-256 key is the one spelling vera.rs refuses.
  | .uncompressed .p256 _ => False
  -- Everything else is taken as written.
  | _ => True

/-- Two identifiers name the same actor: one key of one curve, or the same
other identifier. -/
def SameActor : Did Key Other → Did Key Other → Prop
  -- Two keys are one actor when curve and key agree, whatever the encodings.
  | .compressed c a, .compressed d b => c = d ∧ a = b
  | .compressed c a, .uncompressed d b => c = d ∧ a = b
  | .uncompressed c a, .compressed d b => c = d ∧ a = b
  | .uncompressed c a, .uncompressed d b => c = d ∧ a = b
  -- Two other identifiers are one actor when they are the same identifier.
  | .other a, .other b => a = b
  -- A key and an identifier that is not a key are never one actor.
  | _, _ => False

/-- Respelling an already respelled identifier changes nothing. -/
theorem defra_idempotent (did : Did Key Other) : defra (defra did) = defra did := by
  -- Each kind of identifier computes to the same value on both sides.
  cases did <;> rfl

/-- Both spellings of one key meet at the uncompressed one. -/
theorem spellings_meet (curve : Curve) (key : Key) :
    defra (Did.compressed curve key : Did Key Other) = .uncompressed curve key ∧
      defra (Did.uncompressed curve key : Did Key Other) = .uncompressed curve key :=
  -- Both sides compute to the uncompressed spelling.
  ⟨rfl, rfl⟩

/-- Anything that is not a secp256k1 or P-256 key is returned as given. -/
theorem other_unchanged (id : Other) :
    defra (Did.other id : Did Key Other) = .other id ∧
      wire (Did.other id : Did Key Other) = .other id :=
  -- Neither function has a case for it but the last, which returns it.
  ⟨rfl, rfl⟩

/-- The respelled identifier names the same actor as the original. -/
theorem defra_same_actor (did : Did Key Other) : SameActor did (defra did) := by
  -- For each kind, unfold both definitions: the curve and key are unchanged.
  cases did <;> simp [defra, SameActor]

/-- Two identifiers get one spelling exactly when they name the same actor:
no actor is split in two, and no two actors are merged. This is the test
`same_actor` makes when it compares a DID vera.rs returned with the
provider's own. -/
theorem one_spelling_per_actor (a b : Did Key Other) :
    defra a = defra b ↔ SameActor a b := by
  -- Every pair of kinds: equal respellings are exactly equal curve and key.
  cases a <;> cases b <;> simp [defra, SameActor]

/-- What the provider sends is a spelling vera.rs takes. -/
theorem wire_is_taken (did : Did Key Other) : VeraTakes (wire did) := by
  -- By the kind of identifier, and for a key by its curve.
  cases did with
  -- A compressed key: sent uncompressed for secp256k1, as it is for P-256.
  | compressed curve key => cases curve <;> simp [wire, VeraTakes]
  -- An uncompressed key: sent as it is for secp256k1, compressed for P-256.
  | uncompressed curve key => cases curve <;> simp [wire, VeraTakes]
  -- Any other identifier is sent as it is.
  | other id => simp [wire, VeraTakes]

/-- What the provider sends names the actor it was asked about. -/
theorem wire_same_actor (did : Did Key Other) : SameActor did (wire did) := by
  -- By the kind of identifier, and for a key by its curve.
  cases did with
  -- A compressed key: sent uncompressed for secp256k1, as it is for P-256.
  | compressed curve key => cases curve <;> simp [wire, SameActor]
  -- An uncompressed key: sent as it is for secp256k1, compressed for P-256.
  | uncompressed curve key => cases curve <;> simp [wire, SameActor]
  -- Any other identifier is sent as it is.
  | other id => simp [wire, SameActor]

/-- An actor sent to vera.rs and read back is the actor the provider keeps:
the two spellings never drift apart. -/
theorem defra_of_wire (did : Did Key Other) : defra (wire did) = defra did := by
  -- By the kind of identifier, and for a key by its curve.
  cases did with
  -- Either curve: both sides compute to the uncompressed spelling.
  | compressed curve key => cases curve <;> rfl
  -- Either curve: both sides compute to the uncompressed spelling.
  | uncompressed curve key => cases curve <;> rfl
  -- Neither function changes another identifier.
  | other id => rfl

/-- A secp256k1 actor is sent as it is kept, so the tokens and commands built
for it are the ones built before P-256 actors existed. -/
theorem wire_secp256k1_is_defra (key : Key) :
    wire (Did.compressed .secp256k1 key : Did Key Other) = defra (.compressed .secp256k1 key) ∧
      wire (Did.uncompressed .secp256k1 key : Did Key Other) =
        defra (.uncompressed .secp256k1 key) :=
  -- Both functions compute to the uncompressed spelling of the key.
  ⟨rfl, rfl⟩

end VeraActorSpelling
