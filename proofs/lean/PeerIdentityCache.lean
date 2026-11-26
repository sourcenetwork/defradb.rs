/-! The Iroh peer identity cache remembers which DID each peer proved it holds.
It keeps at most `cap` peers. Peers leave in the order they first arrived: a
peer joins the back of the line on its first insert, a refresh changes its DID
but not its place, and when the line is over-full the front peer is dropped.
What must never happen: the cache grows past `cap`, a peer holds two places,
a refresh moves a peer, the peer just inserted is not cached, or anyone but
the front peer is evicted.

This is the one-thread view of `IrohPeerIdentityCache::insert` and `enqueue`
in `crates/database/p2p/src/iroh/identity_cache.rs`. The interleavings of
several inserters are in `proofs/tla/PeerIdentityCache.tla`. -/
namespace PeerIdentityCache

-- The cache is its line of (peer, DID) pairs, oldest first.
abbrev Cache := List (Nat × Nat)

-- The peers in the cache, in line order.
def keys (c : Cache) : List Nat := c.map Prod.fst

-- A refresh rewrites the peer's DID where it already stands in line.
def refresh (c : Cache) (p d : Nat) : Cache :=
  -- Only the pair for `p` changes; every other pair stays as it was.
  c.map (fun e => if e.1 = p then (p, d) else e)

-- insert: refresh a cached peer in place; otherwise join the back of the line
-- and, if that makes the line longer than `cap`, drop the front peer.
def insert (cap : Nat) (c : Cache) (p d : Nat) : Cache :=
  -- A peer already in line is refreshed where it stands.
  if p ∈ keys c then refresh c p d
  -- A new peer goes to the back of the line.
  else
    -- The line with the new peer at the back.
    let grown := c ++ [(p, d)]
    -- Over-full: the front peer is evicted; otherwise nothing is dropped.
    if grown.length > cap then grown.tail else grown

-- Rewriting DIDs never changes which peer stands where.
theorem keys_refresh (c : Cache) (p d : Nat) : keys (refresh c p d) = keys c := by
  -- Walk the line one pair at a time.
  induction c with
  -- An empty line stays empty.
  | nil => rfl
  -- Each pair keeps its peer, whether or not its DID was rewritten.
  | cons e rest ih =>
    -- Split on whether this pair is the refreshed peer.
    by_cases h : e.1 = p <;> simp_all [keys, refresh]

/-- A refresh keeps every peer, the refreshed one included, in its place. -/
theorem refresh_keeps_order (cap : Nat) (c : Cache) (p d : Nat) (cached : p ∈ keys c) :
    keys (insert cap c p d) = keys c := by
  -- The cached branch is a refresh, which keeps the line's peers.
  simp [insert, cached, keys_refresh]

/-- The cache never holds more than `cap` peers once it starts within `cap`. -/
theorem insert_bounded (cap : Nat) (c : Cache) (p d : Nat) (within : c.length ≤ cap) :
    (insert cap c p d).length ≤ cap := by
  -- Split on refresh versus first insert, and on whether the line overflows.
  -- Open up `insert` so its two branches show.
  unfold insert
  -- One goal per branch: refresh, then first insert.
  split
  -- A refresh keeps the line's length.
  · simpa [refresh] using within
  -- A first insert grows the line by one, and drops one if that overflows.
  · -- Unfold the `let` that names the grown line.
    simp only []
    -- Split on overflow; each case is arithmetic on lengths.
    split <;> simp_all <;> omega

/-- A first insert that overflows evicts exactly the front peer. -/
theorem overflow_evicts_front (cap : Nat) (c : Cache) (p d : Nat) (fresh : p ∉ keys c)
    (full : c.length = cap) :
    insert cap c p d = (c ++ [(p, d)]).tail := by
  -- The grown line has `cap + 1` pairs, so the overflow branch runs.
  simp [insert, fresh, full]

/-- No peer ever holds two places in the line. -/
theorem insert_nodup (cap : Nat) (c : Cache) (p d : Nat) (unique : (keys c).Nodup) :
    (keys (insert cap c p d)).Nodup := by
  -- Split on refresh versus first insert.
  -- Open up `insert` so its two branches show.
  unfold insert
  -- One goal per branch: refresh, then first insert.
  split
  -- A refresh leaves the peers exactly as they were.
  · rw [keys_refresh]; exact unique
  -- A first insert adds a peer that was not in line yet.
  · -- Name the fact that `p` was not cached.
    rename_i fresh
    -- The grown line has no repeats: the old peers had none, and `p` is new.
    have grown : (keys (c ++ [(p, d)])).Nodup := by
      -- The keys of the grown line are the old keys with `p` after them.
      simp only [keys, List.map_append, List.map_cons, List.map_nil]
      -- Two repeat-free lists with no peer in common join without repeats.
      unfold List.Nodup
      rw [List.pairwise_append]
      -- The old keys, `[p]`, and the fact that `p` is not among the old keys.
      refine ⟨unique, List.pairwise_singleton _ p, ?_⟩
      -- Any old key and `p` differ, because `p` was not cached.
      intro a ha b hb
      -- `b` is `p` itself.
      simp only [List.mem_singleton] at hb
      -- So `a = b` would put `p` among the old keys.
      intro same
      -- Replace `b` by `p`, then `a` by `p`.
      subst hb; subst same
      exact fresh ha
    -- Dropping the front peer keeps a repeat-free line repeat-free.
    -- Unfold the `let` that names the grown line.
    simp only []
    -- One goal for the overflow case and one for the room-left case.
    split
    -- The evicted line is the grown line minus its front.
    · have tail_keys : keys (c ++ [(p, d)]).tail = (keys (c ++ [(p, d)])).tail := by
        -- Taking keys and dropping the front commute.
        simp [keys, List.map_tail]
      -- Restate the goal as dropping the front of the grown line's keys.
      rw [tail_keys]
      -- A sub-list of a repeat-free list is repeat-free.
      exact List.Nodup.sublist (List.tail_sublist _) grown
    -- Nothing dropped: the grown line itself.
    · exact grown

/-- Right after an insert, the peer is cached with the DID just written. -/
theorem inserted_is_cached (cap : Nat) (c : Cache) (p d : Nat) (room : 0 < cap) :
    (p, d) ∈ insert cap c p d := by
  -- Split on refresh versus first insert.
  -- Open up `insert` so its two branches show.
  unfold insert
  -- One goal per branch: refresh, then first insert.
  split
  -- A refresh rewrites the pair that already carries `p`.
  · -- Name the fact that `p` was cached.
    rename_i cached
    -- Some pair in line carries `p`.
    obtain ⟨e, he, hp⟩ := List.mem_map.mp cached
    -- That pair becomes `(p, d)`.
    exact List.mem_map.mpr ⟨e, he, by simp [hp]⟩
  -- A first insert puts `(p, d)` at the back, which survives losing the front.
  · -- Unfold the `let` that names the grown line.
    simp only []
    -- One goal for the overflow case and one for the room-left case.
    split
    -- Overflow: the line held at least `cap ≥ 1` pairs, so the back is not the front.
    · -- Name the fact that the grown line overflowed.
      rename_i over
      -- The old line is not empty, so it has a front pair `e`.
      cases c with
      -- An empty old line cannot overflow with a positive `cap`.
      | nil => simp at over; omega
      -- Dropping `e` leaves the rest of the old line and then `(p, d)`.
      | cons e rest => simp
    -- No overflow: `(p, d)` is still at the back.
    · simp

end PeerIdentityCache
