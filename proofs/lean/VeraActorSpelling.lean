/-!
One spelling per secp256k1 actor at the vera.rs boundary.

A secp256k1 `did:key` can be written from the compressed or the uncompressed
public key. vera.rs compares actors as strings, so the provider sends and
returns every such actor under one spelling: the uncompressed one DefraDB
derives. Other key types and identifiers that are not keys pass through.

Rust owner: `defra` and `issuer_is_canonical` in
`crates/vera/src/vera_rs/did_spelling.rs`.

The model is over an abstract key. The three constructors are distinct strings
in Rust; that two different keys never share a spelling is the encoding's
injectivity, assumed here.
-/
namespace VeraActorSpelling

/-- An identifier as the provider decodes it. -/
inductive Did (Key Other : Type) where
  | compressed (key : Key)
  | uncompressed (key : Key)
  | other (id : Other)

variable {Key Other : Type}

/-- The spelling the provider uses for the actor `did` names. -/
def defra : Did Key Other → Did Key Other
  | .compressed key => .uncompressed key
  | did => did

/-- Two identifiers name the same actor: one key, or the same other identifier. -/
def SameActor : Did Key Other → Did Key Other → Prop
  | .compressed a, .compressed b => a = b
  | .compressed a, .uncompressed b => a = b
  | .uncompressed a, .compressed b => a = b
  | .uncompressed a, .uncompressed b => a = b
  | .other a, .other b => a = b
  | _, _ => False

/-- Respelling an already respelled identifier changes nothing. -/
theorem defra_idempotent (did : Did Key Other) : defra (defra did) = defra did := by
  cases did <;> rfl

/-- Both spellings of one key meet at the uncompressed one. -/
theorem spellings_meet (key : Key) :
    defra (Did.compressed key : Did Key Other) = .uncompressed key ∧
      defra (Did.uncompressed key : Did Key Other) = .uncompressed key :=
  ⟨rfl, rfl⟩

/-- Anything that is not a secp256k1 key is returned as given. -/
theorem other_unchanged (id : Other) : defra (Did.other id : Did Key Other) = .other id :=
  rfl

/-- The respelled identifier names the same actor as the original. -/
theorem defra_same_actor (did : Did Key Other) : SameActor did (defra did) := by
  cases did <;> simp [defra, SameActor]

/-- Two identifiers get one spelling exactly when they name the same actor:
no actor is split in two, and no two actors are merged. -/
theorem one_spelling_per_actor (a b : Did Key Other) :
    defra a = defra b ↔ SameActor a b := by
  cases a <;> cases b <;> simp [defra, SameActor]

/-- A delegation token: its issuer, when the token decodes. -/
abbrev Token (Key Other : Type) := Option (Did Key Other)

/-- Whether a token names its issuer as the provider names actors. A token
that does not decode is left for vera.rs to reject. -/
def issuerIsCanonical : Token Key Other → Bool
  | some (.compressed _) => false
  | _ => true

/-- A token is accepted exactly when it does not decode or its issuer is
already in the provider's spelling. -/
theorem issuer_canonical_iff (token : Token Key Other) :
    issuerIsCanonical token = true ↔
      token = none ∨ ∃ issuer, token = some issuer ∧ defra issuer = issuer := by
  cases token with
  | none => simp [issuerIsCanonical]
  | some issuer => cases issuer <;> simp [issuerIsCanonical, defra]

/-- The compressed spelling of a key is refused; the uncompressed one is not. -/
theorem compressed_issuer_refused (key : Key) :
    issuerIsCanonical (some (Did.compressed key) : Token Key Other) = false ∧
      issuerIsCanonical (some (Did.uncompressed key) : Token Key Other) = true :=
  ⟨rfl, rfl⟩

end VeraActorSpelling
