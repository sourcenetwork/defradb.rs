/-!
# IdentityStore: a DID's slot counts its holds exactly, keeps its key just as long, and never changes a held signer

The story, in plain words. The process keeps every signing key in one store,
one slot per DID. Holders (an FFI identity handle, a node) each take one hold
on a DID and give it back when they go. A key comes with its signer: the key
bytes, or a host's callback and context. Two remote signers with the same
host handle and callback are one signer: the host keeps that context valid
while either is held, so they share a slot. A slot remembers three things that
change together in one swap: whether the DID is pinned for the life of the
process, how many holds it has, and its key, named by its signer (or none).

Five operations change a slot:
* `hold` with a signer adds a hold, storing the key if the slot has none; if
  the slot's key has another signer it refuses and changes nothing;
* `retain` adds a hold only if the key is there, and says whether it did;
* `release` takes one hold away, and drops the key if no hold and no pin is
  left;
* `pin` pins the DID with the process's signer, unless something holds the
  DID with another signer, which it refuses;
* `clear` unpins the DID and drops the key unless something still holds it.

The books are right when the count equals the number of holds given out and
not given back, and the key is there exactly when the DID is pinned or held
(`Inv`). Every operation keeps the books right (`hold_keeps`, `retain_keeps`,
`release_keeps`, `pin_keeps`, `clear_keeps`), so a DID someone holds always
has its key (`held_has_key`) and a key never outlives its last holder unless
pinned (`free_has_no_key`). And while a DID has a key, a hold never changes
its signer (`hold_keeps_signer`), nor does a pin while it is held
(`pin_keeps_held_signer`): no holder is re-routed to a signer it did not bring.
A hold that brings the slot's own signer is always taken
(`hold_accepts_same`): a second handle for one host context is not refused.

What must never happen: a hold is lost or counted twice, a held DID answers
no key, a private key stays after its last holder went, or a holder's signer
is swapped for another.

This is one swap at a time. Two holders racing on one slot, a lookup between
swaps, and what clear must not do to the map are the TLA+ model
`proofs/tla/IdentityStore.tla`.

Rust anchors (`crates/defra-core/src/signing/identity_store.rs`): `Holders`,
`same_signer`, `hold_identity`, `retain_identity`, `release_identity`,
`store_identity`, `clear_identity_store`.
-/

namespace IdentityStore

-- One DID's slot, as the Rust `Holders`. A signer is named by a number; two
-- configs with the same number are `same_signer`: the same key bytes, or the
-- same host context (`RemoteSigner::host_context`) for a remote signer.
structure Slot where
  -- Pinned for the life of the process (`store_identity`).
  pinned : Bool
  -- How many holds are given out and not given back.
  holds : Nat
  -- The key and its signer (`config`), or none.
  key : Option Nat

-- A slot just made: not pinned, no holds, no key (`Arc::default()`).
def empty : Slot := { pinned := false, holds := 0, key := none }

-- `Holders::retained`: pinned, or held at least once.
def retained (s : Slot) : Bool := s.pinned || decide (0 < s.holds)

-- `hold_identity` with signer `g`: refused (the slot unchanged, answer no)
-- when the key has another signer; otherwise one more hold, the key kept or
-- stored with `g`, and answer yes.
def hold (s : Slot) (g : Nat) : Slot × Bool :=
  match s.key with
  -- The slot's key has a signer: the same one joins, another is refused.
  | some k => if k = g then ({ s with holds := s.holds + 1 }, true) else (s, false)
  -- No key: store `g`'s and hold it.
  | none => ({ s with holds := s.holds + 1, key := some g }, true)

-- `retain_identity`: one more hold only when the key is there; the answer
-- says whether it took one.
def retain (s : Slot) : Slot × Bool :=
  match s.key with
  -- With the key, add a hold and answer yes.
  | some _ => ({ s with holds := s.holds + 1 }, true)
  -- Without it, change nothing and answer no.
  | none => (s, false)

