/-! The merge gate for a change to a protected replicated document.

The story: a change to a document that has an access policy arrives from a
peer, signed by someone or by nobody. The node either lets it through to
merge, puts it back to be tried again (a retryable skip), parks it, or
rejects it for good.

It decides in two steps. First the node's ACP hook judges the change: it
looks at whether the provider knows the document at all, then at what this
node still owes the provider for the document, then at the provider's
answer about the signer. While the node's own registration of the document
is owed, the provider has never seen it, so its "no" about anyone but the
DID that registration names means nothing: the change waits. While the
grants the node owes for the document's creator are owed, or the node knows
the owner still owes them, the creator's change waits. Only then does the
provider decide: allowed goes through, refused is a refusal.

Then the gate turns a refusal into what happens to the change. Under strict
access with a remote provider, the provider's state can change later (a
grant made on this node or elsewhere), so a refusal of a signed change is
parked: it waits by its document and is tried again with a fresh decision.
A change signed by nobody names nobody a grant could ever cover, so its
refusal is final. A local provider, or a node without strict access, keeps
the old behaviour: a refusal is a rejection.

What must never happen: a signed change under a remote provider with strict
access rejected for good; a change merged that the provider does not allow
(except an unregistered document on a node that treats it as public); a
change parked anywhere else than a signed refusal under a remote provider
with strict access.

