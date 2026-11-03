# Memory Reclamation

defradb.rs holds a lot of derived data: every version of every field, index entries,
blocks shared across documents, advisory counters. This chapter is how data *leaves*,
how the concurrent machinery's own memory is reclaimed, and how I/O is kept off the
compute path - and why nothing leaves by accident.

## No compaction filter, on principle

The engine offers a compaction filter (drop entries during SST compaction). defradb.rs
installs none, and the reasoning is written beside the decision
([`crates/storage/src/backends/regolith/mod.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/mod.rs)):

- a head-set entry's meaning depends on writes that follow it;
- nothing expires by time alone (a pending DAG record leaves when its root merges or
  is quarantined; downsample retention is cut from other keys);
- a document part's tombstone carries the priority a late replicated merge is ordered
  against - dropping it would resurrect deleted data;
- index entries, blocks, ACP tuples and guards each *decide something* until another
  key says otherwise.

A filter that dropped any of them would change what a reader, or a later merge, sees.
Retirement is a business decision with a policy behind it, and those run as ordinary
transactions:

## Block ownership

Blocks are shared: the same field block can back multiple documents' histories. The
systemstore keeps a CID -> DocID ownership map (`/d/b/{cid}/{docID}`), written at every
create and merge. Deleting history removes *ownership* first
([`crates/db/src/block/cleanup.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/block/cleanup.rs)): a block's bytes die only when its last owner
mapping is gone and it is not reachable from anything retained. `referenced_blocks`
computes the reachable set (commit + links + encryption links) of what stays; deletes
keep blocks in that set even when their own owner is gone - the fix for the class of
bug where pruning one document's history pulled a block out from under another's.

## Downsample GC

