import HeadSet.Core

namespace HeadSet

/-- The cache contains the visible head keys AND supersede markers, including
markers for parents not yet received. Keeping only current tips would resurrect
an out-of-order parent. Values/priorities are copied unchanged by the native
adapter; this model abstracts keys to block ids as in `Store`.

A cache is process-local and disposable. It changes neither durable keys nor the
write sets proved disjoint in `derived_writeSets_disjoint`. Snapshot acquisition,
durable commit plus cache publication, and invalidation share one publication
boundary. No lock is held for the lifetime of an application transaction. -/
structure HeadCache where
  durable : Store
  resident : Option Store
  epoch : Nat
  /-- Physical deleted entries remain an engine cost, not logical cache rows. -/
  tombstones : Nat := 0
  deriving Repr, DecidableEq

def cacheExact (s : HeadCache) : Prop :=
  ∀ value, s.resident = some value → value = s.durable

/-- An immutable transaction snapshot. Pending writes belong to the transaction
and must never be included in a shared cold fill. -/
structure HeadRead where
  durable : Store
  resident : Option Store
  epoch : Nat
  deriving Repr, DecidableEq

def capture (s : HeadCache) : HeadRead :=
  ⟨s.durable, s.resident, s.epoch⟩

def readBase (r : HeadRead) : Store := r.resident.getD r.durable

/-- Commit applies the delta to the latest resident state, not the writer's
snapshot. Publishing a writer's whole snapshot loses a concurrent sibling. The
storage owner publishes only after the engine has accepted the transaction. -/
def publish (s : HeadCache) (delta : Store → Store) : HeadCache :=
  { s with durable := delta s.durable
           resident := s.resident.map delta
           epoch := s.epoch + 1 }

def evict (s : HeadCache) : HeadCache := { s with resident := none }

/-- A cold fill is admitted only if no head-store commit or invalidation has
crossed its captured epoch. A rejected fill still answers its own snapshot. -/
def fill (s : HeadCache) (r : HeadRead) : HeadCache :=
  if r.epoch = s.epoch then { s with resident := some r.durable } else s

/-- Transactions apply their own pending writes after selecting the captured
base. This includes arbitrary puts/deletes; the native adapter preserves bytes
and write order. There is no special-case cache mutation in an append or merge. -/
def readHeads (r : HeadRead) (pending : Store → Store) : List BlockId :=
  derivedHeads (pending (readBase r))

theorem captured_base_exact (s : HeadCache) (h : cacheExact s) :
    readBase (capture s) = s.durable := by
  cases hr : s.resident with
  | none => simp [readBase, capture, hr]
  | some value => simpa [readBase, capture, hr] using h value hr

theorem cached_read_exact (s : HeadCache) (h : cacheExact s)
    (pending : Store → Store) :
    readHeads (capture s) pending = derivedHeads (pending s.durable) := by
  simp [readHeads, captured_base_exact s h]

theorem publish_preserves_exact (s : HeadCache) (h : cacheExact s)
    (delta : Store → Store) : cacheExact (publish s delta) := by
  intro value hv
  cases hr : s.resident with
  | none => simp [publish, hr] at hv
  | some previous =>
    have heq := h previous hr
    simp [publish, hr] at hv
    simp [publish, ← hv, heq]

theorem evict_preserves_exact (s : HeadCache) : cacheExact (evict s) := by
  simp [cacheExact, evict]

/-- Epoch equality implies the captured durable projection has not changed.
The publication owner must increment the epoch for EVERY mutation of either
head prefix, including root-store writes, bulk import and drop/reset. Bypassing
that owner disables caching; a stale fill must not re-enable it. -/
theorem fill_preserves_exact (s : HeadCache) (r : HeadRead)
    (h : cacheExact s) (sameEpoch : r.epoch = s.epoch → r.durable = s.durable) :
    cacheExact (fill s r) := by
  unfold fill
  split
  · rename_i he
    intro value hv
    simp at hv
    simpa [← hv] using sameEpoch he
  · exact h

