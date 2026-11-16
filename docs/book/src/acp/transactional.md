# ACP inside the transaction

The rule that makes local ACP trustworthy: **every access decision runs in the
transaction it authorizes, on the same snapshot as the data it gates**. Not a pre-check,
not a cache consultation before the transaction: inside it, validated exactly like a
data decision. An external provider (Vera) cannot join a local transaction; its shape
is the exception below.

![ACP: registration in the create transaction; a racing registration conflicts](../diagrams/14-acp.png)

## The create path: no public window

A create writes the document (blocks, value, index entries, journal append) and then,
*in the same transaction*, scans the document's owner prefix with a **validated scan**
and puts the owner tuple. The commit that makes the document visible also makes its
owner tuple visible: there is no commit boundary between them, hence no window where the
document exists unowned and public.

A concurrent upsert/create of the same document by another identity loses that race: its
validated scan of the owner prefix meets the first writer's tuple put
(`mine ScannedRange, theirs Put`), its re-run finds the existing owner, and business
logic answers `AlreadyRegistered` (or a no-op when the owner is identical).

## The read path: one snapshot

A read-only query decides DAC at its own snapshot: the tuples and the document rows come
from the same consistent view. There is no "authorization checked, then data read"
pair of snapshots to disagree.

## The write path: validated tuples

A write's authorization is a decision on tuples, so it uses exactly the read rules for
decisions:

- point reads of the tuples that grant the identity's permission are validated reads;
- decisions over a set (the owner prefix, a claim queue) are validated scans.

A revocation, grant or new registration that commits first aborts the decision, the
attempt re-runs, and the re-run decides again on the fresh tuples. The revocation race
(the [race gallery](../txn/conflict-logic.md) covers it step by step) lands as: the write
never commits after the revocation it raced.

## External providers (Vera) under the same shape

An external decider cannot join a local transaction. The database keeps the shape anyway:
the create writes an *owed registration* record (a pending marker under the ACP
namespace) in its own transaction; until the provider confirms, the document's reads are
gated to its creator alone; a paged redrive settles owed registrations; and during
engine retries, an external decision is asked once, memoized by request bytes, and
replayed identically for later attempts. Failures of the provider fail closed
(`Unavailable` denies).

### Owed relationship changes

The same bridge carries a relationship change made inside a transaction
(`acp::outbox`). The in-transaction relationship call (`relationship_in_txn`, behind
`Node::add_dac_actor_relationship_change_for_in_txn`, its delete counterpart and
`Node::change_dac_actor_relationship_as`) does not refuse on an external provider: it
*stages* the change, a record written in the transaction that commits or rolls back with
its rows, and the provider is sent the change after the commit.

Staging is for embedders that call the node directly. The HTTP relationship route with
`x-defradb-tx` serves only a provider that joins the transaction; on an external one it
answers 501, `transaction-bound document relationships are unavailable`, and records
nothing, until a caller can abandon a staged change and a drainer settles them.

- **What staging returns.** `RelationshipChange::Staged { seq, authority_checked }`: the
  change's place in the node's queue and whether its acting DID's authority was checked.
  A provider that joins the transaction (local ACP) still answers
  `RelationshipChange::Applied { existed }`, and nothing a local caller sees has changed.
  The methods that answer a boolean keep their meaning for an applied change and report a
  staged add as new (`false`, not "existed already") and a staged delete as found
  (`true`): whether the relationship exists on the provider is not known at staging.
- **When a staged change takes effect.** A staged *add* grants nothing until the provider
  holds it. A staged *delete* denies its subject through the staging transaction from the
  statement that staged it, and on this node from the commit, whatever the provider
  still holds (the gate below). Other nodes see either once the provider holds it.
- **The authority recorded.** The change is submitted after the request that made it is
  gone, so the record says how: `node_key` when the node holds a key for the acting DID
  (the one it signs that DID's delegations with), otherwise the delegation the request
  carries for it (`delegation{token}`), kept as presented. With neither, or with a token
  that is no delegation to this node, staging fails with `OwedAuthorityUnavailable` and
  records nothing. The token is stored in the clear in the node's store, in a record no
  API returns.
- **The check at staging.** Staging refuses what a local ACP would refuse in the common
  case: the acting DID must own the document or hold a relation that manages the changed
  one. While the document's registration is still owed (it was created in this
  transaction, or by one whose registration has not settled), its recorded creator is the
  owner and the only authority, and the provider is not asked. Otherwise the owner and
  the managing relations are read from the provider as its last round trip saw them, and
  an acting DID that an unsettled delete names is refused. The provider still decides at
  submission.
