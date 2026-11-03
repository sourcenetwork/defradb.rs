# Vera: external policy decisions

Vera moves authorization off-node: an external chain (CometBFT or a native
commonware-based chain) holds the policies and tuples, evaluates checks, and proofs
flow back to the database.

## What it is

`crates/vera` defines the `VeraProvider` interface: create policies, register and
archive objects, set and delete relationships, check access. Two implementations:

- **CosmosProvider**: a Cosmos-SDK chain over LCD/gRPC, transactions broadcast to it,
  and a WebSocket subscriber for cache invalidation.
- **VeraRsProvider**: the native vera.rs chain (commonware consensus), commands signed
  and submitted through a worker, and the chain state verified by a light client
  (trusted consensus key, height tracking).

Both are wrapped as a `DocumentACP` and installed as the node's document ACP. Because
the provider cannot join local transactions, the node wraps it in the external-ACP
adapter: owed registrations, gated reads until confirmation, fail-closed
`Unavailable` (the previous chapter's shape).

## Why it exists

Some deployments need authorization decisions that no single database node can make or
forge: multi-party deployments where the policy is shared infrastructure, audits that
require the decision trail to be external to the data holder, or thresholds the chain
provides. Vera is opt-in: without it, local Zanzibar ACP is the whole story.

## The performance shape

External decisions are remote inputs, so they follow the doctrine's remote-input rules:
resolved at the edge, never inside a retried closure, memoized by request bytes across
attempts, single-flight, batched, cached (access-decision and policy caches with the
circuit breaker and tuning knobs in `crates/vera/src/`), with staleness documented and
a persisted-decision path for signers to verify via the light client.

> **Why fail closed.** An external decider that times out is not "probably fine": it is
> a decision the system cannot make. Granting on timeout converts an outage into an
> escalation; denying converts it into a latency event. The interface says so in its
> error type: `Unavailable` means all decisions fail closed.
