# Explicit transactions

An explicit (interactive) transaction spans multiple client statements: begin, several
queries and mutations, commit or discard. defradb.rs runs each one as an **actor on
one worker**: the transaction is begun on a worker of the pool and stays there, held
*by value* by one task (`crates/db/src/txn/owner/`). Its ID names the worker, so every
request for it goes from the edge straight to that worker with `Target::Worker`, one
hop in and one out, and the requests run one at a time, in arrival order, through the
actor's mailbox. There is no lock to take: the requests are messages.

## The registry

The transaction registry (`crates/db/src/txn/registry/`) maps IDs to owners:

- **begin** on a least-loaded worker, made explicit, wrapped as an `OwnedTxn`;
- **run**: a request routed to the owning worker under a deadline; the actor lends
  the transaction to the statement and takes it back alone (`Arc::try_unwrap`; a
  still-borrowed transaction is `TxnInUse`, never waited on);
- **caller isolation**: the request identity is the ownership key; a foreign caller
  gets `NotFound`, not someone else's transaction;
- **idle sweeping**: a deadline-queue job on each worker ends transactions idle past
  the timeout (default 600 s, swept every 60 s) - but only when the sweep message is
  next in the mailbox, so it can never interleave with a running statement.

Statement savepoints wrap every statement: a failed or cancelled statement rolls back
to its savepoint, and the transaction stays usable for the next one. A statement's
conflict is returned to the caller (explicit transactions do not auto-re-run; the
client is the retry policy). Only the creator can `force_commit`/`force_discard`;
plain `commit` on an explicit handle is a typed error.

## Commit fences

A subtlety with actors: the client may stop waiting (timeout, dropped connection)
*while the commit message is still in the mailbox*. The commit still runs - the work
was requested, and "the caller left" must not mean "the write vanishes" or, worse,
"the write lands after a dependent read already reported it absent". The
`CommitFence` counts commits whose caller stopped waiting, and every new transaction
waits while that count is above zero, so no new work starts until the abandoned
commits have landed. The protocol has its own TLA+ model and a Lean proof of the
counter swap ([`CommitFence.tla`](https://github.com/sourcenetwork/defradb.rs/blob/main/proofs/tla/CommitFence.tla), [`crates/db/src/txn/fence.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/txn/fence.rs)).

## Auto-commit vs explicit

Every rule in this book applies to both; the differences are exactly three: explicit
transactions own their retry policy (conflicts surface to the caller), span statements
(savepoints, read-your-own-writes across requests), and live on one worker for their
whole life (the actor). Auto-commit writes are the ones the engine re-runs
([lifecycle](lifecycle.md)).

> **Why an actor and not a lock.** "One transaction, one statement at a time" is an
> ordering requirement, and a mailbox gives ordering without a lock object: requests
> queue in arrival order on the owning worker, the worker is never blocked (other
> documents' work interleaves freely), the idle sweep is just another message, and
> there is no mutex to hold across an await, leak on panic, or convoy under load.
