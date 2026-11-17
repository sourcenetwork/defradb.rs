/-! What lets a node send a policy command to a remote ACP provider for the
signer of a verified write.

The story: a User signs a write, an embedder verifies it and hands it to a
node whose document ACP is a remote provider. The node sends the provider a
command for the User only when it holds the User's key, or the User signed a
delegation for this node, or the User signed a delegation bound to exactly
that command. Nothing else counts, and a delegation bound to one command
counts for no other.

An update or a delete sends the provider no command. The User's signature on
the blocks says who wrote it; the credential only says who is asking, and the
write is admitted when the User held the permission at a verified revision no
older than the freshness limit: a proof of the provider's finalized state at
that revision. It says nothing of a change finalized after that revision, so a
revocation finalized since can still be passed until a newer revision is
verified or the proof ages out. Stale or missing evidence admits nothing. On this node a registration
still owed lets only its creator in, and a revocation still owed shuts the
User out, as for a read. One rule decides for every credential form.

The provider's own verification of a token (vera.rs `authorize_delegation`)
and the signature check are outside this file: `signedBySigner`, `forThisNode`
and `current` are what those checks found, and `Evidence.fresh allowed` is what
the light client verified. -/
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
  /-- It names this node and its deployment and, when the node knows it,
  the deployment's genesis identity. -/
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

/-! ## Updates and deletes -/

/-- What the node could read about the User's permission when the write came
in. -/
inductive Evidence where
  /-- Verified against the provider's finalized state at a revision within
  the freshness limit; `allowed` is the permission at that revision, which a
  change finalized later does not alter. -/
  | fresh (allowed : Bool)
  /-- The newest verified state is older than the freshness limit. -/
  | stale
  /-- The provider did not answer. -/
  | unreachable
  deriving DecidableEq

/-- What this node still owes the provider about the document. -/
structure OwedOnDocument where
  /-- A registration is owed: `some true` when its recorded creator is the
  User, `some false` when it is somebody else, `none` when none is owed. -/
  registration : Option Bool
  /-- A revocation owed on this node names the User on the document. -/
  revocation : Bool

/-- The credential of an update only says who is asking. Nothing, when the
provider proves its decisions or the node holds the key; a good delegation (a
held key answers for it, as for any command). A request-bound delegation names
a command, an update sends none: refused, key or no key. -/
def editCredential (proven holdsKey : Bool) (c : Credential) : Bool :=
  match c.form with
  | .none => proven || holdsKey
  | .delegation => holdsKey || good c
  | .request _ => false

/-- The permission, as the gate decides it for a read: an owed registration
lets only its creator in, an owed revocation shuts the User out, and
otherwise only fresh evidence that allows it counts. -/
def verifiedPermit (owed : OwedOnDocument) (e : Evidence) : Bool :=
  match owed.registration with
  | some creator => creator
  | none =>
    !owed.revocation &&
      match e with
      | .fresh allowed => allowed
      | _ => false

/-- Whether a verified update or delete is admitted: its credential, and the
permission. The same rule for every credential form. -/
def editAdmitted (proven holdsKey : Bool) (c : Credential) (owed : OwedOnDocument)
    (e : Evidence) : Bool :=
  editCredential proven holdsKey c && verifiedPermit owed e

/-- One rule: the credential and the permission, for every form. -/
theorem one_rule_for_every_form (proven holdsKey : Bool) (c : Credential)
    (owed : OwedOnDocument) (e : Evidence) :
    editAdmitted proven holdsKey c owed e =
      (editCredential proven holdsKey c && verifiedPermit owed e) := rfl

/-- An admitted update or delete had the permission. -/
theorem an_edit_needs_the_permission (proven holdsKey : Bool) (c : Credential)
    (owed : OwedOnDocument) (e : Evidence)
    (h : editAdmitted proven holdsKey c owed e = true) :
    verifiedPermit owed e = true := by
  simp [editAdmitted] at h
  exact h.2

