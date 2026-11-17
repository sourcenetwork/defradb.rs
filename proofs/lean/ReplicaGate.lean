/-! The merge gate for a change to a replicated document on a node whose
document ACP is a remote provider.

The story: a change arrives signed by someone. The node either merges it,
puts it back to be tried again (a retryable skip), or rejects it for good.
It looks first at what this node still owes the provider for the document,
then asks the provider. While the node's own registration of the document is
owed, the provider has never seen it, so its "no" means nothing: every change
waits. While the grants the node owes for the document's creator are owed,
or the node knows the owner still owes them, the creator's change waits.
Only then does the provider decide: allowed merges, refused is rejected.

What the provider answers (`holdsPermission`) is outside this file; this is
how the node turns what it owes and that answer into an outcome.
Rust owner: crates/db/src/merge/acp_merge_handler.rs
AcpCompositeMergeHook::on_protected_update over
crates/acp/src/replica.rs ReplicaOwed. -/
namespace ReplicaGate

/-- What the node does with one change. -/
inductive Outcome where
  /-- The change merges. -/
  | merge
  /-- The change is put back to be tried again. -/
  | retry
  /-- The change is rejected for good on this node. -/
  | reject
  deriving DecidableEq

/-- What the node owes the provider for the document, as its gate reads it. -/
structure Owed where
  /-- The node's own registration of the document is still owed. -/
  registration : Bool
  /-- The grants the node owes for the creator are still owed. -/
  creatorGrants : Bool
  /-- The owner is another of the deployment's actors, whose grants for the
  creator the node waits for. -/
  adopted : Bool

/-- The gate: what is owed first, then the provider's answer. -/
def decide (owed : Owed) (signerIsCreator holdsPermission : Bool) : Outcome :=
  if owed.registration then .retry
  else if signerIsCreator && (owed.creatorGrants || (owed.adopted && !holdsPermission)) then
    .retry
  else if holdsPermission then .merge
  else .reject

/-- While the node's own registration is owed, nobody is rejected. -/
theorem owed_registration_rejects_nobody (owed : Owed) (creator held : Bool)
    (h : owed.registration = true) : decide owed creator held ≠ .reject := by
  simp [decide, h]

/-- The creator is never rejected while its grants are owed. -/
theorem creator_waits_for_its_grants (owed : Owed) (held : Bool)
    (h : owed.creatorGrants = true) : decide owed true held ≠ .reject := by
  obtain ⟨r, g, a⟩ := owed
  cases r <;> cases g <;> cases a <;> cases held <;> simp_all [decide]

/-- The creator is never rejected while the node waits for the owner's grants. -/
theorem creator_waits_for_the_owners_grants (owed : Owed) (held : Bool)
    (h : owed.adopted = true) : decide owed true held ≠ .reject := by
  obtain ⟨r, g, a⟩ := owed
  cases r <;> cases g <;> cases a <;> cases held <;> simp_all [decide]

/-- A change is rejected only when the provider refuses its signer. -/
theorem rejected_only_on_refusal (owed : Owed) (creator held : Bool)
    (h : decide owed creator held = .reject) : held = false := by
  obtain ⟨r, g, a⟩ := owed
  cases r <;> cases g <;> cases a <;> cases held <;> cases creator <;> simp_all [decide]

/-- A change merges only when the provider allows its signer. -/
theorem merged_only_when_allowed (owed : Owed) (creator held : Bool)
    (h : decide owed creator held = .merge) : held = true := by
  obtain ⟨r, g, a⟩ := owed
  cases r <;> cases g <;> cases a <;> cases held <;> cases creator <;> simp_all [decide]

/-- With nothing owed, the provider alone decides. -/
theorem nothing_owed_the_provider_decides (creator held : Bool) :
    decide ⟨false, false, false⟩ creator held = if held then .merge else .reject := by
  cases held <;> cases creator <;> simp [decide]

/-- Another signer's change is never held back by the creator's grants: once
the registration is paid, the provider decides it. -/
theorem other_signers_ignore_creator_grants (owed : Owed) (held : Bool)
    (h : owed.registration = false) :
    decide owed false held = if held then .merge else .reject := by
  simp [decide, h]

end ReplicaGate
