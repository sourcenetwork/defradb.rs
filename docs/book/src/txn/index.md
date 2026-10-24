# Transactions

Transactions are the centre of defradb.rs, and the centre of the centre is a division of
labour: **[regolith](https://sourcenetwork.github.io/regolith/) owns the machinery** (snapshots, validation, conflicts, retries,
ordering, durability), **defradb.rs owns the meaning** (what each document write is, what
each conflict means for the caller).

This part covers:

- **Lifecycle and states**: every state a transaction passes through, from `Open` to
  `Done`, including the write-free fast path and the re-run with a reason.
- **The commit path**: group commit and the lock-free pipeline; how a commit becomes
  durable, applied and visible, and how the ticket returns on the committer's own queue.
- **Conflict rules**: the exact table of what conflicts with what, per key class and per
  operation, and the reason each conflict carries.
- **Conflict business logic**: what defradb.rs does with each reason, race by race.
- **Explicit transactions**: interactive transactions owned by one worker, their registry,
  statement savepoints, idle sweeping and commit fences.