theorem stale_fill_rejected (s : HeadCache) (r : HeadRead)
    (h : r.epoch ≠ s.epoch) : fill s r = s := by simp [fill, h]

def publishMany (s : HeadCache) (deltas : List (Store → Store)) : HeadCache :=
  deltas.foldl publish s

theorem publication_epoch (s : HeadCache) (deltas : List (Store → Store)) :
    (publishMany s deltas).epoch = s.epoch + deltas.length := by
  induction deltas generalizing s with
  | nil => simp [publishMany]
  | cons delta rest ih =>
    simp only [publishMany, List.foldl_cons]
    rw [← publishMany, ih]
    simp [publish, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]

/-- The stale-fill guard follows from the publication transition, rather than
assuming that independently captured snapshots happen to agree. -/
theorem same_epoch_same_store (s : HeadCache) (deltas : List (Store → Store))
    (h : s.epoch = (publishMany s deltas).epoch) :
    s.durable = (publishMany s deltas).durable := by
  rw [publication_epoch] at h
  have hn : deltas.length = 0 := by omega
  have hd : deltas = [] := List.length_eq_zero_iff.mp hn
  simp [hd, publishMany]

/-- Failed/discarded work does not publish. Existing readers keep their immutable
snapshot even after a successful commit, eviction, or reclamation. -/
theorem captured_read_stable (r : HeadRead) (pending : Store → Store)
    (before after : HeadCache) :
    (before, readHeads r pending).2 = (after, readHeads r pending).2 := rfl

theorem cached_prune_preserves_heads (s : HeadCache) (h : cacheExact s)
    (b : BlockId) (hsup : isSuperseded s.durable b = true) (head : BlockId) :
    head ∈ readHeads (capture (publish s (fun store => applyPrune store b))) id ↔
      head ∈ readHeads (capture s) id := by
  rw [cached_read_exact _ (publish_preserves_exact s h _), cached_read_exact s h]
  exact prune_preserves_derivedHeads s.durable b hsup head

/-- Logical rows visited by a warm query. The resident budget bounds these rows;
old readers may pin older versions until they finish, as with engine snapshots.
Cold reads and entries over budget retain the durable scan and its history cost. -/
def warmWork (r : HeadRead) : Nat :=
  (readBase r).headKeys.length + (readBase r).supersedes.length

theorem warm_work_ignores_tombstones (s : HeadCache) (history : Nat) :
    warmWork (capture { s with tombstones := history }) = warmWork (capture s) := rfl

/-- Collection CIDs bind the parents and payload. Replaying an identical block
may be admitted even when a raw scan's stretch would have made the engine reject
an idempotent overlapping write. This cannot change the logical head set.
General KV scans retain native validation. Reclamation instead anchors every
shared deletion with `has_for_update`, so a changed key aborts the sweep. -/
theorem replay_preserves_heads (s : Store) (blk : Block) (head : BlockId) :
    head ∈ derivedHeads (applyDerived (applyDerived s blk) blk) ↔
      head ∈ derivedHeads (applyDerived s blk) := by
  simp [derivedHeads, applyDerived, isSuperseded, List.any_append,
    List.any_map, Bool.or_assoc, Bool.or_self]

/-- A concrete counterexample to keeping only the live tips in the cache: the
child arrived before its parent, so the marker must survive the cold fill. -/
theorem tips_only_resurrects_parent :
    let durable : Store := ⟨[2], [(1, 2)]⟩
    let tipsOnly : Store := ⟨derivedHeads durable, []⟩
    let parent : Block := ⟨1, []⟩
    1 ∉ derivedHeads (applyDerived durable parent) ∧
      1 ∈ derivedHeads (applyDerived tipsOnly parent) := by decide

end HeadSet

#print axioms HeadSet.cached_read_exact
#print axioms HeadSet.publish_preserves_exact
#print axioms HeadSet.fill_preserves_exact
#print axioms HeadSet.cached_prune_preserves_heads
#print axioms HeadSet.warm_work_ignores_tombstones
#print axioms HeadSet.tips_only_resurrects_parent

#print axioms HeadSet.replay_preserves_heads
#print axioms HeadSet.same_epoch_same_store
