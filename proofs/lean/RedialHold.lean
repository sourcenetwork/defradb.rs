/-!
# RedialHold: a bounded table of peers Kademlia must not redial yet

The story, in plain words. When a node loses its last connection to a peer,
its Kademlia must not dial that peer again for a while (#3357). The node
writes the peer down with a deadline: the moment the hold ends. Every hold
taken also goes on the end of a queue, oldest first, with a number that
grows by one for each hold, so a peer that disconnects twice has two queue
entries, and only the later one, its latest hold, counts.

Three things shrink the table:
* a hold whose deadline has passed is forgotten from the front of the queue;
* when the queue is full (it holds at most `cap` entries, because the peers
  that disconnect are chosen by the network, not by us), the oldest entry is
  dropped to make room for a new one;
* forgetting a queue entry removes its peer only if that entry is still the
  peer's latest hold.

A tiny example. Peer 1 disconnects at time 0 (hold until 10, number 0) and
again at time 5 (hold until 15, number 1). When the first entry leaves the
queue, peer 1 must stay held until 15, because its latest hold is number 1.

What must never happen:
* the queue grows past `cap` (`hold_bounded`);
* the table names a hold the queue does not have, so it could never be
  forgotten (`hold_latest_in_taken`);
* a hold just taken is not in force until its deadline, or outlasts it
  (`hold_holds`);
* forgetting expired holds drops one that is still in force
  (`live_hold_stays_held`);
* dropping a replaced entry, by expiry or to make room, drops the peer's
  latest hold (`forget_replaced`);
* and a forget that checks only the peer, not the entry, drops a live hold,
  which is why the code compares the entry (`forget_by_peer_drops_live`).

The order of disconnects, trims and dials is the TLA+ model
`proofs/tla/ConnmgrTrimSilence.tla`; this file proves the table's laws for
queues of any length.

Rust anchors: `crates/database/p2p/src/behaviour/redial_hold.rs`
`RedialHold::hold`, `is_held`, `forget_expired`, `forget`, `MAX_HELD`.
-/

namespace RedialHold

-- How long a hold lasts, `REDIAL_HOLD`. The proofs work for any value.
def holdFor : Nat := 10

-- One hold taken (`Taken`): the peer, its deadline, and its number among all holds.
structure Taken where
  -- The peer whose redial is held.
  peer : Nat
  -- The moment the hold ends.
  deadline : Nat
  -- Its number: each hold gets the next one.
  number : Nat
deriving DecidableEq

-- The table (`RedialHold::until`): each held peer's latest hold, as deadline and number.
abbrev Latest := Nat → Option (Nat × Nat)

-- `forget`: drop the entry's peer, but only if the entry is still that peer's latest hold.
def forget (l : Latest) (t : Taken) : Latest :=
  -- Another peer, or a hold that was replaced since, stays as it is.
  fun p => if p = t.peer ∧ l p = some (t.deadline, t.number) then none else l p

-- `forget_expired`: pop entries from the front of the queue while their deadline has passed.
def expire (now : Nat) : Latest → List Taken → Latest × List Taken
  -- An empty queue has nothing to forget.
  | l, [] => (l, [])
  -- The oldest entry ended: forget it and look at the next; otherwise stop.
  | l, t :: ts => if t.deadline ≤ now then expire now (forget l t) ts else (l, t :: ts)

-- The whole state: the table, the queue (oldest first), and the next hold's number.
structure Table where
  -- Each held peer's latest hold.
  latest : Latest
  -- Every hold still kept, oldest first.
  taken : List Taken
  -- The number the next hold gets.
  next : Nat

-- Making room: a full queue drops its oldest entry, forgetting it.
def evict (cap : Nat) (l : Latest) (q : List Taken) : Latest × List Taken :=
  -- Only a queue that has reached the cap gives anything up.
  if cap ≤ q.length then
    match q with
    -- Nothing to drop.
    | [] => (l, [])
    -- Drop the oldest entry.
    | t :: ts => (forget l t, ts)
  else (l, q)

-- Write a peer's latest hold into the table.
def set (l : Latest) (p : Nat) (e : Nat × Nat) : Latest :=
  -- That peer gets the new hold; every other peer keeps its own.
  fun x => if x = p then some e else l x

-- `hold`: forget expired holds, make room if full, then hold `p` until `now + holdFor`.
def hold (cap now p : Nat) (s : Table) : Table :=
  -- Expired holds are forgotten first.
  let e := expire now s.latest s.taken
  -- Then a full queue gives up its oldest entry.
  let v := evict cap e.1 e.2
  -- The new hold becomes the peer's latest and joins the end of the queue.
  { latest := set v.1 p (now + holdFor, s.next),
    taken := v.2 ++ [⟨p, now + holdFor, s.next⟩],
    next := s.next + 1 }

-- `is_held`: after forgetting expired holds, is `p`'s latest hold still in force at `now`?
def isHeld (s : Table) (p now : Nat) : Bool :=
  match (expire now s.latest s.taken).1 p with
  -- In force while `now` is before its deadline.
  | some (u, _) => decide (now < u)
  -- No hold at all.
  | none => false

-- Every hold the table names has its own entry in the queue.
def LatestInTaken (l : Latest) (q : List Taken) : Prop :=
  -- For each peer the table holds, the queue has that very hold.
  ∀ p u n, l p = some (u, n) → (⟨p, u, n⟩ : Taken) ∈ q

-- Forgetting expired holds only shortens the queue.
theorem expire_length (now : Nat) (l : Latest) (q : List Taken) :
    (expire now l q).2.length ≤ q.length := by
  -- One entry at a time.
  induction q generalizing l with
  -- An empty queue stays empty.
  | nil => simp [expire]
  -- The front entry is popped and the rest is shorter, or nothing is popped.
  | cons t ts ih =>
    simp only [expire]
    split
    · have := ih (forget l t)
      simp only [List.length_cons]
      omega
    · simp

-- Making room leaves a queue that was at most `cap` long strictly shorter than `cap`.
theorem evict_length (cap : Nat) (l : Latest) (q : List Taken)
    (hq : q.length ≤ cap) (hcap : 0 < cap) : (evict cap l q).2.length < cap := by
  -- Full or not.
  unfold evict
  split
  · rename_i hfull
    cases q with
    -- An empty queue is shorter than any positive cap.
    | nil => simpa using hcap
    -- A full queue loses one entry.
    | cons t ts =>
      simp at hq hfull ⊢
      omega
  -- A queue below the cap is left alone.
  · rename_i hnot
    show q.length < cap
    omega

-- Promise: taking a hold never grows the queue past `cap`.
theorem hold_bounded (cap now p : Nat) (s : Table) (hcap : 0 < cap)
    (hs : s.taken.length ≤ cap) : (hold cap now p s).taken.length ≤ cap := by
  -- The new queue is what is left after expiry and eviction, plus one.
  simp only [hold, List.length_append, List.length_singleton]
  -- Expiry shortens the queue.
  have h1 := expire_length now s.latest s.taken
  -- Eviction then leaves room for one more.
  have h2 := evict_length cap (expire now s.latest s.taken).1
    (expire now s.latest s.taken).2 (by omega) hcap
  omega

-- What `forget` leaves in the table was there before.
theorem forget_sub (l : Latest) (t : Taken) (p : Nat) (e : Nat × Nat)
    (h : forget l t p = some e) : l p = some e := by
  -- Either it removed the peer (impossible here) or left it unchanged.
  unfold forget at h
  split at h
  · simp at h
  · exact h

-- What expiry leaves in the table was there before.
theorem expire_sub (now : Nat) (l : Latest) (q : List Taken) (p : Nat) (e : Nat × Nat)
    (h : (expire now l q).1 p = some e) : l p = some e := by
  -- One popped entry at a time, each a `forget`.
  induction q generalizing l with
  | nil => simpa [expire] using h
  | cons t ts ih =>
    simp only [expire] at h
    split at h
    · exact forget_sub l t p e (ih (forget l t) h)
    · exact h

-- Forgetting an entry whose deadline differs from the peer's latest hold keeps that hold.
theorem forget_keeps (l : Latest) (t : Taken) (p u n : Nat)
    (hl : l p = some (u, n)) (hne : t.deadline ≠ u) : forget l t p = some (u, n) := by
  -- The entry would have to be the latest hold, which has another deadline.
  unfold forget
  split
  · rename_i h
    have h2 := h.2
    rw [hl] at h2
    simp at h2
    exact absurd h2.1.symm hne
  · exact hl

-- Expiry at `now` keeps every hold whose deadline is after `now`.
theorem expire_keeps_live (now : Nat) (l : Latest) (q : List Taken) (p u n : Nat)
    (hl : l p = some (u, n)) (hlive : now < u) : (expire now l q).1 p = some (u, n) := by
  -- Every entry expiry pops ended by `now`, so it is not this hold.
  induction q generalizing l with
  | nil => simpa [expire] using hl
  | cons t ts ih =>
    simp only [expire]
    split
    · rename_i hexp
      exact ih (forget l t) (forget_keeps l t p u n hl (by omega))
    · exact hl

-- Promise: a hold in the table is in force at every moment before its deadline.
theorem live_hold_stays_held (s : Table) (p u n now : Nat)
    (hl : s.latest p = some (u, n)) (hlive : now < u) : isHeld s p now = true := by
  -- Expiry keeps it, and then `now < u` answers yes.
  unfold isHeld
  rw [expire_keeps_live now s.latest s.taken p u n hl hlive]
  simp [hlive]

-- Promise: right after `hold cap now p`, `p` is held at `t` exactly when `t < now + holdFor`.
theorem hold_holds (cap now p t : Nat) (s : Table) :
    isHeld (hold cap now p s) p t = true ↔ t < now + holdFor := by
  -- The new hold is `p`'s latest.
  have hl : (hold cap now p s).latest p = some (now + holdFor, s.next) := by
    simp [hold, set]
  constructor
  -- Held means some hold of `p` is in force, and the only one is the new hold.
  · intro h
    unfold isHeld at h
    split at h
    · rename_i u m hsome
      have hsrc := expire_sub t _ _ p (u, m) hsome
      rw [hl] at hsrc
      simp at hsrc
      obtain ⟨rfl, rfl⟩ := hsrc
      simpa using h
    · simp at h
  -- Before the deadline the new hold is in force.
  · intro ht
    exact live_hold_stays_held _ p _ _ t hl ht

-- Promise: forgetting an entry that is not its peer's latest hold changes nothing.
theorem forget_replaced (l : Latest) (t : Taken)
    (h : l t.peer ≠ some (t.deadline, t.number)) : forget l t = l := by
  -- Peer by peer.
  funext p
  unfold forget
  split
  · rename_i hp
    obtain ⟨rfl, h2⟩ := hp
    exact absurd h2 h
  · rfl

-- Forgetting the front entry keeps every other hold the table names in the rest of the queue.
theorem forget_in (l : Latest) (t : Taken) (ts : List Taken)
    (H : LatestInTaken l (t :: ts)) : LatestInTaken (forget l t) ts := by
  intro p u n h
  -- The hold was in the table before, so it was in the queue.
  have h0 := forget_sub l t p (u, n) h
  rcases List.mem_cons.mp (H p u n h0) with heq | hmem
  -- It was the front entry itself, which `forget` removed: impossible.
  · subst heq
    simp [forget, h0] at h
  -- It is further back, still in the queue.
  · exact hmem

-- Expiry keeps the table inside the queue.
theorem expire_in (now : Nat) (l : Latest) (q : List Taken) (H : LatestInTaken l q) :
    LatestInTaken (expire now l q).1 (expire now l q).2 := by
  -- One popped entry at a time.
  induction q generalizing l with
  | nil => simpa [expire] using H
  | cons t ts ih =>
    simp only [expire]
    split
    · exact ih (forget l t) (forget_in l t ts H)
    · exact H

-- Making room keeps the table inside the queue.
theorem evict_in (cap : Nat) (l : Latest) (q : List Taken) (H : LatestInTaken l q) :
    LatestInTaken (evict cap l q).1 (evict cap l q).2 := by
  -- Either nothing changes or the front entry is forgotten.
  unfold evict
  split
  · cases q with
    | nil => simpa using H
    | cons t ts => exact forget_in l t ts H
  · exact H

-- Writing a new hold and queueing it keeps the table inside the queue.
theorem set_in (l : Latest) (q : List Taken) (p u n : Nat) (H : LatestInTaken l q) :
    LatestInTaken (set l p (u, n)) (q ++ [⟨p, u, n⟩]) := by
  intro x u' n' h
  unfold set at h
  split at h
  -- The new hold is the entry just queued.
  · rename_i hx
    subst hx
    simp at h
    obtain ⟨rfl, rfl⟩ := h
    simp
  -- Any other hold was already in the queue.
  · exact List.mem_append_left _ (H x u' n' h)

-- Promise: if every hold the table names is in the queue, that stays so after a hold.
theorem hold_latest_in_taken (cap now p : Nat) (s : Table) (H : LatestInTaken s.latest s.taken) :
    LatestInTaken (hold cap now p s).latest (hold cap now p s).taken := by
  -- Expiry, then eviction, then the new entry, each keep it.
  exact set_in _ _ p _ _ (evict_in cap _ _ (expire_in now s.latest s.taken H))

-- The planted bug: a forget that drops the peer whatever its latest hold is.
def forgetByPeer (l : Latest) (t : Taken) : Latest :=
  -- Drops the peer even if this entry was replaced since.
  fun p => if p = t.peer then none else l p

-- Promise: forgetting by peer alone drops a hold still in force, and `forget` keeps it.
-- Peer 1 was held until 10 (number 0), then again until 15 (number 1).
theorem forget_by_peer_drops_live :
    forgetByPeer (fun p => if p = 1 then some (15, 1) else none) ⟨1, 10, 0⟩ 1 = none ∧
    forget (fun p => if p = 1 then some (15, 1) else none) ⟨1, 10, 0⟩ 1 = some (15, 1) := by
  -- Both sides compute.
  decide

end RedialHold
