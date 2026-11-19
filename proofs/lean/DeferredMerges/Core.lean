/-! The index of deferred and parked composites, as one node keeps it.

The story: a composite (one change to a document) that cannot merge yet is
deferred. Governance defers it until something it awaits arrives (another
composite, or a field value). The ACP gate parks it when a remote provider
refused its signer: it then waits under its document, and under the refused
composite, for the document's relationships to change. Either way the node
files it in an in-memory index under each key it waits for, so that the
arrival of that key, or a change of that document, hands it back to be
tried again (`release`). A bounded sweep hands back everything parked under
some documents (`release_parked`). A composite that merged drops its own
entry (`forget`); the governance sweep can queue a composite directly
(`enqueue_ready`).

The index is bounded: at most `capacity` composites in all, at most
`perKey` composites under one key, and at most `perComposite` keys per
composite. A composite past a bound is simply not indexed (or, deferred by
governance, not under the full key): it stays unmerged, and the node's
slower retry paths try it again. A parked composite whose document is full
is not indexed at all, since its document is what releases it.

What must never happen: a bound exceeded; a key naming a composite the
index no longer holds, or a held composite missing from a key it is filed
under (a dangling entry); filing a composite pushing out another one; the
sweep's release of parked composites handing back a governance waiter.

This file is the model. The proofs are in `DeferredMerges/Invariant.lean`,
`DeferredMerges/Release.lean` and `DeferredMerges/Park.lean`.

The model is sequential: one operation at a time. The Rust is lock-free and
its operations are atomic per map, not across maps; a race costs only a
composite's fast path, never its bound (see the Rust type's comment).
Rust owner: crates/database/db/src/merge/governance/deferred.rs DeferredMerges
(defer, park, release, release_parked, enqueue_ready, take_ready, forget,
take_entry, unindex; MAX_DEFERRED_COMPOSITES, MAX_WAITERS_PER_DEPENDENCY,
MAX_AWAITED_PER_COMPOSITE), over crates/database/db/src/merge/governance/awaited.rs
WaitKey; parking by crates/database/db/src/merge/merge_handler/composite.rs
prepare_composite_merge. -/
namespace DeferredMerges

/-- A composite's CID, as a number. -/
abbrev Cid := Nat

/-- What a composite can wait for (`WaitKey`). -/
inductive Key where
  /-- Another composite, by CID (`WaitKey::Composite`). -/
  | composite (cid : Cid)
  /-- A field value, named by a number (`WaitKey::ImmutableField`). -/
  | field (value : Nat)
  /-- The relationships of one document on a remote provider
  (`WaitKey::AcpObject`). -/
  | acpObject (doc : Nat)
  deriving DecidableEq, Repr

/-- Whether a key is a document's relationships: what parks a refusal. -/
def Key.isAcp : Key → Bool
  -- A document key.
  | .acpObject _ => true
  -- A composite or a field value: governance's keys.
  | _ => false

/-- The bounds of the index. -/
structure Limits where
  /-- Most composites indexed at once (`MAX_DEFERRED_COMPOSITES`, or what
  `set_capacity` set). -/
  capacity : Nat
  /-- Most composites under one key (`MAX_WAITERS_PER_DEPENDENCY`). -/
  perKey : Nat
  /-- Most keys one composite is filed under (`MAX_AWAITED_PER_COMPOSITE`). -/
  perComposite : Nat

/-- The bounds the Rust uses in production. -/
def production : Limits :=
  -- 16,384 composites per node, 1,024 per key, 64 keys per composite.
  ⟨16384, 1024, 64⟩

/-- One indexed composite and the keys it is filed under (`Entry`). -/
structure Entry where
  /-- The composite. -/
  cid : Cid
  /-- The keys it is filed under. -/
  keys : List Key

/-- Read a key from rows of keys and what they hold: the first row for the
key answers; a key with no row holds nobody. -/
def rowsGet : List (Key × List Cid) → Key → List Cid
  -- No row: nobody.
  | [], _ => []
  -- The first row answers for its key; any other key is read further on.
  | (k, v) :: rest, k' => if k' = k then v else rowsGet rest k'

/-- The composites filed under each key, written out as rows (`waiters`).
Read as a function of the key. -/
structure Waiters where
  /-- One row per key that holds someone. -/
  rows : List (Key × List Cid)

/-- What key `k` holds. -/
def Waiters.get (w : Waiters) (k : Key) : List Cid :=
  -- Read the rows.
  rowsGet w.rows k

/-- Waiters read as a function of the key. -/
instance : CoeFun Waiters (fun _ => Key → List Cid) :=
  -- Reading is `get`.
  ⟨Waiters.get⟩

