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

A relationship change made inside a transaction is staged the same way: recorded in the
transaction as owed to the provider and submitted after the commit
([Owed relationship changes](transactional.md#owed-relationship-changes)). With vera.rs:

- **Who signs.** The worker signs the submission; the command inside it carries the
  acting DID's authority, whatever its subject: an actor, everyone, or a userset or
  cross-object subject. The same holds for a relationship changed outside a
  transaction: it is sent under its requestor's token, never under the node's own actor. The node signs a delegation itself when it holds that DID's key
  (its own actor, or a registered identity with a local secp256k1, Ed25519 or P-256 key);
  otherwise it stores the delegation the request presented and submits with it. A
  delegation that expired before the change could be submitted parks the change as
  awaiting authority.
- **What the grantee and the revoked subject see.** A grantee reads once the chain holds
  the grant, normally when the committing request returns; with the chain unreachable,
  not before a later commit on the same document, a direct change of the tuple or a
  restart submits it. A revoked subject is denied by this node from the commit, and by
  every node once the chain holds the revocation.
- **The check at staging.** The document's owner comes from the light client's verified
  state, and a managing relation is evaluated there as a permission on the document. The
  chain counts as unreachable when the read fails with a connection error, or when the
  light client has no fresh verified state left (30 seconds after the chain stops
  answering); then an
  add is staged unchecked and a delete is refused. A read that fails any other way fails
  the staging call.
- **Outcomes.** A verified receipt that succeeded settles the change; one that was
  rejected is the chain's refusal (the change is parked, or deleted if it was staged
  unchecked); a submission whose receipt could not be confirmed leaves it staged. A
  repeated `SetRelationship` or `DeleteRelationship` succeeds without effect, so a change
  submitted again after a crash settles, unless the acting DID lost its authority in
  between (then it is parked as refused) or another manager changed the relationship in
  between (then it is applied again).
- **The Cosmos provider.** A broadcast that timed out or failed in transport leaves the
  change staged. Any other failed transaction is an answer the provider does not
  classify: the change stays staged, the pass goes on, and five such answers in a row
  with the same words park it as refused.

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