- **When the check cannot run.** Only a provider that could not be reached counts
  (`ProviderUnavailable`); any other error of the provider, such as an owner it names
  that is no DID, fails the staging call. With the provider unreachable:
  - an **add** is staged unchecked. `authority_checked` is `false`, in the answer and in
    the record, and a warning is logged. It grants nothing until the provider takes it.
    It never holds its tuple: if the provider refuses it the record is deleted, and if
    it is left waiting for authority, or the provider answered it with an error, it is
    deleted as soon as a later change of its tuple comes up for submission. An actor
    nobody checked cannot keep the owner's change from the provider;
  - a **delete** is not staged: the call fails with `OwedCheckUnavailable`, to be made
    again once the provider answers. A delete denies from its commit, and is never taken
    on the word of an actor nobody could check. The creator of a document whose
    registration is still owed is checked without the provider, so it can revoke on that
    document during an outage.
- **Subjects the provider cannot be sent.** A change whose subject is of a kind the
  provider cannot take (a userset or cross-object subject on the Cosmos provider, a typed
  wildcard on either) fails with `UnsupportedSubject` and records nothing. It could never
  be submitted, and a delete of it would deny for ever.
- **One spelling.** The acting DID and an actor subject are stored in the provider's one
  spelling of the key (for vera.rs, the uncompressed `did:key` DefraDB derives), and the
  gate compares a requester in the same spelling, so a revocation staged under one
  spelling of a key denies a request under another. A key the node holds is found under
  whichever spelling it was registered with, at staging and at submission alike, so a
  change recorded as `node_key` is signed with it whatever spelling the record keeps.
- **Settlement.** A pass submits what is owed in sequence order from the head of the
  queue, read again each pass. A change is submitted only when it is the oldest unsettled
  change of its tuple, and only after the registration its document still owes. What the
  provider answers decides it:
  - *applied*: the record is deleted;
  - *refused* (the acting DID lacks the authority, or the provider rejected the
    request): a change whose check ran is parked as `refused{reason}`; one staged
    unchecked is deleted;
  - a kind of change the provider cannot take at all: the record is deleted, checked or
    not;
  - no usable authority (the delegation expired, the key is no longer held): it is
    parked as `awaiting_authority`, checked or not;
  - an error of the provider that does not say whether it is final: the change stays
    staged, with the last error and how many such answers it has had, and the pass goes
    on. Five such answers (`MAX_FAILED_SUBMISSIONS`), whatever their words, are taken as
    a refusal. Only the provider's answers count: an unreachable provider is not one;
  - an error of the node's own before anything was sent (a request it could not build
    or sign): the change stays staged with the error as its last error, nothing is
    counted, and the pass goes on. Every pass that comes to it logs a warning naming the
    change;
  - *unreachable* (a transport failure, a timeout): the pass stops and nothing is
    written.

  A parked change whose acting DID was checked holds every later change of its tuple and
  nothing else: changes of other tuples go on past it. A checked change the node could
  not send holds its tuple the same way, for as long as the node cannot send it: it is
  not counted towards a refusal, so nothing parks it, and a direct change of the tuple
  fails with `OwedChangesPending` meanwhile. A change staged unchecked holds nothing
  (above): one the node could not send is deleted, like one waiting for authority, when
  a later change of its tuple comes up. Every record deleted without the provider holding the change is
  counted in the pass's report (`OwedSettlement::discarded`) and logged with its
  sequence, actor, document and relation.
- **When a pass runs.**
  - After the commit of a request that staged a change, over the documents that
    transaction staged on (its own changes and what was owed on them before). The commit
    waits for it, for at most 10 seconds (`OWED_SETTLE_DEADLINE`): that is the most a
    staged change adds to its commit, and past it the commit returns with the rest still
    owed. With the provider answering, the wait is one round trip per change.
  - Once at start, after the owed registrations, over everything.
  - Before a relationship change made outside a transaction on a tuple that has changes
    owed, over that document.

  A commit that stops waiting leaves the submission it was waiting for in flight. That
  submission cannot take effect after a later change of the same tuple: the node sends
  the later change only after the earlier one, and both providers take one signer's
  requests in order. vera.rs keeps one journaled request per worker and prepares the
  next only once that one has a verified receipt, so while the earlier submission is in
  flight nothing else is sent; Cosmos orders one signer's transactions by account
  sequence. A provider supplied by a host has to give the same guarantee
  (`DocumentACP::submit_owed_change`). Two nodes that change one relationship through
  different workers are ordered by the provider alone, as they are for changes made
  outside a transaction.

  There is no background drainer, timer or back-off yet. **A change left staged (the
  provider was down, or the deadline passed) is submitted by the next commit that stages
  on the same document, by a direct change of its tuple, or at the next start**; until
  then an owed add stays ungranted and an owed delete keeps denying.
