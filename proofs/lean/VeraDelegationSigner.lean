/-!
Which signatures leave the node in a vera.rs delegation token.

An actor's key signs the delegations its node sends to vera.rs. The key may sit
outside the process, so the provider treats what a signer returns as untrusted
and looks at it before it becomes part of a token. vera.rs refuses a
delegation whose signature is not 64 bytes, is not two scalars of the curve
order, has `s` in the upper half of that order, or does not verify under the
key the issuer DID names. The provider runs the same four tests, in that order.

What must never happen: a token leaves the node with a signature vera.rs would
refuse, or the provider refuses a signature vera.rs would accept.

Rust owner: `admit` in `crates/vera/src/vera_rs/signer/admission.rs`, called by
`issue_at` in `crates/vera/src/vera_rs/bearer.rs`. The verifier it mirrors is
`verify_bearer_token` in vera.rs `vera-crypto`.

The model is over what the tests observe of a signature. That ECDSA
verification itself is sound is assumed, not modeled.
-/
namespace VeraDelegationSigner

/-- What the tests can observe of the bytes a signer returned. -/
structure Answer where
  /-- How many bytes the signer returned. -/
  length : Nat
  /-- Both halves are scalars of the curve order. -/
  scalars : Bool
  /-- `s` is in the lower half of the curve order. -/
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
def admit (answer : Answer) : Option Refusal :=
  -- A signature of any other length is not an ES256K signature.
  if answer.length ≠ 64 then some .length
  -- Zero, or a value at or above the curve order, is not a signature half.
  else if answer.scalars = false then some .scalar
  -- vera.rs accepts one of the two forms of a signature, the one with low `s`.
  else if answer.lowS = false then some .highS
  -- The signature has to be the issuer's, over this token.
  else if answer.verifies = false then some .unverified
  -- Every test passed.
  else none

/-- What vera.rs's verifier accepts of a signature: all four tests at once. -/
def veraAccepts (answer : Answer) : Bool :=
  answer.length == 64 && answer.scalars && answer.lowS && answer.verifies

/-- The token the provider builds from an answer: one that carries the answer
when it is admitted, and none otherwise. -/
def token (answer : Answer) : Option Answer :=
  match admit answer with
  | none => some answer
  | some _ => none

/-- The provider admits an answer exactly when vera.rs would accept it: nothing
vera.rs refuses is admitted, and nothing vera.rs accepts is refused. -/
theorem admitted_iff_vera_accepts (answer : Answer) :
    admit answer = none ↔ veraAccepts answer = true := by
  unfold admit veraAccepts
  cases answer.scalars <;> cases answer.lowS <;> cases answer.verifies <;>
    by_cases h : answer.length = 64 <;> simp [h]

/-- A token that leaves the node carries a signature vera.rs accepts. -/
theorem sent_is_accepted (answer sent : Answer) (h : token answer = some sent) :
    veraAccepts sent = true := by
  unfold token at h
  cases admitted : admit answer with
  | none =>
    rw [admitted] at h
    cases h
    exact (admitted_iff_vera_accepts answer).mp admitted
  | some reason =>
    rw [admitted] at h
    cases h

/-- A refused answer never becomes a token. -/
theorem refused_is_not_sent (answer : Answer) (reason : Refusal)
    (h : admit answer = some reason) : token answer = none := by
  unfold token
  rw [h]

/-- An answer of the right length and form that verifies is always admitted:
an honest signer is never refused. -/
theorem honest_answer_admitted (answer : Answer) (length : answer.length = 64)
    (scalars : answer.scalars = true) (lowS : answer.lowS = true)
    (verifies : answer.verifies = true) : admit answer = none := by
  simp [admit, length, scalars, lowS, verifies]

/-- Each refusal names the first test the answer fails, so the error the
provider reports is the true reason. -/
theorem refusal_names_the_first_failure (answer : Answer) :
    (admit answer = some .length ↔ answer.length ≠ 64) ∧
    (admit answer = some .scalar ↔ answer.length = 64 ∧ answer.scalars = false) ∧
    (admit answer = some .highS ↔
      answer.length = 64 ∧ answer.scalars = true ∧ answer.lowS = false) ∧
    (admit answer = some .unverified ↔
      answer.length = 64 ∧ answer.scalars = true ∧ answer.lowS = true ∧
        answer.verifies = false) := by
  unfold admit
  cases answer.scalars <;> cases answer.lowS <;> cases answer.verifies <;>
    by_cases h : answer.length = 64 <;> simp [h]

end VeraDelegationSigner
