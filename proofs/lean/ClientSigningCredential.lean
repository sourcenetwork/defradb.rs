/-!
A client keeps its key and authenticates a signing socket with either an
identity token or a delegation to this node's worker and deployment. The edge
verifies that credential once. The socket carries the verified caller and its
expiration; it never demands a second credential format. The node's ordinary
write permission and the signing-key match still decide the write.

Rust owners: `http::identity_extractor`, `http::auth_middleware`,
`http::client_signing`, and `vera::vera_rs::bearer`.
-/
namespace ClientSigningCredential

/-- The two credentials accepted by their existing verification owners. -/
inductive Kind where
  | identityToken | callerDelegation
  deriving DecidableEq

/-- The verified credential binds its caller and absolute expiration. -/
structure Verified where
  kind : Kind
  caller : Nat
  expires : Nat

/-- A socket admits the verified caller only while its credential is current
and native node access permits document updates. -/
def admitted (credential : Verified) (now : Nat) (canWrite : Bool) : Bool :=
  decide (now < credential.expires) && canWrite

/-- Credential format never changes an otherwise identical admission. -/
theorem both_formats_admit_the_same_caller (caller expires now : Nat) (canWrite : Bool) :
    admitted ⟨.identityToken, caller, expires⟩ now canWrite =
      admitted ⟨.callerDelegation, caller, expires⟩ now canWrite := rfl

/-- Expiration prevents a new signing dispatch for either credential. -/
theorem expired_never_dispatches (credential : Verified) (now : Nat) (canWrite : Bool)
    (expired : credential.expires ≤ now) : admitted credential now canWrite = false := by
  simp [admitted, Nat.not_lt.mpr expired]

/-- Native write denial cannot be bypassed by a valid delegation. -/
theorem permission_denied_never_dispatches (credential : Verified) (now : Nat) :
    admitted credential now false = false := by simp [admitted]

/-- A connection signer verifying the caller's own signatures needs no
signing-service delegation. Other remote signers retain their policy decision. -/
def needsPolicyDecision (remote callerKey : Bool) : Bool := remote && !callerKey

/-- A client signs its own blocks without a synthetic collection-wide grant. -/
theorem client_key_needs_no_service_grant (remote : Bool) :
    needsPolicyDecision remote true = false := by simp [needsPolicyDecision]

/-- An external signing service keeps its existing policy authorization. -/
theorem service_keeps_policy_decision : needsPolicyDecision true false = true := rfl

end ClientSigningCredential
