/-!
Which signatures leave the node in a vera.rs delegation token.

An actor's key signs the delegations its node sends to vera.rs. The key may sit
outside the process, so the provider treats what a signer returns as untrusted
and looks at it before it becomes part of a token. The key is a secp256k1 or
P-256 key, which signs with ECDSA, or an Ed25519 key.

vera.rs refuses a delegation whose signature is not 64 bytes or does not
verify under the key the issuer DID names. Under an ECDSA key it also refuses
one that is not two scalars of the curve order, or has `s` in the upper half
of that order. The provider runs the same tests, in that order: length, then
for an ECDSA key the scalars and the half of `s`, then verification.

What must never happen: a token leaves the node with a signature vera.rs would
refuse, or the provider refuses a signature vera.rs would accept.

Rust owner: `admit` in `crates/vera/src/vera_rs/signer/admission.rs`, called by
`issue_at` in `crates/vera/src/vera_rs/bearer.rs`. The verifier it mirrors is
`verify_bearer_token` in vera.rs `vera-crypto`.

The model is over what the tests observe of a signature. That ECDSA and
Ed25519 verification themselves are sound is assumed, not modeled.
-/
namespace VeraDelegationSigner

/-- How the issuer's key signs. -/
inductive Key where
  /-- secp256k1 (ES256K) or P-256 (ES256). -/
  | ecdsa
  /-- Ed25519 (EdDSA). -/
  | ed25519
  deriving DecidableEq

/-- What the tests can observe of the bytes a signer returned. -/
structure Answer where
  /-- How many bytes the signer returned. -/
  length : Nat
  /-- Both halves are scalars of the curve order. Read for an ECDSA key only. -/
  scalars : Bool
  /-- `s` is in the lower half of the curve order. Read for an ECDSA key only. -/
  lowS : Bool
  /-- The signature verifies over the signing input under the issuer's key. -/
  verifies : Bool

/-- Why an answer is refused, one reason per test. -/
inductive Refusal where
  | length
  | scalar
  | highS
  | unverified
  deriving DecidableEq

/-- The provider's decision on an answer: `none` admits it, `some reason`
refuses it at the first test it fails. -/
def admit (key : Key) (answer : Answer) : Option Refusal :=
  -- A signature of any other length is no signature of either kind.
  if answer.length ≠ 64 then some .length
  else match key with
    | .ecdsa =>
      -- Zero, or a value at or above the curve order, is not a signature half.
      if answer.scalars = false then some .scalar
      -- vera.rs accepts one of the two forms of a signature, the one with low `s`.
      else if answer.lowS = false then some .highS
      -- The signature has to be the issuer's, over this token.
      else if answer.verifies = false then some .unverified
      -- Every test passed.
      else none
    | .ed25519 =>
      -- Strict verification is the one test an Ed25519 signature has left.
      if answer.verifies = false then some .unverified
      else none

/-- What vera.rs's verifier accepts of a signature: every test of the key's
kind at once. -/
def veraAccepts (key : Key) (answer : Answer) : Bool :=
  match key with
  | .ecdsa => answer.length == 64 && answer.scalars && answer.lowS && answer.verifies
  | .ed25519 => answer.length == 64 && answer.verifies

/-- The token the provider builds from an answer: one that carries the answer
when it is admitted, and none otherwise. -/
def token (key : Key) (answer : Answer) : Option Answer :=
  match admit key answer with
  | none => some answer
  | some _ => none

/-- The provider admits an answer exactly when vera.rs would accept it: nothing
vera.rs refuses is admitted, and nothing vera.rs accepts is refused. -/
theorem admitted_iff_vera_accepts (key : Key) (answer : Answer) :
    admit key answer = none ↔ veraAccepts key answer = true := by
  unfold admit veraAccepts
  cases key <;> cases answer.scalars <;> cases answer.lowS <;> cases answer.verifies <;>
    by_cases h : answer.length = 64 <;> simp [h]

/-- A token that leaves the node carries a signature vera.rs accepts. -/
theorem sent_is_accepted (key : Key) (answer sent : Answer)
    (h : token key answer = some sent) : veraAccepts key sent = true := by
  unfold token at h
  cases admitted : admit key answer with
  | none =>
    rw [admitted] at h
    cases h
    exact (admitted_iff_vera_accepts key answer).mp admitted
  | some reason =>
    rw [admitted] at h
    cases h

/-- A refused answer never becomes a token. -/
theorem refused_is_not_sent (key : Key) (answer : Answer) (reason : Refusal)
    (h : admit key answer = some reason) : token key answer = none := by
  unfold token
  rw [h]

/-- An ECDSA answer of the right length and form that verifies is always
admitted: an honest signer is never refused. -/
theorem honest_answer_admitted (answer : Answer) (length : answer.length = 64)
    (scalars : answer.scalars = true) (lowS : answer.lowS = true)
    (verifies : answer.verifies = true) : admit .ecdsa answer = none := by
  simp [admit, length, scalars, lowS, verifies]

/-- An Ed25519 answer of the right length that verifies is always admitted,
whatever the two ECDSA observations read. -/
theorem honest_ed25519_answer_admitted (answer : Answer) (length : answer.length = 64)
    (verifies : answer.verifies = true) : admit .ed25519 answer = none := by
  simp [admit, length, verifies]

/-- An Ed25519 answer is never refused for a reason that belongs to ECDSA. -/
theorem ed25519_refusal_is_length_or_verification (answer : Answer) :
    admit .ed25519 answer ≠ some .scalar ∧ admit .ed25519 answer ≠ some .highS := by
  unfold admit
  cases answer.verifies <;> by_cases h : answer.length = 64 <;> simp [h]

/-- Each refusal of an ECDSA answer names the first test the answer fails, so
the error the provider reports is the true reason. -/
theorem refusal_names_the_first_failure (answer : Answer) :
    (admit .ecdsa answer = some .length ↔ answer.length ≠ 64) ∧
    (admit .ecdsa answer = some .scalar ↔ answer.length = 64 ∧ answer.scalars = false) ∧
    (admit .ecdsa answer = some .highS ↔
      answer.length = 64 ∧ answer.scalars = true ∧ answer.lowS = false) ∧
    (admit .ecdsa answer = some .unverified ↔
      answer.length = 64 ∧ answer.scalars = true ∧ answer.lowS = true ∧
        answer.verifies = false) := by
  unfold admit
  cases answer.scalars <;> cases answer.lowS <;> cases answer.verifies <;>
    by_cases h : answer.length = 64 <;> simp [h]

/-- Each refusal of an Ed25519 answer names the first test the answer fails. -/
theorem ed25519_refusal_names_the_first_failure (answer : Answer) :
    (admit .ed25519 answer = some .length ↔ answer.length ≠ 64) ∧
    (admit .ed25519 answer = some .unverified ↔
      answer.length = 64 ∧ answer.verifies = false) := by
  unfold admit
  cases answer.verifies <;> by_cases h : answer.length = 64 <;> simp [h]

end VeraDelegationSigner
