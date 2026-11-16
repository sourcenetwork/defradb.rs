/-! What lets a node send a policy command to a remote ACP provider for the
signer of a verified write.

The story: a User signs a write, an embedder verifies it and hands it to a
node whose document ACP is a remote provider. The node sends the provider a
command for the User only when it holds the User's key, or the User signed a
delegation for this node, or the User signed a delegation bound to exactly
that command. Nothing else counts, and a delegation bound to one command
counts for no other.

The provider's own verification of a token (vera.rs `authorize_delegation`)
and the signature check are outside this file: `signedBySigner`, `forThisNode`
and `current` are what those checks found. -/
namespace VerifiedRemoteAuthority

/-- The form a credential comes in. -/
inductive Form where
  /-- Nothing was handed in. -/
  | none
  /-- "This node may send my policy commands." -/
  | delegation
  /-- "This node may send exactly the command `names`." -/
  | request (names : Nat)
  deriving DecidableEq

/-- What the node knows about one credential and its signer. -/
structure Credential where
  /-- The form it comes in. -/
  form : Form
  /-- The User signed it. -/
  signedBySigner : Bool
  /-- It names this node and its deployment. -/
  forThisNode : Bool
  /-- It has not expired. -/
  current : Bool

/-- A credential that passed the three checks every form needs. -/
def good (c : Credential) : Bool :=
  c.signedBySigner && c.forThisNode && c.current

/-- Whether the node may send `command` for the User: it holds the key, or
the credential is a good general delegation, or a good one bound to exactly
`command`. -/
def maySend (holdsKey : Bool) (c : Credential) (command : Nat) : Bool :=
  holdsKey ||
    match c.form with
    | .none => false
    | .delegation => good c
    | .request names => good c && names == command

/-- Whether the node may act for the User in a write that sends no command:
a request-bound delegation has nothing to name there. -/
def mayAct (holdsKey : Bool) (c : Credential) : Bool :=
  holdsKey ||
    match c.form with
    | .delegation => good c
    | _ => false

/-- With no key and no credential the node sends nothing. -/
theorem nothing_without_authority (c : Credential) (command : Nat)
    (h : c.form = .none) : maySend false c command = false := by
  simp [maySend, h]

/-- A key the node holds is enough, whatever came with the write. -/
theorem a_held_key_is_enough (c : Credential) (command : Nat) :
    maySend true c command = true := by
  simp [maySend]

/-- Without the key, a credential somebody else signed sends nothing. -/
theorem a_foreign_signature_sends_nothing (c : Credential) (command : Nat)
    (h : c.signedBySigner = false) : maySend false c command = false := by
  cases hf : c.form <;> simp [maySend, good, h, hf]

/-- Without the key, a credential made for another node sends nothing. -/
theorem another_nodes_delegation_sends_nothing (c : Credential) (command : Nat)
    (h : c.forThisNode = false) : maySend false c command = false := by
  cases hf : c.form <;> simp [maySend, good, h, hf]

/-- Without the key, an expired credential sends nothing. -/
theorem an_expired_credential_sends_nothing (c : Credential) (command : Nat)
    (h : c.current = false) : maySend false c command = false := by
  cases hf : c.form <;> simp [maySend, good, h, hf]

/-- A request-bound delegation sends no command but the one it names. -/
theorem a_request_sends_only_its_command (c : Credential) (names command : Nat)
    (hf : c.form = .request names) (hne : names ≠ command) :
    maySend false c command = false := by
  simp [maySend, hf, hne]

/-- A request-bound delegation that the node may send under names that
command. -/
theorem a_sent_request_names_its_command (c : Credential) (names command : Nat)
    (hf : c.form = .request names) (h : maySend false c command = true) :
    names = command := by
  simp [maySend, hf] at h
  exact h.2

/-- A request-bound delegation never lets the node act where no command is
sent: an update, or a caller logging in. -/
theorem a_request_is_no_general_authority (c : Credential) (names : Nat)
    (hf : c.form = .request names) : mayAct false c = false := by
  simp [mayAct, hf]

/-- Whatever lets the node act in general lets it send any one command. -/
theorem general_authority_covers_every_command (holdsKey : Bool) (c : Credential)
    (command : Nat) (h : mayAct holdsKey c = true) :
    maySend holdsKey c command = true := by
  cases hk : holdsKey
  · cases hf : c.form <;> simp_all [mayAct, maySend]
  · simp [maySend]

/-- Whether a command the node could not send is owed again when a fresh
credential is handed in for it: the question asked when its write was
admitted, asked of the fresh credential about that same command. There is no
second rule. -/
def reauthorized (holdsKey : Bool) (fresh : Credential) (command : Nat) : Bool :=
  maySend holdsKey fresh command

/-- A fresh credential is taken exactly when it would have admitted the
command. -/
theorem a_fresh_credential_is_taken_as_at_admission (holdsKey : Bool)
    (fresh : Credential) (command : Nat) :
    reauthorized holdsKey fresh command = maySend holdsKey fresh command := rfl

/-- A fresh request-bound delegation that names another command leaves the
owed command where it was. -/
theorem a_fresh_request_for_another_command_is_refused (fresh : Credential)
    (names command : Nat) (hf : fresh.form = .request names) (hne : names ≠ command) :
    reauthorized false fresh command = false :=
  a_request_sends_only_its_command fresh names command hf hne

/-- A fresh credential somebody else signed leaves it where it was too. -/
theorem a_fresh_credential_of_another_signer_is_refused (fresh : Credential)
    (command : Nat) (h : fresh.signedBySigner = false) :
    reauthorized false fresh command = false :=
  a_foreign_signature_sends_nothing fresh command h

/-- One fresh request-bound delegation makes at most one command owed again:
the registration's does not release the grant staged beside it. -/
theorem a_fresh_request_releases_one_command (fresh : Credential)
    (names first second : Nat) (hf : fresh.form = .request names)
    (h1 : reauthorized false fresh first = true)
    (h2 : reauthorized false fresh second = true) : first = second := by
  have a := a_sent_request_names_its_command fresh names first hf h1
  have b := a_sent_request_names_its_command fresh names second hf h2
  omega

end VerifiedRemoteAuthority