Downsampled collections roll source documents into windowed targets; retention then
prunes source history ([`crates/db/src/downsample/gc.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/downsample/gc.rs)):

- plans are grouped per source; a source with any consumer lacking a retention policy
  is skipped entirely (never partially pruned);
- a height is pruneable only when *every* plan for the source agrees (the
  intersection), and only below its retention cutoff;
- current head CIDs are never pruned (they are the present, not history);
- pruned composite commits lose their priority-index entries and their ownership, with
  field blocks deleted only when unreferenced by anything kept;
- the whole prune is one transaction, discarded on error: a half-pruned history is
  never published.

## Collection delete and truncate

Whole-collection removal walks the full reachable DAG and deletes unowned, unretained
blocks (`delete_owned_dag`); truncate does the same filtered by a predicate. Index
entries go page by page through the index manager.

## Tombstones and markers

Deletion markers (`d/del/`) are read before materializing a document and survive until
their history is pruned. LWW tombstones (empty bytes, priority intact) survive in the
stored document for the same reason: a late merge from a replica must still lose to
the delete.

## The other reclamation: memory inside the concurrent machinery

The chapter so far reclaimed *data*. A lock-free system has a second, harder
reclamation problem: the memory of its own concurrent structures. When several cores
traverse a shared structure without locks, nobody may `drop()` or `free()` a node
while another core may still hold a pointer to it - and knowing when no core does,
without a lock, is *the* classical problem of wait-free programming.

defradb.rs's answer is **kovan**, a wait-free memory reclamation library that the whole
stack reaches through `regolith::sync` (kovan 0.1.x, with `kovan-map`,
`kovan-queue` and `kovan-channel` beside it). Its shape:

- A reader **pins** (one guard), **loads** pointers under the guard, and the guard
  keeps every loaded pointer valid until it drops. Read overhead is one atomic load
  and one comparison.
- A writer that unlinks a value **retires** it: the value is freed only when no
  guard can still reach it. Reclamation is wait-free - pin, retire, guard drop and
  flush each complete in a bounded number of the caller's own steps, whatever other
  threads (including stalled ones) do.
- Memory is **bounded**: unlike epoch-based schemes, whose pending-retirement lists
  grow with the slowest thread, batches that cannot yet be placed are accumulated or
  adopted by other threads, never dropped and never unbounded, even across thread
  exit.
- Retired nodes can be recycled through a **free list** (`Reclaimable`), so a
  structure that publishes one node per update allocates nothing in the steady
  state.

This is what sits under every `Atom`-swapped snapshot in the system. When the
[ACP policy snapshot](../acp/zanzibar.md) publishes a new immutable policy table with
one compare-and-swap, the *old* table's memory is retired and freed by kovan exactly
when the last reader's guard drops - no reference-count handoff, no epoch thread, no
lock. The same holds for the collections cache's epoch-guarded swaps and the
document-change coalescers in [events](../events/index.md). The maps and queues built
on it (`HopscotchMap` per-core hash maps, `ArrayQueue`/`SegQueue`, the channels the
[inboxes](../threading/pool.md) use) inherit the same guarantee for every node they
unlink. The reclamation protocol itself, and the maps, are modelled in TLA+ and Lean
in kovan's own proofs tree - the rule "no logic change without its models" holds one
layer below this repository too.

One more reclamation loop, small and instructive: the async synchronization
primitives (`regolith::sync`'s locks, semaphores, latches, notify) park a **waiter
node** when a contended acquire suspends. Up to three parties may hold that node
(the waiting future, the primitive's queue, a wake list); each holds one reference
bit in the node's state word, and whichever party clears the last bit returns the
node to its primitive's node pool. No other path frees a node, so a node is never
reachable by a party that has let it go - and, as the code beside it says, *no epoch
scheme is needed*: reference bits plus a pool replace general-purpose reclamation for
this one shape.

## How I/O is separated from compute

Reclamation of *waits* follows the same separation as everything else in the
threading model: I/O never runs on a data-path core, and a data-path core never
blocks on I/O.

- **Every device operation is a unit on a per-thread `IoQueue`.** A `CacheOnly` read
  miss or a pending commit ticket becomes a unit on the *asking thread's own* queue;
  the completion arrives on that same queue, and only there
  ([the worker pool](../threading/pool.md)). A busy core is never woken to service
  someone else's completion, and no core ever waits for a lock held by a thread that
  is itself blocked on the device.
- **Units are claimable, single-flight.** A block read is one unit no matter how many
  tasks await that block; a commit group's fsync is one unit claimed by the first
  member core to poll. The memory bound follows: N waiters cost one in-flight device
  operation and N waiter nodes recycled through their pools.
- **Tasks wait; threads do not.** Every wait in the data path is an `.await` on a
  future driven by the worker's own waker: the task suspends, the core runs other
  work, the completion wakes exactly that task. The synchronization primitives
  themselves make no system call - no futex, no thread park, no clock read - so
  "waiting" costs a parked waiter node, not a kernel call.
- **Background I/O has its own threads.** Compaction and log writeback, the only
  threads the engine starts, are placed by the pool as background-class threads,
  never on a caller's core ([LSM structure](../storage/lsm.md)). In-memory
  environments start none and run compaction inline by necessity.
- **The network is one runtime away.** Remote inputs are resolved at the
  [edge](../threading/edge.md) before a transaction; a worker never holds memory
  waiting on the network mid-transaction, because the doctrine forbids the network
  inside the retried closure at all.

The memory-accounting consequence is the point of this section: because waits are
per-task nodes (bounded, pooled, recycled by kovan and by the reference-bit scheme)
rather than per-thread kernel waits, the system's memory for concurrency is
proportional to in-flight work, not to thread count or device latency.

## Memory in the small

Reclamation of *RAM* follows the same bounded-by-design rules: iterators page by bytes
and entries, streaming reads hold a page, caches are byte-bounded (the KMS tier, the
vector code caches), the legacy-store rewrite copies in bounded batches, and the
store's statistics deliberately keep no histograms (the LSM chapter). The wasm and
memory environments size to their profiles.

> **Why explicit ownership instead of refcounting or GC.** Reference counting says "no
> one pointed at it *at that instant*"; replication makes that instant meaningless (a
> peer may re-deliver the block later). Ownership mappings are durable, replicated
> facts, pruned by transactions that validate what stays - so "may I delete this
> block?" is answered by the same machinery that answers every other question, and a
> wrong answer loses a commit instead of losing data.