/-- What the index holds. -/
structure State where
  /-- The indexed composites (`entries`). -/
  entries : List Entry
  /-- The composites filed under each key (`waiters`); a key nobody waits
  for has none. -/
  waiters : Waiters
  /-- Composites handed back, in order (`ready`). -/
  ready : List Cid
  /-- Composites handed back and not yet taken (`queued`). -/
  queued : List Cid

/-- The empty index. -/
def empty : State :=
  -- Nothing indexed, nothing waiting, nothing handed back.
  ⟨[], ⟨[]⟩, [], []⟩

/-- The CIDs of the indexed composites. -/
def cids (s : State) : List Cid :=
  -- One CID per entry.
  s.entries.map (·.cid)

/-- Change what one key holds: its old row goes, and a new row comes first
unless the key now holds nobody. -/
def update (w : Waiters) (k : Key) (v : List Cid) : Waiters :=
  -- The new row, if any, then every other key's rows.
  ⟨(if v = [] then [] else [(k, v)]) ++ w.rows.filter (fun r => r.1 != k)⟩

/-- The keys composite `c` is filed under, if it is indexed. -/
def lookup (s : State) (c : Cid) : Option (List Key) :=
  -- Find its entry and read its keys.
  (s.entries.find? (·.cid == c)).map (·.keys)

/-- Take `c` out of each key in `ks` (`unindex`). A key left with nobody is
the same as a key nobody waits for. -/
def unindex (c : Cid) : List Key → Waiters → Waiters
  -- No key left: done.
  | [], w => w
  -- Take `c` out of the first key, then out of the rest.
  | k :: ks, w => unindex c ks (update w k ((w k).filter (· ≠ c)))

/-- Remove `c`'s entry, if any, and take it out of every key it is filed
under (`take_entry`). Returns its keys when it was indexed. -/
def takeEntry (c : Cid) (s : State) : Option (List Key) × State :=
  -- Is `c` indexed?
  match lookup s c with
  -- No: nothing changes.
  | none => (none, s)
  -- Yes: drop its entry and unfile it from its keys.
  | some ks =>
    (some ks, { s with entries := s.entries.filter (·.cid ≠ c), waiters := unindex c ks s.waiters })

/-- File `c` under each key in turn while that key has room and does not
hold it already (the loop in `defer`). Returns the keys it was filed under
and the new waiters. -/
def file (L : Limits) (c : Cid) : List Key → Waiters → List Key × Waiters
  -- No key left: filed under nothing more.
  | [], w => ([], w)
  -- The first key has room and does not hold `c`: file it there, then go on.
  | k :: ks, w =>
    if (w k).length < L.perKey ∧ c ∉ w k then
      -- File under `k`, then under the rest.
      let r := file L c ks (update w k (w k ++ [c]))
      -- `k` is one of the keys it is filed under.
      (k :: r.1, r.2)
    -- Otherwise skip this key and go on.
    else file L c ks w

/-- Defer composite `c` until any of `awaiting` (`defer`, and parking). Its
old entry goes first. Then nothing is filed when it awaits nothing, when
the index is full, or when no key has room. -/
def defer (L : Limits) (c : Cid) (awaiting : List Key) (s : State) : State :=
  -- Drop any entry `c` already has.
  let s := (takeEntry c s).2
  -- It awaits nothing: nothing to file.
  if awaiting = [] then s
  -- The index is full: not indexed.
  else if L.capacity ≤ s.entries.length then s
  else
    -- File it under the first `perComposite` keys that have room.
    let r := file L c (awaiting.take L.perComposite) s.waiters
    -- No key had room: not indexed.
    if r.1 = [] then s
    -- Otherwise it is indexed under the keys it was filed under.
    else { s with entries := s.entries ++ [⟨c, r.1⟩], waiters := r.2 }

/-- Park composite `c`, whose frame `refused` the provider refused, under
its document `doc` and under the refused composite (`park`, called by
`prepare_composite_merge` returning `Deferral::Parked` for a
`Refusal::Parked`). Its document is what releases it, so it is indexed only
when its document has room: past that bound it is not indexed at all, not
even under the refused composite, and its old entry, if any, is gone. -/
def park (L : Limits) (c : Cid) (doc : Nat) (refused : Cid) (s : State) : State :=
  -- The index once `c`'s old entry, if any, is gone.
  let t := (takeEntry c s).2
  -- Its document has room: filed as a defer under the two keys it waits for.
  if (t.waiters (.acpObject doc)).length < L.perKey then
    defer L c [.acpObject doc, .composite refused] s
  -- Its document is full: not indexed.
  else t