/-- With nothing owed on the document, an admitted update or delete was
admitted on fresh evidence whose revision allowed it: never on a guess. -/
theorem an_edit_is_admitted_on_fresh_evidence_only (proven holdsKey : Bool)
    (c : Credential) (owed : OwedOnDocument) (e : Evidence)
    (hr : owed.registration = none)
    (h : editAdmitted proven holdsKey c owed e = true) :
    e = .fresh true := by
  simp [editAdmitted, verifiedPermit, hr] at h
  obtain ⟨_, _, he⟩ := h
  cases e with
  | fresh allowed => simp_all
  | stale => simp at he
  | unreachable => simp at he

/-- Stale evidence admits no update or delete of a registered document. -/
theorem stale_evidence_admits_nothing (proven holdsKey : Bool) (c : Credential)
    (owed : OwedOnDocument) (hr : owed.registration = none) :
    editAdmitted proven holdsKey c owed .stale = false := by
  simp [editAdmitted, verifiedPermit, hr]

/-- A provider that does not answer admits nothing either: fail closed. -/
theorem an_unreachable_provider_admits_nothing (proven holdsKey : Bool)
    (c : Credential) (owed : OwedOnDocument) (hr : owed.registration = none) :
    editAdmitted proven holdsKey c owed .unreachable = false := by
  simp [editAdmitted, verifiedPermit, hr]

/-- A revocation contained in the verified revision of fresh evidence refuses
the update. -/
theorem a_revocation_in_the_verified_revision_refuses (proven holdsKey : Bool) (c : Credential)
    (owed : OwedOnDocument) (hr : owed.registration = none) :
    editAdmitted proven holdsKey c owed (.fresh false) = false := by
  simp [editAdmitted, verifiedPermit, hr]

/-- A revocation still owed on this node refuses the update whatever the
provider says. -/
theorem an_owed_revocation_refuses (proven holdsKey : Bool) (c : Credential)
    (owed : OwedOnDocument) (e : Evidence) (hr : owed.registration = none)
    (hv : owed.revocation = true) :
    editAdmitted proven holdsKey c owed e = false := by
  simp [editAdmitted, verifiedPermit, hr, hv]

/-- While its registration is owed, only the document's creator may update
or delete it. -/
theorem an_owed_registration_admits_only_its_creator (proven holdsKey : Bool)
    (c : Credential) (owed : OwedOnDocument) (e : Evidence)
    (hr : owed.registration = some false) :
    editAdmitted proven holdsKey c owed e = false := by
  simp [editAdmitted, verifiedPermit, hr]

/-- A request-bound delegation never admits an update or a delete, even
where the node holds the key. -/
theorem a_request_never_admits_an_edit (proven holdsKey : Bool) (c : Credential)
    (names : Nat) (owed : OwedOnDocument) (e : Evidence)
    (hf : c.form = .request names) :
    editAdmitted proven holdsKey c owed e = false := by
  simp [editAdmitted, editCredential, hf]

/-- With no credential, a provider that proves nothing still needs the key. -/
theorem no_credential_on_an_unproven_provider_needs_the_key (c : Credential)
    (owed : OwedOnDocument) (e : Evidence) (hf : c.form = .none) :
    editAdmitted false false c owed e = false := by
  simp [editAdmitted, editCredential, hf]

/-- With no credential on a provider that proves its decisions, the verified
permission alone admits the write. -/
theorem no_credential_is_admitted_on_the_permission (holdsKey : Bool)
    (c : Credential) (owed : OwedOnDocument) (e : Evidence) (hf : c.form = .none) :
    editAdmitted true holdsKey c owed e = verifiedPermit owed e := by
  simp [editAdmitted, editCredential, hf]

/-- A delegation without the key must be good: one somebody else signed, one
made for another node, or one that expired admits nothing. -/
theorem a_bad_delegation_admits_no_edit (proven : Bool) (c : Credential)
    (owed : OwedOnDocument) (e : Evidence) (hf : c.form = .delegation)
    (hg : good c = false) :
    editAdmitted proven false c owed e = false := by
  simp [editAdmitted, editCredential, hf, hg]

end VerifiedRemoteAuthority
