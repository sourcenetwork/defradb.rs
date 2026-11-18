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
  awaiting authority. The registration a create owes records its authority the same
  way, and a retry of it reads nothing else.
- **After a failed submission.** The node reports what an actor still owes, waits for
  one transaction's entries, takes a fresh delegation for one owed registration or
  change, and lets the actor that staged a refused change abandon it
  ([What is owed after the commit](transactional.md#what-is-owed-after-the-commit)). A
  fresh delegation is verified against the owed command as the first one was.
- **What the grantee and the revoked subject see.** A grantee reads once the chain holds
  the grant, normally when the committing request returns; with the chain unreachable,
  not before a later commit on the same document, a direct change of the tuple or a
  restart submits it. A revoked subject is denied by this node from the commit, and by
  every node once the chain holds the revocation.
- **The check at staging.** The document's owner comes from the light client's verified
  state, and a managing relation is evaluated there as a permission on the document. The
  chain counts as not having answered in three cases: the read's error holds an I/O
  error (a refused or reset connection); it holds a failure of the transport itself
  that is not a JSON-RPC answer (a response over the size limit, a response with no
  result, any failure of the in-process transport); or the light client has no fresh
  verified state left. A plain request timeout and a response body that is not JSON are
  none of these as the HTTP transport reports them, so they fail the staging call until
  the verified state has gone stale; the light client's default freshness window is 30 seconds, and it is the
  light client's setting, not a constant of this code. When the chain did not answer, an
  add is staged unchecked and a delete is refused. A read that fails any other way (a
  JSON-RPC error, a proof that does not verify) fails the staging call.
- **Outcomes.** A verified receipt that succeeded settles the change; one that was
  rejected is the chain's refusal (the change is parked, or deleted if it was staged
  unchecked); a submission whose receipt could not be confirmed leaves it staged. A
  repeated `SetRelationship` or `DeleteRelationship` succeeds without effect, so a change
  submitted again after a crash settles, unless the acting DID lost its authority in
  between (then it is parked as refused) or another manager changed the relationship in
  between (then it is applied again).
- **The Cosmos provider.** A broadcast that timed out or failed in transport leaves the
  change staged. A transaction the node could not key or sign is the node's own error
  and counts for nothing. Any other failed transaction is an answer the provider does
  not classify: the change stays staged, the pass goes on, and five such answers park it
  as refused. It takes no userset or cross-object subject, so such a change is refused
  at staging.

## Acting for a signer: delegations and the one-request form

vera.rs takes a policy command from a node's worker only under the authority of the
actor the command is made as. A node has that authority in three ways.

- **A key it holds.** Its own actor, or a registered identity with a local key: the node
  signs a five-minute delegation itself.
- **The actor's delegation.** A token the actor signed for this node's worker and
  deployment with the scope `acp:policy`
  (`vera::vera_rs::caller_delegation`). It authenticates an HTTP caller, and it covers
  every registration and relationship command the node sends for that actor until it
  expires.
- **A delegation bound to one request.** The same token with vera.rs's `request` claim
  (`vera::vera_rs::request_delegation`): the digest of one command, the deployment's
  genesis identity and an operation identity. It covers that command and nothing else.

What vera.rs checks for the one-request form, and the node checks first:

- the signature, the worker (`sub`), the deployment (`aud`), the scope and the validity
  interval, as for any delegation;
- the **digest**: SHA-256 of `vera/acp-operation/v1\0` and the compact JSON
  `{"PolicyCommand":[policy ID, command]}`, with the policy ID as 64 lowercase
  hexadecimal digits. One token names one command on one policy. There is no digest over
  a list of commands, so a create that also grants needs a token for the registration and
  one for each grant;
- the **operation identity**: eight bytes of expiry and 24 bytes the signer chooses. It
  must not have expired and may expire at most **600 seconds** after the chain's clock
  at execution, and the token may not outlive it. A request-bound delegation is therefore
  usable for at most ten minutes, and `request_delegation` issues none that lasts longer;
- the **genesis identity** of the deployment: the ID of its genesis block, which its
  first block names as its parent. The node checks all of this before it sends, and
  refuses a token bound to another command, or naming another genesis identity, without
  sending it. A provider that does not know the genesis identity leaves that one check to
  the chain.

vera.rs keeps the outcome of an executed request under the actor and the operation
identity until that identity expires. The same token sent again is answered with the
recorded outcome and changes nothing; a different command under a used identity is
rejected. A registration or a grant submitted twice across a crash therefore takes effect
once, even when the relationship was changed in between.

A signer can make the token before the write runs. `vera::vera_rs::request_digest`
gives the digest from the same bytes the node submits, so an actor inside the command is
in the spelling the node sends. A document ID comes from the document's content: the
client that signs a create's blocks (`ClientCreateSignatures::from_input`) has it before
the node sees the write. The policy ID and the resource are the collection's. The worker,
the deployment and the genesis identity are the node's delegation audience, in its light
client status (`delegation`: `worker`, `deployment` and `genesis_id`, `0x` and 64
hexadecimal digits), from `RemotePolicies::light_client_status` in process or
`GET /api/v0/acp/status`. `RequestBinding::for_command` builds the binding from that
audience, a command and a nonce, and `request_delegation` signs it.

The vera.rs provider learns the genesis identity from its light client: it reads the
deployment's first block and checks its finality against the trusted consensus key, the
trust every other read rests on, and takes that block's parent. An embedded deployment
is read the same way through its in-process transport, so a client of a node that serves
no JSON-RPC learns it from the node all the same. Until the first block is proven (the
node does not serve it, or did not answer yet) the audience has no `genesis_id`: the
provider asks again at each new height, a request-bound delegation cannot be built from
the audience, and one built otherwise is checked without it, by the chain only. A
provider that is not vera.rs reports none.

Where a one-request delegation is taken, and where not:

- **Taken** beside a verified write
  ([Verified writes on an external provider](transactional.md#verified-writes-on-an-external-provider)),
  as `SignerCredential::RequestBound` for the registration a create owes, in the
  context of a relationship call (`RequestContext::with_request_delegation`) for that one
  change, and as the fresh credential of one owed registration or change
  (`Node::reauthorize_owed_acp_as`), whose command it must name.
- **Not taken** as a caller's credential. `caller_of_delegation` refuses it, so an HTTP
  request that presents one is not authenticated. Presented as a plain bearer it admits
  no verified write and stages no change.
- **Checked wherever it is sent.** A command that goes to the chain with one, at once or
  after a commit, is compared with the command the token names first, and is not sent
  when they differ. A plain delegation is checked again the same way as its command is
  sent: one that expired since the write sends nothing.

A command made as the write's signer is signed by the node's worker; the token is the
signer's authority for it, and the chain records the signer, not the node, as owner.

## Replicated documents

A replicated change merges on a node only if its verified signer holds the permission
on that node's provider when it merges: `delete` for a deletion, `update` otherwise.
The document's creator gets no exception. A refused change is parked rather than
rejected for good (see "Parked refusals" below); only an unsigned change's refusal is
final. Under a remote provider every frame, the genesis included,
is put back to be tried again (a retryable skip) until the provider holds the document
and the node can read it. The node counts as able to read when its identity or its
provider's own actor (`DocumentACP::own_actor`) can; both are the same node.

**A creator on another deployment.** Such a creator cannot write on this node's
provider, so nobody would register the document here and it would never merge. An
embedder can list collections whose replicated documents the receiving node registers
itself:

```rust
let replicas = defra_node::ReplicaRegistration::new(["Note"], Arc::new(MyOwnership));
let node = Node::builder().with_replica_registration(replicas) /* ... */;
```

`ReplicaOwnership::is_own_actor` says whether a DID is one of this deployment's own
actors, and `creator_grants` names the relationships to grant on a document the node
registers. They must give the creator `update` and `delete` under the collection's
policy (for example `writer` for the creator), and read for the deployment's other
nodes if they are to merge the document too. The setting is off by default, and has no
effect on an unlisted collection or under a local provider.

For a listed collection, a genesis whose verified signer is not one of the
deployment's own actors, of a document the provider does not hold, merges and records
in the same transaction:

- an owed registration of the document as the provider's own actor, sent with the
  node's key;
- the embedder's grants, staged as owed relationship changes made by that actor;
- a marker that ties them together as one unit.

They are submitted after the commit, registration first, and again whenever a change to
the document is put back while something is still owed. A genesis signed by one of the
deployment's own actors is never registered by a receiver: it waits for its creator's
own registration, as before.

While the unit is owed:

- a change by anyone but the registering actor is put back while the registration
  itself is owed, because the provider has not seen it;
- a change by the creator is also put back while a grant of the unit is still owed.

A grant the provider refuses is not "owed": from then on the provider decides the
creator.

**Two receivers of one deployment.** The provider takes the first registration. The
second receiver is told the document is already registered. Its grants, which only the
owner can give, are discarded with a note that the actor's report shows. It then waits
for the owner's grants: the creator's changes are put back there until the provider
lets the creator update, and the marker is then dropped. A receiver that first sees the
genesis after another registered it, while the creator cannot write yet, waits the same
way. The creator is never refused on either node while the owner's unit is owed. If the
owner never grants it, the creator's changes keep being put back on the other node and
nothing is quarantined.

**Deletes.** A remote provider keeps the object when its document is deleted: a merged
delete never unregisters, and a local delete leaves a registration that is still owed
in place, so it lands. Otherwise the delete and the changes before it would wait for
ever on every node that waits for the provider to hold the document. A document whose
creator lost its last grant stays registered and refuses that creator.

**Parked refusals.** Under a remote provider, a signed change whose signer the
provider refuses is parked by its document, because a later grant can change the
answer. A change whose history holds a parked one is parked with it: it cannot merge
before the change it builds on. A change on another branch is not held up.

A parked change is tried again through the normal merge path, with a fresh decision:

- when this node changes the document's relationships on the provider (a relationship
  set or deleted through the node, or an owed change or registration it settles);
- when the refused change itself merges, for what waits behind it;
- on the sweep, every 60 seconds and for at most 256 documents a pass. The sweep covers
  a grant made by another node of the deployment, which this node is not told about.

The index holds at most 1,024 parked changes per document and 16,384 per node. It is
in memory. A change past either bound, or parked before a restart, is not indexed: it
stays unmerged and the replication layer tries it again on its own retry clock, as
every retryable skip is. A revoked writer's later change is refused and parked. It
merges only if the writer is granted again.

**Limits.** Every merged change asks the provider for the document's registration, the
node's read and the signer's permission. These decisions are cached against the light
client's verified state, so a new chain revision misses the cache. A change parked on
a grant that never comes is tried again by every sweep until a restart drops it from
the index.

The models are `proofs/tla/ReplicaRegistration.tla` and `proofs/tla/ParkedMerge.tla`;
the gate's classification, parked refusals included, is proved in
`proofs/lean/ReplicaGate.lean`, and the bounded index parked changes wait in in
`proofs/lean/DeferredMerges.lean`.

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
