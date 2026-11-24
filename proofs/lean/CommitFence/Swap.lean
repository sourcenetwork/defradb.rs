/-!
# CommitFence.Swap: the caller and the owner settle each fence exactly once

The story, in plain words. A caller hands a commit to the worker that owns
its transaction and waits for the answer. Each such commit carries a small
state word that starts "pending". If the caller stops waiting, it raises the
database's count of commits still landing, then tries to swap the word from
pending to "abandoned". The owner, once it is finished with the commit, tries
to swap the word from pending to "done". A swap only works on a pending word,
so the two race and exactly one wins. The loser lowers the count: a caller
that lost (the owner was done first) takes its own raise back; an owner that
lost (the caller had left) lowers the caller's raise once the commit is done.

What must never happen:
* both swaps win, or both lose (`exactly_one_wins`);
* the caller's one raise is lowered twice, or never (`one_lowering_per_raise`);
* the caller wins and the owner does not lower, which would leak the raise
  (`caller_win_means_owner_lowers`);
* a commit whose caller never stopped waiting lowers anything
  (`uncancelled_owner_wins`).

The temporal story (the raise comes before the caller's swap, a reader looks
at the count, wakes) is the TLA+ model `proofs/tla/CommitFence.tla`; this file
carries the law that model takes on trust about the two swaps.

Rust anchors: `Fence::abandon` and `Fence::finish` (`crates/database/db/src/txn/fence.rs`),
two `compare_exchange` calls on one `AtomicU8`, modeled here as their
sequential meaning.
-/

namespace CommitFence

-- The state word one commit carries.
inductive Word where
  -- Neither side has moved it yet.
  | pending
  -- The caller stopped waiting first.
  | abandoned
  -- The owner finished first.
  | done
  -- Words can be compared and printed.
  deriving DecidableEq, Repr

-- One compare-and-swap from pending to `target`: the word after it, and whether it won.
def swap (word target : Word) : Word × Bool :=
  -- Look at the word as it is now.
  match word with
  -- Still pending: the swap writes the target and wins.
  | .pending => (target, true)
  -- Already moved by the other side: the swap changes nothing and loses.
  | other => (other, false)

-- Both swaps on one commit, in the order `callerFirst` says: who won, the caller's result first.
def race (callerFirst : Bool) : Bool × Bool :=
  -- When the caller moves first:
  if callerFirst then
    -- the caller swaps pending to abandoned,
    let caller := swap .pending .abandoned
    -- then the owner swaps whatever the caller left toward done;
    let owner := swap caller.1 .done
    -- report the caller's win, then the owner's.
    (caller.2, owner.2)
  -- When the owner moves first:
  else
    -- the owner swaps pending to done,
    let owner := swap .pending .done
    -- then the caller swaps whatever the owner left toward abandoned;
    let caller := swap owner.1 .abandoned
    -- report the caller's win, then the owner's.
    (caller.2, owner.2)

-- How many times the count is lowered for one cancelled commit: once by each side that lost.
def lowerings (callerFirst : Bool) : Nat :=
  -- A caller that lost takes its raise back (one lowering), a winner lowers nothing;
  (if (race callerFirst).1 then 0 else 1)
  -- an owner that lost lowers the caller's raise (one lowering), a winner lowers nothing.
  + (if (race callerFirst).2 then 0 else 1)

-- Promise: whichever order the two swaps run in, exactly one of them wins.
theorem exactly_one_wins (callerFirst : Bool) :
    -- The caller's result differs from the owner's: one won and one lost.
    (race callerFirst).1 ≠ (race callerFirst).2 := by
  -- Check both orders; each is a small computation the checker runs.
  cases callerFirst <;> decide

-- Promise: whichever order, the caller's one raise is lowered exactly once.
theorem one_lowering_per_raise (callerFirst : Bool) :
    -- The lowerings from both sides add up to one, the number of raises.
    lowerings callerFirst = 1 := by
  -- Check both orders by computing.
  cases callerFirst <;> decide

-- Promise: if the caller's swap won, the owner's lost, so the owner is the one that lowers.
theorem caller_win_means_owner_lowers (callerFirst : Bool) :
    -- A winning caller implies a losing owner.
    (race callerFirst).1 = true → (race callerFirst).2 = false := by
  -- Check both orders by computing.
  cases callerFirst <;> decide

-- Promise: an owner whose caller never stopped waiting finds the word pending and wins, so it lowers nothing.
theorem uncancelled_owner_wins :
    -- The owner's swap on a pending word wins.
    (swap .pending .done).2 = true := by
  -- Compute the swap.
  decide

end CommitFence