What the provider answers (`holds`) is outside this file; this is how the
node turns what it knows, what it owes and that answer into an outcome.
Frames the gate does not judge at all (a genesis, a collection without a
policy, a node without a provider, and a local provider's frames outside a
transaction, which are judged in the mutation's own snapshot) are outside
it too.
Rust owner: crates/database/db/src/merge/acp_merge_handler.rs
AcpCompositeMergeHook::on_protected_update and
AcpCompositeMergeHook::parks_refusals, composed by
crates/database/db/src/merge/merge_handler/protected_update.rs check_protected_update
and frame_signer,
over crates/engine/acp/src/replica.rs ReplicaOwed. -/
namespace ReplicaGate

/-- What the node does with one change. -/
inductive Outcome where
  /-- The change goes through: this gate lets it merge. -/
  | merge
  /-- The change is put back to be tried again. -/
  | retry
  /-- The change waits, parked by its document, for that document's
  relationships to change, and is then tried again. -/
  | parked
  /-- The change is rejected for good on this node. -/
  | reject
  deriving DecidableEq, Repr

/-- Which kind of ACP provider the node has. -/
inductive Provider where
  /-- A provider inside the node (`supports_transactions` is true). -/
  | local
  /-- A provider outside the node, such as Vera (`supports_transactions`
  is false). -/
  | remote
  deriving DecidableEq, Repr

/-- How the node is set up for the change's collection. -/
structure Node where
  /-- The kind of provider. -/
  provider : Provider
  /-- Strict replicated-document access: an unregistered document is not
  public, and the node's own signature is no shortcut. -/
  strict : Bool
  /-- The node registers replicated documents of this collection with its
  provider (`replicas_for` finds a listing). -/
  listed : Bool

/-- What the node owes the provider for the document, as stored. -/
structure Owed where
  /-- A registration of the document this node still owes. -/
  registration : Bool
  /-- A grant this node owes for the creator is still owed. -/
  creatorGrants : Bool
  /-- The owner, another of the deployment's actors, owes the creator's
  grants, and this node waits for them. -/
  adopted : Bool

/-- What is known about the change's signer. -/
structure Frame where
  /-- The change's own signature verified and names a DID. A signature
  that fails to verify, or names something that is not a DID, counts as no
  signature (`frame_signer`): the hook and the gate read this one flag. -/
  signed : Bool
  /-- That DID is the one the owed registration names. -/
  isRegistrant : Bool
  /-- That DID is the creator the node's replica marker names. -/
  isCreator : Bool
  /-- The provider lets that DID make this change (`signer_may`). -/
  holds : Bool

/-- What owed the hook reads: the stored one only for a listed collection
under strict access with a remote provider, otherwise nothing
(`replicas_for` is `None`, so `ReplicaOwed::default()`). -/
def owedRead (n : Node) (owed : Owed) : Owed :=
  -- Only this one setup reads what is owed.
  if n.strict && n.provider == .remote && n.listed then owed
  -- Every other setup owes nothing.
  else ⟨false, false, false⟩

/-- The hook's judgement (`on_protected_update`), which never parks. -/
def hook (n : Node) (registered : Bool) (owed : Owed) (f : Frame) : Outcome :=
  -- The provider does not know the document yet.
  if !registered then
    -- Strict access waits for the registration; otherwise it is public.
    (if n.strict then .retry else .merge)
  -- An owed registration naming someone else than the signer: wait.
  else if (owedRead n owed).registration && !(f.signed && f.isRegistrant) then .retry
  -- The creator, while its grants are owed or the owner's are awaited and
  -- the provider does not let it in yet: wait.
  else if (f.signed && f.isCreator) &&
      ((owedRead n owed).creatorGrants ||
        ((owedRead n owed).adopted && !(f.signed && f.holds))) then .retry
  -- The provider lets a signer in: through. Nobody unsigned is let in.
  else if f.signed && f.holds then .merge
  -- Otherwise the provider refused.
  else .reject

/-- Whether a refusal under this setup is parked (`parks_refusals`). -/
def parks (n : Node) : Bool :=
  -- Strict access with a remote provider, whose answer a later grant changes.
  n.strict && n.provider == .remote

/-- The gate (`check_protected_update`): the hook's judgement, with a signed
refusal parked where refusals park. -/
def gate (n : Node) (registered : Bool) (owed : Owed) (f : Frame) : Outcome :=
  -- Look at what the hook said.
  match hook n registered owed f with
  -- A refusal of a signed change where refusals park: parked.
  | .reject => if f.signed && parks n then .parked else .reject
  -- Anything else passes through unchanged.
  | o => o

/-- The gate as it was before refusals were parked: the hook's judgement as
it stands. -/
def oldGate (n : Node) (registered : Bool) (owed : Owed) (f : Frame) : Outcome :=
  -- Nothing turned a refusal into anything else.
  hook n registered owed f

/-- The hook itself never parks a change. -/
theorem hook_never_parks (n : Node) (registered : Bool) (owed : Owed) (f : Frame) :
    hook n registered owed f ≠ .parked := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h⟩ := f
  -- Then check every combination.
  cases p <;> revert s l registered r g a sg ir ic h <;> decide

/-- Under strict access with a remote provider, a signed change is never
rejected for good. -/
theorem signed_never_rejected_under_remote (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (hp : n.provider = .remote) (hs : n.strict = true) (hf : f.signed = true) :
    gate n registered owed f ≠ .reject := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h⟩ := f
  -- Put the given setup and signature in place.
  simp only at hp hs hf; subst hp hs hf
  -- Then check every combination of the rest.
  revert l registered r g a ir ic h; decide

/-- A signed change the provider refuses under strict access with a remote
provider is parked. -/
theorem signed_refusal_parks_under_remote (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (hp : n.provider = .remote) (hs : n.strict = true) (hf : f.signed = true)
    (hr : hook n registered owed f = .reject) : gate n registered owed f = .parked := by
  -- The gate rewrites the hook's refusal, and refusals park in this setup.
  simp [gate, hr, parks, hp, hs, hf]

/-- A refusal of a change signed by nobody is rejected for good, under every
provider and setup. -/
theorem unsigned_refusal_is_rejected (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (hf : f.signed = false) (hr : hook n registered owed f = .reject) :
    gate n registered owed f = .reject := by
  -- The gate keeps the hook's refusal of an unsigned change.
  simp [gate, hr, hf]

/-- Once the provider knows the document and no registration is owed, a
change signed by nobody is rejected. -/
theorem unsigned_is_rejected (n : Node) (owed : Owed) (f : Frame)
    (hf : f.signed = false) (ho : (owedRead n owed).registration = false) :
    gate n true owed f = .reject := by
  -- Unsigned means not the registrant, not the creator, and not let in.
  simp [gate, hook, ho, hf]

/-- A change is parked only when the hook refused a signed change under
strict access with a remote provider. -/
theorem parked_only_from_signed_refusal (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (h : gate n registered owed f = .parked) :
    n.provider = .remote ∧ n.strict = true ∧ f.signed = true ∧
      hook n registered owed f = .reject := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert h; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- A change the provider lets in, with nothing owed holding it back, goes
through. -/
theorem allowed_merges (n : Node) (owed : Owed) (f : Frame)
    (hf : f.signed = true) (hh : f.holds = true)
    (hreg : (owedRead n owed).registration = false ∨ f.isRegistrant = true)
    (hgr : f.isCreator = false ∨ (owedRead n owed).creatorGrants = false) :
    gate n true owed f = .merge := by
  -- Unfold the gate and the hook.
  unfold gate hook
  -- Each fact given settles one test of the hook.
  rcases hreg with hreg | hreg <;> rcases hgr with hgr | hgr <;> simp [hf, hh, hreg, hgr]

/-- A change goes through only when the provider lets its signer in, or on a
node without strict access whose provider does not know the document. -/
theorem merged_only_when_allowed (n : Node) (registered : Bool) (owed : Owed) (f : Frame)
    (h : gate n registered owed f = .merge) :
    (f.signed = true ∧ f.holds = true) ∨ (registered = false ∧ n.strict = false) := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert h; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- A change is rejected for good only when the provider refused it, and
then only if it is unsigned or refusals do not park in this setup. -/
theorem rejected_only_on_refusal (n : Node) (registered : Bool) (owed : Owed) (f : Frame)
    (h : gate n registered owed f = .reject) :
    hook n registered owed f = .reject ∧ (f.signed && f.holds) = false ∧
      (f.signed = false ∨ parks n = false) := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert h; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- While an owed registration names someone else than the signer, nobody is
rejected or parked: the change waits, or, without strict access on an
unregistered document, goes through. -/
theorem owed_registration_rejects_nobody (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (h : (owedRead n owed).registration = true)
    (hn : (f.signed && f.isRegistrant) = false) :
    gate n registered owed f = .retry ∨ gate n registered owed f = .merge := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert h hn; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- The creator is never rejected or parked while its grants are owed. -/
theorem creator_waits_for_its_grants (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (hf : f.signed = true) (hc : f.isCreator = true)
    (h : (owedRead n owed).creatorGrants = true) :
    gate n registered owed f = .retry ∨ gate n registered owed f = .merge := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert hf hc h; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- The creator is never rejected or parked while the node waits for the
owner's grants. -/
theorem creator_waits_for_the_owners_grants (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (hf : f.signed = true) (hc : f.isCreator = true)
    (h : (owedRead n owed).adopted = true) :
    gate n registered owed f = .retry ∨ gate n registered owed f = .merge := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Then check every combination.
  revert hf hc h; cases p <;> revert s l registered r g a sg ir ic h' <;> decide

/-- With the document known and nothing owed, the provider alone decides: a
refusal is parked where refusals park and the change is signed, and
rejected otherwise. -/
theorem nothing_owed_the_provider_decides (n : Node) (owed : Owed) (f : Frame)
    (h : owedRead n owed = ⟨false, false, false⟩) :
    gate n true owed f =
      if f.signed && f.holds then .merge
      else if f.signed && parks n then .parked else .reject := by
  -- Unfold the gate and the hook with nothing owed.
  unfold gate hook
  -- The answer depends only on the signature and the provider's answer.
  cases hs : f.signed <;> cases hh : f.holds <;> simp [h, hs, hh]

/-- Another signer's change is never held back by the creator's grants:
with the document known and no registration in the way, the provider
decides it. -/
theorem other_signers_ignore_creator_grants (n : Node) (owed : Owed) (f : Frame)
    (hc : f.isCreator = false)
    (hreg : (owedRead n owed).registration = false ∨ (f.signed && f.isRegistrant) = true) :
    gate n true owed f =
      if f.signed && f.holds then .merge
      else if f.signed && parks n then .parked else .reject := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Put the non-creator in place.
  simp only at hc; subst hc
  -- Then check every combination.
  revert hreg; cases p <;> revert s l r g a sg ir h' <;> decide

/-- A local provider decides exactly as before: the gate is the old gate. -/
theorem local_provider_unchanged (n : Node) (registered : Bool) (owed : Owed) (f : Frame)
    (hp : n.provider = .local) : gate n registered owed f = oldGate n registered owed f := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Put the local provider in place.
  simp only at hp; subst hp
  -- Then check every combination.
  revert s l registered r g a sg ir ic h'; decide

/-- A node without strict access decides exactly as before, whatever its
provider. -/
theorem lax_access_unchanged (n : Node) (registered : Bool) (owed : Owed) (f : Frame)
    (hs : n.strict = false) : gate n registered owed f = oldGate n registered owed f := by
  -- Take every part apart into yes/no answers.
  obtain ⟨p, s, l⟩ := n; obtain ⟨r, g, a⟩ := owed; obtain ⟨sg, ir, ic, h'⟩ := f
  -- Put the lax access in place.
  simp only at hs; subst hs
  -- Then check every combination.
  cases p <;> revert l registered r g a sg ir ic h' <;> decide

/-- With a local provider, a change is rejected only when the provider
refuses it, as before. -/
theorem local_rejected_only_on_refusal (n : Node) (registered : Bool) (owed : Owed)
    (f : Frame) (_hp : n.provider = .local) (h : gate n registered owed f = .reject) :
    (f.signed && f.holds) = false :=
  -- The general statement covers it.
  (rejected_only_on_refusal n registered owed f h).2.1

/-- With a local provider, nothing is ever parked. -/
theorem local_never_parks (n : Node) (registered : Bool) (owed : Owed) (f : Frame)
    (hp : n.provider = .local) : gate n registered owed f ≠ .parked := by
  -- Suppose it were parked.
  intro h
  -- A parked change needs a remote provider.
  have hr := (parked_only_from_signed_refusal n registered owed f h).1
  -- Which is not the local one given.
  rw [hp] at hr; cases hr

/-- With a local provider, nothing owed is ever read. -/
theorem local_reads_nothing_owed (n : Node) (owed : Owed) (hp : n.provider = .local) :
    owedRead n owed = ⟨false, false, false⟩ := by
  -- The read needs a remote provider.
  simp [owedRead, hp]

/-! ## Conformance cases

Every combination of the inputs, with the hook's judgement, whether
refusals park, and the gate's outcome, one tab-separated line each, for
`crates/database/db/src/merge/gate_conformance_tests.rs` to drive the Rust hook with.
Run by `lake env lean --run ReplicaGate.lean`. -/

/-- Every yes/no value. -/
def bools : List Bool := [false, true]

/-- The wire name of an outcome. -/
def Outcome.name : Outcome → String
  -- Each outcome by the name the Rust test reads.
  | .merge => "merge" | .retry => "retry" | .parked => "parked" | .reject => "reject"

/-- One line per combination of the inputs. -/
def cases : List String := Id.run do
  -- Collect the lines here.
  let mut out := []
  -- Every provider, setup, registration, owed state and signer.
  for p in [Provider.local, .remote] do
    for s in bools do for l in bools do for reg in bools do
      for r in bools do for g in bools do for a in bools do
        for sg in bools do for ir in bools do for ic in bools do for h in bools do
          -- The setup, what is owed and the signer for this line.
          let n : Node := ⟨p, s, l⟩
          let o : Owed := ⟨r, g, a⟩
          let f : Frame := ⟨sg, ir, ic, h⟩
          -- The inputs, then the hook's answer, `parks`, and the gate's.
          let fields := [if p == .remote then "remote" else "local"] ++
            ([s, l, reg, r, g, a, sg, ir, ic, h].map toString) ++
            [(hook n reg o f).name, toString (parks n), (gate n reg o f).name]
          -- One line for this combination.
          out := out ++ ["\t".intercalate fields]
  -- Every line, in order.
  return out

end ReplicaGate

/-- Print every conformance case. -/
def main : IO Unit := do
  -- One line per case.
  for line in ReplicaGate.cases do IO.println line