-- `release_identity`: one hold fewer (Nat subtraction stops at zero, as
-- `saturating_sub`), and the key dropped when nothing retains the slot.
def release (s : Slot) : Slot :=
  -- The slot with one hold fewer.
  let next := { s with holds := s.holds - 1 }
  -- Its key goes when it is neither pinned nor held.
  if retained next then next else { next with key := none }

-- `store_identity` with signer `g`: refused (unchanged, answer no) when the
-- DID is held with another signer; otherwise pinned with `g`'s key.
def pin (s : Slot) (g : Nat) : Slot × Bool :=
  -- Held, with a key whose signer is not `g`: refuse.
  if 0 < s.holds ∧ s.key ≠ none ∧ s.key ≠ some g then (s, false)
  -- Otherwise pin with `g`.
  else ({ s with pinned := true, key := some g }, true)

-- `clear_identity_store`: unpinned, and the key dropped unless held.
def clear (s : Slot) : Slot :=
  -- The slot unpinned.
  let next := { s with pinned := false }
  -- Its key goes when nothing holds it.
  if retained next then next else { next with key := none }

-- The books are right for a slot with `n` holds given out: the count is `n`,
-- and the key is there exactly when the slot is pinned or `n` is not zero.
def Inv (s : Slot) (n : Nat) : Prop :=
  s.holds = n ∧ (s.key.isSome = true ↔ (s.pinned = true ∨ 0 < n))

-- A slot just made has its books right with no holds given out.
theorem empty_keeps : Inv empty 0 := by
  -- Count zero, and no key with no pin and no hold.
  simp [Inv, empty]

-- A hold keeps the books right, with one more hold given out exactly when it
-- answers yes.
theorem hold_keeps (s : Slot) (g : Nat) (n : Nat) (h : Inv s n) :
    Inv (hold s g).1 (if (hold s g).2 then n + 1 else n) := by
  -- Unfold the books, knowing the count is `n`.
  obtain ⟨hc, hk⟩ := h
  -- Split on the slot's key, then on whether its signer is `g`.
  cases hs : s.key with
  -- No key: the key is stored and a hold given out.
  | none => simp [Inv, hold, hs, hc]
  -- A key: the same signer joins with one more hold, another changes nothing.
  | some k =>
    by_cases hg : k = g
    · simp [Inv, hold, hs, hc, hg]
    · simp only [hold, hs, hg, if_false]
      exact ⟨hc, hk⟩

-- While the slot has a key, a hold never changes its signer: a refused hold
-- changes nothing, and an accepted one brought the same signer.
theorem hold_keeps_signer (s : Slot) (g k : Nat) (hs : s.key = some k) :
    (hold s g).1.key = some k := by
  -- Split on whether `g` is the slot's signer.
  by_cases hg : k = g <;> simp [hold, hs, hg]

-- A hold that brings the slot's own signer is taken: one more hold, the
-- same key, and the answer yes. Nothing valid is refused.
theorem hold_accepts_same (s : Slot) (k : Nat) (hs : s.key = some k) :
    hold s k = ({ s with holds := s.holds + 1 }, true) := by
  -- The slot's signer is `k`, so the hold joins it.
  simp [hold, hs]

-- A hold answers no exactly when the slot's key has another signer.
theorem hold_refuses_other (s : Slot) (g k : Nat) (hs : s.key = some k) (hg : k ≠ g) :
    hold s g = (s, false) := by
  -- The key's signer is not `g`, so the slot is handed back unchanged.
  simp [hold, hs, hg]

-- A retain keeps the books right, with one more hold given out exactly when
-- it answers yes.
theorem retain_keeps (s : Slot) (n : Nat) (h : Inv s n) :
    Inv (retain s).1 (if (retain s).2 then n + 1 else n) := by
  -- Split on whether the slot has its key.
  cases hs : s.key with
  -- No key: nothing changes, and nothing is given out.
  | none => simpa [retain, hs] using h
  -- The key: the count goes up by one with the key kept.
  | some k => simp [Inv, retain, hs, h.1]

