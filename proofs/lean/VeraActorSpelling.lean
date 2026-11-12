/-!
One spelling per secp256k1 actor on DefraDB's side of the vera.rs boundary.

A secp256k1 `did:key` can be written from the compressed or the uncompressed
public key. vera.rs takes every spelling a request uses as one actor and
records and returns it under the compressed one. DefraDB derives the
uncompressed one and compares actors as strings, so the provider respells
every such actor it sends or returns, and compares a DID read from vera.rs
with its own only after respelling both. Other key types and identifiers that
are not keys pass through.

What must never happen: one actor is split in two because vera.rs and DefraDB
spell it differently, or two actors are taken for one.

Rust owner: `defra` and `same_actor` in
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
no actor is split in two, and no two actors are merged. This is the test
`same_actor` makes when it compares a DID vera.rs returned with the
provider's own. -/
theorem one_spelling_per_actor (a b : Did Key Other) :
    defra a = defra b ↔ SameActor a b := by
  cases a <;> cases b <;> simp [defra, SameActor]

end VeraActorSpelling