- **A change made outside a transaction.** It goes to the provider at once, as before.
  When changes are owed on its tuple a pass over the document runs first. If a *staged*
  change is still owed on the tuple afterwards (the provider was not reached, or the
  document's registration is still owed), the direct change fails with
  `OwedChangesPending` and is to be made again: sent now, it would reach the provider
  before the staged change it follows, and a revocation would be undone when the staged
  grant lands. If only a *parked* change remains on the tuple, the direct change is sent
  and overtakes it: nothing submits a parked change again by itself.
- **A change submitted twice.** A record is deleted after the provider applied its
  change, in a transaction of its own; nothing marks a change as in flight. A crash, a
  lost answer or a lost commit between the two leaves the change staged, and the next
  pass submits it again. vera.rs sets a relationship it already holds and deletes one it
  does not hold without error (a successful receipt saying the record existed, or was not
  found), so the second submission is read as applied and the record is deleted. Two
  windows remain open. If the acting DID lost its authority in between, the second
  submission is rejected and the change is parked as refused although the provider holds
  it; a rejected receipt carries no reason that tells the two apart. And if someone else
  changed the same relationship on the provider in between, the second submission applies
  the change again over theirs.
- **Limits of this step.**
  - A parked change stays parked: nothing here retries, abandons or reauthorizes one, so
    the changes behind it on its tuple wait, and a parked delete keeps denying.
  - The denial of an owed delete is scoped to its document. A staged delete of a
    membership tuple (a member removed from a group object) denies that subject on the
    group object only. Documents that grant access through a userset naming that group
    keep honouring the member until the provider holds the delete.
  - No event is published when a staged add settles.
  - A document's owner is taken to stay the same on the provider while the node still
    holds the document's owed registration. If the creator gives the document away
    through another client after the provider registered it and before this node
    cleared its record, this node goes on letting the creator read it until the record
    is cleared (`AssumeNoTransferWhileOwed` in `proofs/tla/AcpOutbox.tla`).
  - A create that requires transaction-bound authority
    (`with_transactional_authority_required`, used by verified creates) is refused on an
    external provider as before.

- **The record.** A change is written in the transaction that stages it, under the ACP
  namespace, and commits or rolls back with its rows: `/acp-owed/q/<seq>` holds the
  change (tuple, add or delete, acting DID, the authority to submit it as that DID,
  whether that DID was checked at staging, its state, attempts and last error; at most
  16 KiB, and staged only if it would still fit parked as refused or awaiting authority
  with 128 bytes of the provider's words in each place they are kept, so a change that
  was staged can always be parked; longer words are cut), `/acp-owed/t/<tuple>` what is
  unsettled on that tuple (its oldest and latest sequences, how many changes and how many
  of them deletes), and `/acp-owed/o/<object>` the number of unsettled changes on that
  object. A change is unsettled while it has a record, a refused one included; a settled
  change has none.
- **The order.** A sequence comes from `Db::allocate` after the staging transaction read
  the tuple's key, which it also writes. Two transactions that stage the same tuple
  conflict, and the one that commits later holds the higher sequence. Across tuples a
  sequence follows the order of staging, not of commit, so a lower sequence can appear
  in the queue after a higher one: a reader of the queue starts from its head each time
  instead of keeping a high-water mark.
- **The bounds.** One transaction stages at most 256 changes, one acting DID holds at
  most 1,000 unsettled (`/acp-owed/a/<actor>` counts them), and a node holds at most
  10,000; each is a typed error before the commit. A parked change stays unsettled, so
  the acting DID's bound is what keeps one actor's parked changes from taking the whole
  of the node's. The bounds are constants with no configuration key yet. The actor's and
  the node's are checked against the counts the staging transaction sees, so
  transactions staging at once can pass either by what they stage together.
- **The gate.** While any delete of a tuple is unsettled, the adapter denies that
  tuple's subject on that object before it asks the provider: on document access, on
  access decisions, and on the authority to add or delete relationships. An add staged
  after the delete does not lift the denial: the provider may refuse that add and apply
  the delete, and until it has seen the delete it still grants what the delete takes. The
  denial covers every permission on the object, so a subject that also holds access by
  another path waits for the settlement. A delete whose subject is wider than one actor
  (the wildcard, a set of another object's holders) denies everyone. The owner, as the
  provider names it, is spared in both cases. The denial starts at the commit, on this
  node, and other nodes see the revocation once the provider holds it. Inside the
  transaction that staged the delete it starts at once: every read and access check made
  through that transaction applies the same rule to the transaction's own view, so the
  revoked subject is denied before the commit and a statement that is rolled back
  revokes nothing. An owed add grants nothing, in its own transaction either. With
  nothing owed on an object the gate costs one point read.

## The query engine's side

The planner wraps collection plans in permission filter nodes; `_commits` rows,
mutation target sets and arrival entries are filtered through the same checker; the
block read gate checks ACP before serving replicated blocks. Unregistered documents are
public only when the node's configuration says so.

> **Why validated, not cached.** A cached authorization is an answer about the past
> being reused for the present: between cache fill and use, a revocation may have
> landed, and the write would commit on an authorization that no longer exists.
> Validated tuples make the authorization itself part of the commit's consistency
> guarantee: if anything it rested on changed, the commit loses, exactly like a data
> race. That symmetry is the entire trick, and the TLA+ models for ACP soundness and
> the deferred-ACP overlay pin it.
