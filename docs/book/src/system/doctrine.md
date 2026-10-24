# Design doctrine

Every subsystem chapter in this book ends with a "why". This chapter collects the rules
that produced those answers. They are not aspirational: they are enforced rules of the
repository, written down for every contributor and checked in review.

## The engine owns the transaction

[regolith](https://sourcenetwork.github.io/regolith/), run at `IsolationLevel::DefraLevel`, owns everything transactional: validation,
conflicts and their reasons, retries, merges, commit-ordered append, ID allocation, the
commit receipt, and encryption at rest. defradb.rs implements regolith's traits
(`KeyClassifier`, `MergeOperator`, `LogLayout`, `EventListener`, `KeyProvider`,
`PrefixExtractor`, `TransactionHooks`) and calls its methods.

The consequence is a negative rule with real teeth: defradb.rs carries *no* transaction
mechanics. No lock around storage, no retry loop, no read outside a transaction feeding a
decision inside it, no key-layout trick to dodge a conflict, no pending state, no second
commit, no guard. A conflict problem is solved by a regolith mechanism. Changes only ever
remove coordination from defradb.rs, never add it.

> **Why.** Every lock, retry loop and guard is a second implementation of something the
> engine already does, and the two copies drift. The history of this codebase includes a
> shared transaction mutex, per-document write queues, an arrival guard, a batch gate and
> eleven retry loops; all were deleted, one measured step at a time, and the engine's own
> mechanisms replaced them; the plan that deleted them, step by gated step, is recorded in the repository's docs.

## Pick the mechanism by what the decision depends on

The reads-and-writes rules are the heart of the design, and each chapter of this book
instantiates them:

- A decision on a whole value: `get` (an Ordinary key), validated at commit. An identical
  rewrite is not a change.
- A decision on some parts of a document: `get_parts(key, parts)`, validated only against
  writes that touch those parts. Disjoint parts of one document never conflict.
- Traversal hints that decide nothing (HNSW neighbours, head sets): `get_parts(key, &[])`,
  never validated.
- A decision on what exists in a range (ACP tuples, a claim queue): a validated scan, so a
  write inside the range aborts the decision.
- Writes that commute (blocks, head sets, document part operands, secondary index entries,
  counters, journal appends): blind puts, merges and appends that never conflict.
- Races that need one winner (unique keys, the DocID mapping, deletion markers, DDL,
  ACP tuples, the applied key of a remote block): ordinary validated keys whose loss
  surfaces as a typed `Conflict`.

> **Why.** A read that validates more than its decision needs is a defect: it manufactures
> conflicts out of writes that could not have changed the answer. The mechanism is picked
> so that the set of conflicting writes is exactly the set of writes that change the
> decision.

## Everything with an outside effect is prepared before the transaction

A transaction closure may run more than once. KMS keys, embeddings, signatures and short
IDs are computed before `transact`; nothing remote is fetched inside a closure. An input
that depends on the transaction's own reads ends the attempt, the input is fetched at the
edge, and the attempt runs again with the input memoized by its exact request bytes.

> **Why.** Remote calls inside a retried closure are either wasted (fetched again per
> attempt) or wrong (the answer changes between attempts). Preparing inputs makes attempts
> deterministic and fast, and a signed or encrypted block is never redone.

## Lock-free, thread-per-core, one network runtime

Synchronization comes from `regolith::sync` (atomics, lock-free maps, queues, channels).
The final code has no `std::sync::Mutex` and no `std::sync::RwLock` anywhere in the
workspace, and lints reject holding a lock across an `.await`. Shared values are immutable
snapshots swapped atomically; data moves between threads as messages, never as shared
memory. Every core runs one pinned executor, no task steals, and the network lives on one
tokio runtime at the edge. The [Threading part](../threading/index.md) is the full story.

> **Why.** Throughput and tail latency. A mutex on the data path serializes cores that
> have nothing to do with each other; task migration wrecks caches and makes wakeups
> cross cores; a second runtime makes shutdown and ordering undefined. The engine's
> per-thread I/O queues only work if a thread's completions arrive on that thread, which
> requires the task to still be there.

## Business logic owns the meaning of each conflict

A conflict is not an error to hide: regolith returns it with a reason (whose write won,
on which key, at which sequences), and defradb.rs decides what it means. An already-deleted
document is a no-op; a changed part re-runs the decision on the new value; a lost
unique-key race is reported to the client; a revoked ACP tuple denies the write; a changed
definition re-runs against the new one. One retry policy for every attempt, from
configuration, through one function.

> **Why.** Only the business layer knows whether a lost race is an error (the user's
> compare-and-set failed), a retry (re-decide on fresh data) or a success (the delete
> already happened). The engine cannot know; the client must not have to.

## Nothing valid is rejected

A clean refusal is not a safer wrong answer. A request the product is supposed to serve is
served; if the wiring is missing, the wiring is the work. "Fail loud" governs states the
system cannot know (corruption, missing configuration), never features.

## Formal models beside the code

No logic change without its models: temporal and concurrent behaviour gets a TLA+ model,
data structures and merge laws get Lean proofs, both written before the code and both run
in CI. The [Verification chapter](../proofs/index.md) maps mechanism to model.

> **Why.** The invariants here (convergence, ordering, exactly-once, no public window for
> ACP) are exactly the class of property that testing cannot establish and model checking
> can. The ELI5 rule for proof files (every line explained in plain words) keeps them
   useful to every developer, not just the author.
