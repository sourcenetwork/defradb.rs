/-!
# SingleFlight.Join: the CID guard is exclusive, and a joined PushLog acks only a safe block

The story, in plain words. A node lets one request at a time work on a block
name (a CID). The table of owners says, for each CID, whether some request owns
it. Taking a CID works only when nobody owns it; letting it go makes it free
again. When a second PushLog for the same CID arrives while the first is still
working, it joins: after the owner lets go it looks at the block and decides
one of three things: answer success, process its own copy, or wait again.

What must never happen:
* two requests own one CID at once (`acquire_exclusive`);
* a joined request answers success while the block is not safe
  (`join_acks_only_durable`);
* a joined request processes its copy while someone else owns the CID
  (`join_processes_only_unowned`).

The temporal story (two senders, failures, the deadline) is the TLA+ model
`proofs/tla/PushLogJoin.tla`; this file carries the two laws that model takes on
trust about one step.

Rust anchors: `ProcessQueue::try_acquire` and `ProcessQueue::release_sync`
(`crates/p2p/src/sync/queue.rs`, an atomic `insert_if_absent` and `force_remove`
on a lock-free map, modeled here as their sequential meaning),
`SyncManager::join_in_flight` (`crates/p2p/src/sync/manager/process/pushlog.rs`),
and the root claim a DocSync or branchable reply takes before it fetches
(`DagFetchLimiter::claim_root` and `RootClaim::drop` in
`crates/p2p/src/sync/coordinator/mod.rs`, the same `insert_if_absent` and
`remove`; `tryAcquire`, `release` and their laws are its meaning too).
-/

namespace SingleFlight

-- The owners table: for each CID (a number here), whether some request owns it.
abbrev Owners := Nat → Bool

-- Try to take a CID: nothing when it is owned already, else the table with it owned.
def tryAcquire (owners : Owners) (cid : Nat) : Option Owners :=
  -- If someone owns the CID, taking it fails;
  if owners cid then none
  -- otherwise the new table marks this CID owned and leaves every other CID as it was.
  else some (fun c => if c = cid then true else owners c)

-- Let a CID go: afterwards nobody owns it, and every other CID is unchanged.
def release (owners : Owners) (cid : Nat) : Owners :=
  -- This CID becomes free; the rest of the table is copied as it was.
  fun c => if c = cid then false else owners c

-- Promise: a successful take leaves the CID owned.
theorem acquire_marks_owned (owners next : Owners) (cid : Nat)
    -- Given that taking the CID succeeded and produced the table `next`,
    (h : tryAcquire owners cid = some next) :
    -- the CID is owned in `next`.
    next cid = true := by
  -- Unfold what taking means, then split on whether the CID was owned before.
  unfold tryAcquire at h
  -- Look at both cases of "was it owned?".
  split at h
  -- It was owned: taking returned nothing, which contradicts the success we assumed.
  · cases h
  -- It was free: the new table is the one built above, and it marks the CID owned.
  · cases h; simp

-- Promise: the guard is exclusive. Once a take succeeded, a second take of the
-- same CID fails until the CID is let go.
theorem acquire_exclusive (owners next : Owners) (cid : Nat)
    -- Given that the first take succeeded and produced the table `next`,
    (h : tryAcquire owners cid = some next) :
    -- a second take on `next` fails.
    tryAcquire next cid = none := by
  -- The CID is owned in `next` (the promise just above),
  have owned := acquire_marks_owned owners next cid h
  -- and taking an owned CID fails by definition.
  simp [tryAcquire, owned]

-- Promise: after the CID is let go, a take of it succeeds again.
theorem release_then_acquire (owners : Owners) (cid : Nat) :
    -- Taking the CID on the table where it was just let go returns some table.
    (tryAcquire (release owners cid) cid).isSome = true := by
  -- The CID is free in the released table, so the take takes the success branch.
  simp [tryAcquire, release]

-- Promise: taking one CID never changes who owns another CID.
theorem acquire_other_unchanged (owners next : Owners) (cid other : Nat)
    -- Given a different CID `other`
    (hne : other ≠ cid)
    -- and a successful take of `cid` producing `next`,
    (h : tryAcquire owners cid = some next) :
    -- `other` is owned in `next` exactly when it was owned before.
    next other = owners other := by
  -- Unfold the take and split on whether `cid` was owned.
  unfold tryAcquire at h
  -- Both cases of "was `cid` owned?".
  split at h
  -- Owned: the take failed, contradicting the success we assumed.
  · cases h
  -- Free: the new table only changed `cid`, and `other` is a different CID.
  · cases h; simp [hne]

-- What a joined request does after it looks at the block once.
inductive Step where
  -- answer success to the sender;
  | ack
  -- take the guard and process its own copy;
  | process
  -- wait for the current owner to let go, then look again.
  | wait
  deriving DecidableEq, Repr

-- The join's decision: success when the block is safe (merged or registered
-- for recovery); else process when nobody owns the CID; else wait.
def joinStep (durable owned : Bool) : Step :=
  -- A safe block is answered with success,
  if durable then .ack
  -- an unsafe block someone owns means waiting for them,
  else if owned then .wait
  -- and an unsafe block nobody owns is processed by this request.
  else .process

-- Promise: a joined request answers success only when the block is safe.
theorem join_acks_only_durable (durable owned : Bool)
    -- Given that the decision was to answer success,
    (h : joinStep durable owned = .ack) :
    -- the block was safe.
    durable = true := by
  -- Try both values of `durable` and `owned`; every case but "safe" yields another step.
  cases durable <;> cases owned <;> simp_all [joinStep]

-- Promise: a joined request processes its copy only when nobody owns the CID.
theorem join_processes_only_unowned (durable owned : Bool)
    -- Given that the decision was to process,
    (h : joinStep durable owned = .process) :
    -- nobody owned the CID.
    owned = false := by
  -- Try both values of each input; processing is chosen only when the CID was free.
  cases durable <;> cases owned <;> simp_all [joinStep]

end SingleFlight