-- A retain answers yes exactly when the slot has its key, so it never hands
-- out a hold on a key the store does not have.
theorem retain_answers_key (s : Slot) : (retain s).2 = s.key.isSome := by
  -- Both branches answer what the key says.
  cases hs : s.key <;> simp [retain, hs]

-- A release by a holder that has a hold keeps the books right, with one hold
-- fewer given out.
theorem release_keeps (s : Slot) (n : Nat) (h : Inv s n) (held : 0 < n) :
    Inv (release s) (n - 1) := by
  -- The holder's hold means the key was there.
  have hk : s.key.isSome = true := h.2.mpr (Or.inr held)
  -- Unfold the books, knowing the count is `n`.
  obtain ⟨hc, _⟩ := h
  -- Split on whether the slot is pinned and on whether a hold remains.
  cases hp : s.pinned <;> by_cases hz : 0 < n - 1 <;>
    simp [Inv, release, retained, hc, hp, hz, hk] <;> omega

-- A pin keeps the books right; it gives out no hold.
theorem pin_keeps (s : Slot) (g : Nat) (n : Nat) (h : Inv s n) : Inv (pin s g).1 n := by
  -- Split on whether the pin is refused.
  by_cases hr : 0 < s.holds ∧ s.key ≠ none ∧ s.key ≠ some g
  -- Refused: nothing changes.
  · rw [show pin s g = (s, false) from if_pos hr]
    exact h
  -- Accepted: the count is unchanged, pinned with a key.
  · rw [show pin s g = ({ s with pinned := true, key := some g }, true) from if_neg hr]
    simp [Inv, h.1]

-- While something holds the DID, a pin never changes its signer.
theorem pin_keeps_held_signer (s : Slot) (g k : Nat) (held : 0 < s.holds)
    (hs : s.key = some k) : (pin s g).1.key = some k := by
  -- Split on whether `g` is the slot's signer.
  by_cases hg : k = g
  -- The same signer: the pin stores what is already there.
  · subst hg
    by_cases hr : 0 < s.holds ∧ s.key ≠ none ∧ s.key ≠ some k
    · rw [show pin s k = (s, false) from if_pos hr]
      exact hs
    · rw [show pin s k = ({ s with pinned := true, key := some k }, true) from if_neg hr]
  -- Another signer: the pin is refused and changes nothing.
  · have hr : 0 < s.holds ∧ s.key ≠ none ∧ s.key ≠ some g := by
      refine ⟨held, ?_, ?_⟩ <;> simp [hs, hg]
    rw [show pin s g = (s, false) from if_pos hr]
    exact hs

-- A clear keeps the books right; it takes back no hold.
theorem clear_keeps (s : Slot) (n : Nat) (h : Inv s n) : Inv (clear s) n := by
  -- Unfold the books, knowing the count is `n`.
  obtain ⟨hc, hk⟩ := h
  -- Split on whether a hold is given out.
  by_cases hz : 0 < n
  -- Held: the key was there and stays.
  · have : s.key.isSome = true := hk.mpr (Or.inr hz)
    simp [Inv, clear, retained, hc, hz, this]
  -- Not held: the key goes, and the slot is unpinned.
  · simp [Inv, clear, retained, hc, hz]

-- While any hold is given out, the slot has its key: a holder's signed
-- request always finds it.
theorem held_has_key (s : Slot) (n : Nat) (h : Inv s n) (held : 0 < n) :
    s.key.isSome = true :=
  -- The books say the key is there when a hold is.
  h.2.mpr (Or.inr held)

-- With no hold given out and no pin, the slot has no key: a private key never
-- outlives its last holder.
theorem free_has_no_key (s : Slot) (h : Inv s 0) (unpinned : s.pinned = false) :
    s.key = none := by
  -- The books say the key is there only when pinned or held.
  cases hs : s.key with
  -- No key: done.
  | none => rfl
  -- A key would need a pin or a hold, and there is neither.
  | some k =>
    have := h.2.mp (by simp [hs])
    simp [unpinned] at this

end IdentityStore