/-- Hand back one composite a released key held (the inner loop of
`release`): whoever takes its entry queues it, once. -/
def releaseOne (s : State) (c : Cid) : State :=
  -- Try to take its entry.
  match takeEntry c s with
  -- Another key's release took it already: nothing to do.
  | (none, s) => s
  -- Taken: queue it unless it is queued already.
  | (some _, s) =>
    if c ∈ s.queued then s else { s with queued := s.queued ++ [c], ready := s.ready ++ [c] }

/-- Release one key: remove it, then hand back everything it held. -/
def releaseKey (s : State) (k : Key) : State :=
  -- What the key held, then the key removed, then each of those handed back.
  (s.waiters k).foldl releaseOne { s with waiters := update s.waiters k [] }

/-- Release every key in `ks`, in order (`release`). -/
def release (s : State) (ks : List Key) : State :=
  -- One key after another.
  ks.foldl releaseKey s

/-- A list of keys without repeats: each kept at its last place. -/
def dedup : List Key → List Key
  -- Nothing: nothing.
  | [] => []
  | k :: ks =>
    -- The rest without repeats, worked out once.
    let rest := dedup ks
    -- Keep `k` only if the rest does not hold it.
    if k ∈ rest then rest else k :: rest

/-- Every key some indexed composite is filed under, once each: the keys of
the Rust `waiters` map (`present_iff` in `Invariant.lean` proves they are
exactly the keys holding someone). -/
def presentKeys (s : State) : List Key :=
  -- All keys of all entries, without repeats.
  dedup (s.entries.flatMap (·.keys))

/-- The document keys the sweep picks: up to `n` of them. -/
def pickParked (n : Nat) (s : State) : List Key :=
  -- The present document keys, at most `n`.
  ((presentKeys s).filter Key.isAcp).take n

/-- Release what is parked under up to `n` documents (`release_parked`).
Returns how many documents were released. The Rust picks in its map's
order; the theorems hold for every pick of document keys. -/
def releaseParked (n : Nat) (s : State) : Nat × State :=
  -- The documents picked, how many, and their release.
  let ks := pickParked n s
  (ks.length, release s ks)

/-- Queue `c` directly (`enqueue_ready`): once, taking its entry if it has
one. Returns whether it was queued now. -/
def enqueueReady (c : Cid) (s : State) : Bool × State :=
  -- Already queued: nothing happens.
  if c ∈ s.queued then (false, s)
  else
    -- Take its entry, if any, and queue it.
    let s := (takeEntry c s).2
    (true, { s with queued := s.queued ++ [c], ready := s.ready ++ [c] })

/-- Take the next handed-back composite (`take_ready`). -/
def takeReady (s : State) : Option (Cid × State) :=
  -- Look at the front of the queue.
  match s.ready with
  -- Nothing handed back.
  | [] => none
  -- The first one leaves the queue and stops being queued.
  | c :: rest => some (c, { s with ready := rest, queued := s.queued.filter (· ≠ c) })

/-- Drop a merged composite's own entry (`forget`). Returns whether it had
one. -/
def forget (c : Cid) (s : State) : Bool × State :=
  -- Take the entry and say whether there was one.
  let r := takeEntry c s
  (r.1.isSome, r.2)

/-- One operation on the index. -/
inductive Op where
  /-- Governance defers `c` until any of `keys`. -/
  | defer (c : Cid) (keys : List Key)
  /-- The gate parks `c` under document `doc` and refused composite `refused`. -/
  | park (c : Cid) (doc : Nat) (refused : Cid)
  /-- These keys arrived or changed. -/
  | release (keys : List Key)
  /-- The sweep releases up to `n` documents. -/
  | releaseParked (n : Nat)
  /-- The governance sweep queues `c`. -/
  | enqueueReady (c : Cid)
  /-- Re-drive takes the next handed-back composite. -/
  | takeReady
  /-- `c` merged and drops its entry. -/
  | forget (c : Cid)

/-- Apply one operation. -/
def step (L : Limits) (s : State) : Op → State
  -- Each operation by its function, keeping only the new index.
  | .defer c ks => defer L c ks s
  | .park c doc r => park L c doc r s
  | .release ks => release s ks
  | .releaseParked n => (releaseParked n s).2
  | .enqueueReady c => (enqueueReady c s).2
  | .takeReady => match takeReady s with | some (_, s) => s | none => s
  | .forget c => (forget c s).2

/-- Apply operations in order. -/
def run (L : Limits) (s : State) (ops : List Op) : State :=
  -- One operation after another.
  ops.foldl (step L) s

end DeferredMerges
