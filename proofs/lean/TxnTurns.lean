/-!
# TxnTurns: tickets come out in order, and skipping passes only dropped ones

The story, in plain words. Each explicit transaction hands out turns, so the
calls a C caller makes on it run in the order they were made. A call takes a
ticket (the next number), waits until the transaction serves that number,
runs, and then serves the next one. A call that gives up before its turn
marks its ticket dropped, and whoever reaches a dropped ticket skips it.

What must never happen: two calls get the same ticket, a later call gets an
earlier number, or skipping passes over a ticket that was not dropped (its
call would then never run, or run out of turn).

This file proves, one step at a time, that taking a ticket hands out the
current next number and moves `next` past it (`take_hands_out_next`,
`take_moves_next`), so tickets are distinct and in call order, and that
skipping from a finished ticket passes only dropped tickets
(`skip_passes_only_dropped`). Which party skips, and the race between a
finishing call and a dropping one, are the TLA+ model
`proofs/tla/TxnTurns.tla`.

Rust anchors (`crates/ffi/src/state/turns.rs`): `Turns::take`,
`Turns::finish`, `Ticket`'s `Drop`.
-/

namespace TxnTurns

-- One transaction's turns.
structure Turns where
  -- The next ticket number to hand out.
  next : Nat
  -- The ticket whose turn it is.
  serving : Nat
  -- Tickets dropped before their turn and not yet skipped.
  dropped : List Nat

-- `Turns::take`: hand out the next number, and move `next` past it.
def take (t : Turns) : Turns × Nat :=
  -- The new state, and the ticket handed out.
  ({ t with next := t.next + 1 }, t.next)

-- From `s`, skip every dropped ticket in a row, at most `fuel` of them, and
-- answer the first ticket that was not dropped (the next one to serve).
def skipFrom : Nat → Nat → List Nat → Nat
  -- No fuel left: stop where we are.
  | 0, s, _ => s
  -- A dropped ticket is skipped; any other is the one to serve.
  | fuel + 1, s, dropped => if s ∈ dropped then skipFrom fuel (s + 1) dropped else s

-- Taking a ticket hands out exactly the current next number.
theorem take_hands_out_next (t : Turns) : (take t).2 = t.next := by
  -- The second component is `t.next` by definition.
  rfl

-- Taking a ticket moves `next` one past it, so the next call gets a later,
-- different number.
theorem take_moves_next (t : Turns) : (take t).1.next = (take t).2 + 1 := by
  -- Both sides are `t.next + 1` by definition.
  rfl

-- Skipping from `s` passes only dropped tickets: every ticket from `s` up to
-- (not including) the one served next was dropped.
theorem skip_passes_only_dropped (fuel s k : Nat) (dropped : List Nat)
    (low : s ≤ k) (high : k < skipFrom fuel s dropped) : k ∈ dropped := by
  -- Induct on the fuel, for every starting ticket.
  induction fuel generalizing s with
  | zero =>
    -- No fuel: the answer is `s` itself, so no `k` lies between.
    simp [skipFrom] at high
    omega
  | succ fuel ih =>
    -- Split on whether `s` itself was dropped.
    by_cases h : s ∈ dropped
    · -- `s` was dropped and skipped: either `k` is `s`, or it lies further on.
      simp [skipFrom, h] at high
      by_cases hk : k = s
      · subst hk
        exact h
      · exact ih (s + 1) (by omega) high
    · -- `s` was not dropped: it is served at once, so no `k` lies between.
      simp [skipFrom, h] at high
      omega

end TxnTurns
