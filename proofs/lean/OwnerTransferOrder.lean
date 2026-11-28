/-! The story. A node owes an external provider changes on one object, in
queue order: grants, revocations and, at most once per owner, a transfer of
the object's ownership to another actor. The provider takes a change only
from the object's owner at the time it arrives. So a transfer must wait
until every earlier change on its object is decided (those were made under
the old owner's authority), and every later change must wait until the
transfer is decided (those were made for the new owner's object). A change
parked as refused is decided: it holds nothing back. The changes of one
request are applied in their queue order, so they hold nothing back among
themselves.

The same transfer is counted as taking the owner relation from its actor,
which denies that actor on the object until the provider holds the
transfer: the node never lets the old owner in on the provider's
not-yet-updated word.

This file decides both and proves that they hold what they must and
nothing more.

The node also orders its object's changes by commit around a transfer: of
a transfer and another change staged on one object at once, the one that
commits second is aborted, because it read, validated, a key the other
wrote (`stage` in `crates/engine/acp/src/outbox/ops.rs`).

The code: `held_by_transfer` in
`crates/engine/acp/src/external/transfer_order.rs`, and `denial` in
`crates/engine/acp/src/outbox/deny.rs` over the counts `account` in
`crates/engine/acp/src/outbox/ops.rs` merges for a transfer. -/
namespace OwnerTransferOrder

-- One change owed on the same object, earlier in the queue.
structure Earlier where
  -- Its place in the queue.
  seq : Nat
  -- Whether it is a transfer of ownership.
  transfer : Bool
  -- Whether it is still to be delivered: staged, in flight or waiting for
  -- authority. A change parked as refused is not, nor one staged unchecked
  -- that waits for authority (`earlier_on_object` decides this).
  live : Bool
  deriving Repr

/-- Whether a change (a transfer when `transfer` holds) waits behind one of
`earlier`, sharing a request with the changes `members`: an earlier change
still to be delivered, outside the request, that is a transfer, or that
any change at all waits for when this one is a transfer. -/
def heldByTransfer (transfer : Bool) (earlier : List Earlier) (members : List Nat) : Bool :=
  -- Any earlier live change outside the request that the ordering rule names.
  earlier.any fun e => e.live && !(members.contains e.seq) && (transfer || e.transfer)

/-- With nothing earlier on its object, nothing holds a change back. -/
theorem nothing_earlier_holds_nothing (transfer : Bool) (members : List Nat) :
    heldByTransfer transfer [] members = false := by
  -- `any` over the empty list is false.
  simp [heldByTransfer]

/-- A change parked as refused holds nothing back, transfer or not: it was
decided already, and waiting for it would wait for ever. -/
theorem a_refused_change_holds_nothing (transfer : Bool) (e : Earlier) (members : List Nat)
    (h : e.live = false) :
    heldByTransfer transfer [e] members = false := by
  -- The only earlier change fails its first conjunct.
  simp [heldByTransfer, h]

/-- A change of the same request holds nothing back: the request applies its
changes in queue order. -/
theorem a_member_holds_nothing (transfer : Bool) (e : Earlier) (members : List Nat)
    (h : e.seq ∈ members) :
    heldByTransfer transfer [e] members = false := by
  -- The only earlier change fails its second conjunct.
  simp [heldByTransfer, h]

/-- A transfer waits for any earlier change on its object still to be
delivered outside its request: those were made under the old owner. -/
theorem a_transfer_waits_for_any_earlier_change (e : Earlier) (members : List Nat)
    (live : e.live = true) (outside : e.seq ∉ members) :
    heldByTransfer true [e] members = true := by
  -- Every conjunct holds for the only earlier change.
  simp [heldByTransfer, live, outside]

/-- A change that is not a transfer waits for an earlier transfer still to
be delivered outside its request: it was made for the new owner's object. -/
theorem a_change_waits_for_an_earlier_transfer (e : Earlier) (members : List Nat)
    (live : e.live = true) (outside : e.seq ∉ members) (t : e.transfer = true) :
    heldByTransfer false [e] members = true := by
  -- Every conjunct holds for the only earlier change.
  simp [heldByTransfer, live, outside, t]

/-- A change that is not a transfer never waits behind changes that are not
transfers: other tuples go on as before. -/
theorem only_a_transfer_holds_an_ordinary_change (earlier : List Earlier) (members : List Nat)
    (none : ∀ e ∈ earlier, e.transfer = false) :
    heldByTransfer false earlier members = false := by
  -- Each earlier change fails the last conjunct.
  unfold heldByTransfer
  -- `any` is false when no element satisfies the predicate.
  rw [List.any_eq_false]
  -- Take any earlier change, and use that it is no transfer.
  intro e he
  simp [none e he]

/-- What a send asks: when a change is not held, every earlier change still
to be delivered outside its request is no transfer, and, when it is itself a
transfer, there is none at all. -/
theorem not_held_means_ordered (transfer : Bool) (earlier : List Earlier) (members : List Nat)
    (h : heldByTransfer transfer earlier members = false) :
    ∀ e ∈ earlier, e.live = true → e.seq ∉ members → transfer = false ∧ e.transfer = false := by
  -- Unfold the decision to a statement about each earlier change.
  unfold heldByTransfer at h
  rw [List.any_eq_false] at h
  -- Take any earlier live change outside the request.
  intro e he live outside
  -- Its conjunction is false, and the first two parts hold, so the last fails.
  have := h e he
  simp [live, outside] at this
  exact this

-- What the owed changes on an object say about one identity: the count of
-- deletes wider than one actor, and of those naming it, all and of the
-- owner relation.
structure Owed where
  -- Changes naming the identity that take something from it.
  first : Nat
  -- Of those, the ones that take the owner relation from it.
  second : Nat
  deriving Repr

-- What owed changes deny an identity.
inductive Denial where
  -- Nothing.
  | none
  -- Everything, unless it owns the object.
  | unlessOwner
  -- Everything: its owner relation is being taken.
  | always
  deriving DecidableEq, Repr

/-- The denial owed changes put on an identity, as `denial` decides it. -/
def denial (wide : Nat) (named : Owed) : Denial :=
  -- The owner relation taken denies outright; any other revocation spares the owner.
  if named.second > 0 then .always
  else if wide > 0 || named.first > 0 then .unlessOwner
  else .none

/-- What staging an owed transfer adds to its actor's counts: one change that
takes the owner relation from it. -/
def stageTransfer (named : Owed) : Owed :=
  -- Both counters go up by one.
  { first := named.first + 1, second := named.second + 1 }

/-- An owed transfer denies its old owner outright, whatever else is owed:
the node does not let it in while the provider still names it the owner. -/
theorem a_transfer_denies_its_old_owner (wide : Nat) (named : Owed) :
    denial wide (stageTransfer named) = .always := by
  -- The second counter is positive after staging.
  simp [denial, stageTransfer]

/-- Nothing else owed on the object lifts that denial: more wide deletes leave
it as it is. -/
theorem nothing_lifts_the_transfer_denial (wide more : Nat) (named : Owed) :
    denial (wide + more) (stageTransfer named) = denial wide (stageTransfer named) := by
  -- Both sides are `always`.
  simp [denial, stageTransfer]

-- What one staging on an object reads and writes, as conflicts see it.
inductive Key where
  -- The object's staged-change marker: every staging writes it, a transfer
  -- reads it, validated.
  | stagedMark
  -- The object's transfer key: a transfer writes it, every other staging
  -- reads it, validated.
  | transferMark
  deriving DecidableEq, Repr

/-- The keys a staging reads, validated: a transfer reads the staged-change
marker, any other change reads the transfer key. -/
def reads (transfer : Bool) : List Key :=
  -- One validated read each.
  if transfer then [.stagedMark] else [.transferMark]

/-- The keys a staging writes: the staged-change marker, and a transfer its
transfer key too. -/
def writes (transfer : Bool) : List Key :=
  -- The staged marker always; the transfer key for a transfer.
  if transfer then [.stagedMark, .transferMark] else [.stagedMark]

/-- Whether a staging that commits second is aborted by one that committed
first while it was open: it read, validated, a key the first wrote. -/
def abortedBy (first second : Bool) : Bool :=
  -- Some key the second read is one the first wrote.
  (reads second).any fun key => (writes first).contains key

/-- A transfer that commits second, after any change on its object staged
beside it, is aborted. -/
theorem a_transfer_committing_second_is_aborted (first : Bool) :
    abortedBy first true = true := by
  -- Every staging writes the staged marker, and a transfer reads it.
  cases first <;> decide

/-- Any change that commits second, after a transfer staged beside it, is
aborted. -/
theorem a_change_committing_after_a_transfer_is_aborted (second : Bool) :
    abortedBy true second = true := by
  -- A transfer writes the transfer key, which every other staging reads.
  cases second <;> decide

/-- Two changes that are not transfers never abort each other: the order
around a transfer costs ordinary changes no conflict. -/
theorem ordinary_changes_do_not_conflict : abortedBy false false = false := by
  -- An ordinary change reads only the transfer key, which no ordinary change writes.
  decide

/-- So around a transfer, of two changes staged on one object at once only
the first to commit stands: their commit order is their queue order once
the second is staged again. -/
theorem around_a_transfer_one_of_two_commits (first second : Bool)
    (h : first = true ∨ second = true) : abortedBy first second = true := by
  -- One of the two is a transfer; each case is a theorem above.
  rcases h with h | h
  · subst h; exact a_change_committing_after_a_transfer_is_aborted second
  · subst h; exact a_transfer_committing_second_is_aborted first

end OwnerTransferOrder
