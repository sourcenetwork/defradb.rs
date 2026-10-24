# Why thread-per-core

The threading model is not a style preference; it is the shape the rest of the design
pushes you towards. Three pressures, one conclusion.

## Pressure one: the engine completes I/O on the asking thread

[regolith](https://sourcenetwork.github.io/regolith/)'s non-blocking API is built on per-thread `IoQueue`s: a `CacheOnly` read miss or
a pending commit ticket becomes a unit on the caller's own queue, and the completion
arrives on that same queue. That design exists to make I/O wait-free for everyone else:
a busy core is never woken to service someone else's completion, and no core ever waits
on a lock held by a thread that is blocked on the device.

But it only works if the task that awaits is *still on that thread* when the completion
arrives. A work-stealing executor that migrates the task to another core between the
await and the wakeup breaks the invariant: the completion lands on a queue whose owner
is not waiting for it, and the task is on a queue that will never see it. So the
executor must be local, and tasks must not migrate.

## Pressure two: shared mutable state is the tax on cores

Every cache line a second core touches is coherence traffic; every mutex a second
contender takes is a cross-core round trip that can park both. The database's hot state
(per-collection indexes, buffered iterators, transaction objects, CRDT accumulators) is
vastly cheaper if one core owns it and it never leaves L2. defradb.rs leans into that:
a worker's state lives on the worker; read-mostly state (schema definitions, the ACP
policy snapshot) is an immutable snapshot published atomically and *copied* per worker;
anything that must cross threads crosses as a message, never as shared memory.

## Pressure three: the workload has natural affinity

A single document's writes and merges must be ordered somewhere: regolith validates
every commit wherever it runs, but one-at-a-time-per-document is what makes conflicts
rare (the race gallery exists *between* racing decisions, not inside one document's
stream). Document affinity (hash the collection and DocID to a worker) gives that
ordering for free, with no lock, as long as the router is stable and requests actually
go there. That requires routing at the edge, once, and no hopping.

## What it buys

- **No task stealing**: wakeups never cross cores; caches stay warm; tail latency stops
  being a scheduling lottery.
- **No locks on the data path**: ownership replaces locking. Where two tasks must
  coordinate, they are on the same core, or they go through a lock-free queue.
- **Throughput that scales with cores**: each core runs its full pipeline (parse to
  commit) independently; the only shared points are the engine's own lock-free
  structures and the commit pipeline's helping protocol.
- **One code path**: the same executor, the same queue discipline, on one core (wasm,
  embedded) or sixty-four. Nothing requires a second thread.

## The discipline it demands

Thread-per-core is a contract, not a feature: nothing on a worker may block the thread
(no `block_on`, no `thread::sleep`, no spin-wait); long tasks yield after a bounded
slice so the core stays fair; every wait is an `.await` on something the worker's own
core completes; futures on the data path need not be `Send`, and that liberty is used.
The repository's lints and review rules enforce it; the [pool chapter](pool.md) shows
the machinery.

> **Why not just tokio everywhere.** tokio is an excellent *network* runtime, and
> defradb.rs keeps exactly one for that job. But a general-purpose stealing runtime is
> the wrong shape for a storage engine's data path: it migrates tasks (breaking queue
> affinity), its locks are designed for coarse sharing, and its "just spawn more tasks"
> answer to blocking work is precisely what a per-core design exists to avoid. The
> split is not tokio-versus-not; it is one runtime per job: a network runtime for
> network-shaped work, pinned cores for database-shaped work.
