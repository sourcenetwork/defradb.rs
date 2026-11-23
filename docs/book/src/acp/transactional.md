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

The node's HTTP relationship routes (`POST` and `DELETE /api/v0/acp/document/relationship`,
`POST /api/v0/acp/document/relationships`) with `x-defradb-tx` stage the same way, in the
client's transaction. The acting DID is the one the request's verified credential names,
never a field of the request. The authority recorded is the node's own key when the
provider holds the key for that DID, whatever the caller signed in with, as the route
without a transaction sends as a DID whose key the node holds; otherwise it is the
delegation the credential is. So a caller signed in with an identity token, which is no
delegation, has its change staged under the node's key when the node holds that DID's
key, and is otherwise refused with 401 (`OwedAuthorityUnavailable`) with nothing staged.
The check below runs at staging in both cases,
so a caller without authority is refused before its commit (401 for a document it does
not own, 503 for a delete while the provider cannot be asked) and owes nothing. The answer
keeps Go's field (`ExistedAlready`, `RecordFound`, with a staged change reported as new
or found) and adds `Staged: {Seq, AuthorityChecked}` for a staged change; an applied
change answers as before. A transaction the client discards, or the node's idle sweep
ends, takes its staged changes with it. HTTP carries no delegation bound to one request:
such a token signs nobody in, so over HTTP a change is staged under the caller's plain
delegation; an embedder stages under a request-bound one in process
(`RequestContext::with_request_delegation`). The defradb server's own HTTP adapter still
refuses relationship changes in a transaction. The calls that report what is owed, wait
for it, take a fresh credential and abandon a refused change
([What is owed after the commit](#what-is-owed-after-the-commit)) are node calls,
with no HTTP route.

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

  Through a provider whose worker journals its requests (vera.rs) a change is sent with
  an *in-flight mark*. Before its request is journaled the record stores the request's
  digest, and before the request is sent its transaction hash too (`in_flight`). A
  change the node finds marked, after a crash or a commit that stopped waiting, is
  decided before anything else is done with it: the request still in the worker's
  journal is sent and waited for; one out of the journal is looked up by its hash, as
  a verified receipt, and failing that in the worker's note. A record that stores the
  hash names its request by the hash alone, in the journal and in the note; one that
  stores only the digest names it by the digest, among the requests signed from the
  worker sequence stored with the mark on, so an earlier request with the same bytes
  (a delegation used again for the same command) does not decide it. Whoever resolves
  a request it did not journal itself (the provider at start, another command before
  its own, whether its bytes are the same or not) writes the outcome to that note,
  `owed-outcomes.json` beside the worker's journal, which keeps the latest 256.
  Applied, the record is deleted; rejected, the change applied nothing and goes again
  on its own; known to none of them, it is sent again, by the next pass, which the
  drainer runs after its back-off without waiting for a commit. So a change takes
  effect at most once **as long as its
  journal entry, its receipt or its note can still be read**: a receipt the provider
  no longer serves and a note pushed out are the one way left to send a change twice.
  The changes one transaction staged one after the other go in one request, at most 16
  (`MAX_CHANGES_PER_REQUEST`) and 256 KiB, which vera.rs applies together or not at
  all (`batchCalls`); each keeps its own credential. A request refused that way applied
  nothing, and its changes go again one at a time (`alone`), so a change is only ever
  parked for a refusal of its own. Every refusal is published on the event bus as
  `acp-owed-change-refused`, with the change, the provider's words and whether it stays
  parked. A change the provider cannot take at all is removed and counted in
  `discarded`, and is not published: the provider was asked nothing. A provider
  without a journal (Cosmos, a host's own) is sent each change on its own with no
  mark, as below. A change a node finds marked once it runs on such a provider has its
  mark cleared and is sent again: no journal is left to ask about its request, and it
  can take effect twice.

  A parked change whose acting DID was checked holds every later change of its tuple and
  nothing else: changes of other tuples go on past it. A checked change the node could
  not send holds its tuple the same way, for as long as the node cannot send it: it is
  not counted towards a refusal, so nothing parks it, and a direct change of the tuple
  fails with `OwedChangesPending` meanwhile. A change staged unchecked holds nothing
  (above): one the node could not send is deleted, like one waiting for authority, when
  a later change of its tuple comes up. Every record deleted without the provider
  holding the change is counted in the pass's report (`OwedSettlement::discarded`),
  logged with its sequence, actor, document and relation, and leaves a note for its
  actor ([What is owed after the commit](#what-is-owed-after-the-commit)).
- **When a pass runs.**
  - After the commit of a request that staged a change, over the documents that
    transaction staged on (its own changes and what was owed on them before). The commit
    waits for it, for at most 10 seconds (`OWED_SETTLE_DEADLINE`): that is the most a
    staged change adds to its commit, and past it the commit returns with the rest still
    owed. With the provider answering, the wait is one round trip per change.
  - By the node's drainer, over everything: once at start after the owed
    registrations, after every commit that staged a change, and, while the provider is
    away, answers with errors that may pass, or a pass leaves a change the node may send
    again at once (the node could not send it, or nobody knows what its request came
    to; `OwedSettlement::retry`), again after a back-off that doubles from 1 second up
    to 60. A parked change is no reason for another pass: only its actor moves it. One
    drainer runs per node, and every pass takes the same lock, so passes are serial.
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

  A change left staged (the provider was down, or the deadline passed) is submitted by
  the drainer without another request, on an idle node too; until then an owed add stays
  ungranted and an owed delete keeps denying.
- **A change made outside a transaction.** It goes to the provider at once, as before.
  When changes are owed on its tuple a pass over the document runs first. If a *staged*
  change is still owed on the tuple afterwards (the provider was not reached, or the
  document's registration is still owed), the direct change fails with
  `OwedChangesPending` and is to be made again: sent now, it would reach the provider
  before the staged change it follows, and a revocation would be undone when the staged
  grant lands. If only a *parked* change remains on the tuple, the direct change is sent
  and overtakes it: nothing submits a parked change again by itself.
- **A change submitted twice.** Through a provider without a journal a record is
  deleted after the provider applied its change, in a transaction of its own, and
  nothing marks a change as in flight. A crash, a lost answer or a lost commit between
  the two leaves the change staged, and the next pass submits it again. Through
  vera.rs the in-flight mark above decides such a change, and it is submitted again
  only when its journal entry, receipt and note can no longer be read. vera.rs sets a relationship it already holds and deletes one it
  does not hold without error (a successful receipt saying the record existed, or was not
  found), so the second submission is read as applied and the record is deleted. Two
  windows remain open. If the acting DID lost its authority in between, the second
  submission is rejected and the change is parked as refused although the provider holds
  it; a rejected receipt carries no reason that tells the two apart. And if someone else
  changed the same relationship on the provider in between, the second submission applies
  the change again over theirs.
- **Limits of this step.**
  - Nothing submits a parked change by itself. Its actor hands in a fresh credential for
    it, or abandons it when the provider refused it
    ([What is owed after the commit](#what-is-owed-after-the-commit)); until then the
    changes behind it on its tuple wait, and a parked delete keeps denying.
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

- **The record.** A change is written in the transaction that stages it, under the ACP
  namespace, and commits or rolls back with its rows: `/acp-owed2/q/<seq>` holds the
  change (tuple, add or delete, acting DID, the authority to submit it as that DID,
  whether that DID was checked at staging, its state, attempts and last error; at most
  16 KiB, and staged only if it would still fit parked as refused or awaiting authority
  with 128 bytes of the provider's words in each place they are kept, so a change that
  was staged can always be parked; longer words are cut), and `/acp-owed2/l/<tuple><seq>`
  marks it live on its tuple with whether it is a delete, so the tuple's oldest and
  latest unsettled sequences are the first and last of those keys, and how many are owed
  on it, and how many of them are deletes, are theirs. A change is unsettled while it
  has a record, a refused one included; a settled change has none.
- **The counts.** What the deny overlay and the node's bound decide on is counted: per
  object, the deletes whose subject is wider than one actor (`/acp-owed2/o/<object>`); per
  actor a delete names on an object, its deletes and those of the owner relation
  (`/acp-owed2/s/<object><actor>`); and for the node, its changes (`/acp-owed2/n`). Each count is one merged value with a staged part,
  which only staging adds to, and a settled part, which only settling adds to; what is
  owed is the difference. A count is never read and written back, put or deleted, so a
  settle aborts no transaction that read the staged part: not one that stages on the
  same document or the same tuple, and not one whose access decision read the deny
  overlay there (`proofs/tla/OwedParts.tla`, `proofs/lean/OwedParts.lean`). A count
  whose changes have all settled reads as nothing owed, exactly as an absent one, but it
  is not removed. **Each object a wider delete was ever owed on, and each actor a delete
  ever named on an object, keeps one small folded value (about 40 bytes and its key),
  and each tuple ever staged on keeps its head key (8 bytes and its key)**, until
  regolith can fold a chain of merges that has no value beneath it, which a compaction
  filter could then drop (sourcenetwork/regolith#314).
- **The layout before the counts were split.** It lived under `/acp-owed/`, and this
  version does not read it as owed state. A store whose queue there still holds a
  change (a `/acp-owed/q/` record; a settled change has none) is refused: the node does
  not start, with `LegacyOwedChanges` naming how many it owes, and a gate opened on such
  a store refuses every access decision, settling pass and redrive with the same error;
  the node's drainer stops at it instead of trying again.
  An owed revocation in it can therefore never stop denying unseen. Settle those changes
  with the version that staged them, or create the store again. What else that layout
  keeps once everything has settled (counts at zero, the sequence counter, notes of
  discarded changes) owes nothing, is not counted and does not hold a start back.
- **The order.** A sequence comes from `Db::allocate` after the staging transaction read
  the tuple's head key (`/acp-owed2/h/<tuple>`, the latest sequence staged on it), which it
  then writes. Only staging reads or writes that key: two transactions that stage the
  same tuple conflict, and the one that commits later holds the higher sequence; a
  settle of the tuple meanwhile does not conflict with either. Across tuples a
  sequence follows the order of staging, not of commit, so a lower sequence can appear
  in the queue after a higher one: a reader of the queue starts from its head each time
  instead of keeping a high-water mark.
- **The bounds.** One transaction stages at most 256 changes, one acting DID holds at
  most 1,000 unsettled (`/acp-owed2/a/<actor>` counts them), and a node holds at most
  10,000; each is a typed error before the commit. A parked change stays unsettled, so
  the acting DID's bound is what keeps one actor's parked changes from taking the whole
  of the node's. The bounds are constants with no configuration key yet. The actor's is
  checked against the count the staging transaction sees, so transactions staging at
  once can pass it by what they stage together. The node's is too while it is far away;
  within 1,024 of it (`VALIDATED_WITHIN_OF_NODE_LIMIT`) the staged part of the node's
  count is read validated, so transactions that stage there commit one after the other,
  and a settle aborts none of them. They can pass it only by adding more than that
  margin between them in transactions that each read the count below it.
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
  revokes nothing. An owed add grants nothing, in its own transaction either. A decision
  is two point reads, whatever the policy: the object's count of deletes wider than one
  actor and the count of deletes naming the identity there, each read on its staged
  part. An anonymous decision reads only the object's count: no delete can name it. A delete that commits after the decision read refuses it at commit; a settle
  never does, and one the read did not see only denies more.

### Verified writes on an external provider

A verified write (`Node::execute_verified_create_in_txn`,
`Node::execute_verified_mutation_in_txn`) is one create, update or delete an embedder has
checked against its signer's signature, run in a transaction the node owns. On a local
ACP its authority is *bound*: decided inside the transaction and validated at the
commit. An external provider cannot be bound that way. There its authority counts as
bound when what the write needs from the provider is **owed in the transaction, with
authority to send it as the signer**:

- a **create** owes the registration of its document, recorded with the row as an
  ordinary create records it;
- a relationship change made in the same transaction is staged as above, and is sent
  after the registration;
- an **update** or a **delete** owes nothing. It is admitted on the signer's
  permission, as the next part states.

The embedder hands the node what lets it act for the signer, as an argument beside the
write (`SignerCredential`), and the node carries it for that signer only. Nothing is read
from process-wide state:

| Credential | A create is staged when | An update or a delete is staged when |
|---|---|---|
| `None` | the node holds a key for the signer | the signer holds the permission, on a provider that proves its decisions (vera.rs); elsewhere the node must also hold the key |
| `Delegation(token)` | the provider takes `token` now as the signer's delegation to this node for its policy commands | the same, and the signer holds the permission |
| `RequestBound(token)` | `token` is the signer's delegation bound to exactly the command the write sends: the registration of the document a create makes | never |

Anything else is refused before the statement writes a row or an owed record, with the
error code `WRITE_AUTHORITY_UNAVAILABLE`, and the node discards the transaction: no
credential and no key, a delegation signed by somebody else, made for another node or
expired, a plain delegation presented as request-bound, a request-bound one presented as
plain, and a request-bound one that names another document, another command or, where
the provider knows the deployment's genesis identity (it is in the node's delegation
audience, [Vera](vera.md)), another genesis identity. An update
or a delete sends the provider no command, so a request-bound delegation has nothing to
name there and is refused, even where the node holds the key.

**Updates and deletes: admitted on the signer's permission.** The blocks of a verified
update or delete are signed by its signer, so who wrote it is proven; nothing is owed to
the provider, and nothing is recorded as owed for the write. The credential only says who
is asking. What admits the write is that the signer held the `update` (or `delete`)
permission on that document at a verified revision no older than the freshness limit
(below), decided at admission by the one check every update and
delete makes (`QueryRunner::authorize_mutation`), for every credential form alike: a
delegation proves who asks, and the permission still decides; a key the node holds
decides nothing either. The check runs before the statement writes anything, in this
order:

1. a registration of the document still owed on this node admits only its recorded
   creator, whatever the provider says (it does not know the document yet);
2. a revocation still owed on this node refuses the subject it names, on every
   permission;
3. otherwise the provider decides. On vera.rs the decision is a permission proof the
   light client verifies against the chain's finalized state, at a revision whose header
   is younger than the light client's freshness limit
   (`acp_light_client::FreshnessPolicy::default().max_age`, 30 seconds, with 15 seconds
   of allowed clock skew; set in `VeraRsProvider::connect`). It is never a local guess:
   the read path, the replicated-merge path and this one ask the same
   `VeraDocumentACP::check_doc_access`, and the node's access cache is off for vera.rs
   (`DocumentACP::proves_decisions`). A provider that cannot be reached, or whose newest
   verified revision is older than that limit, gives no decision and the write is
   refused: it fails closed.

A user without the permission, a reader who may read but not update, and a revoked
writer are refused, and nothing is written; the refusal is the one an ordinary update or
delete gets (the document is not found, or not authorized), or the provider's
unavailability, not `WRITE_AUTHORITY_UNAVAILABLE`. On the Cosmos provider, which proves no
decisions and keeps an access cache, `None` still needs a key the node holds.

**Freshness and revocations.** The guarantee is the permission at the proof's revision,
not at the moment of admission. A decision is read at the light client's newest verified
revision and kept, for the same request and for later ones, under that revision's state
root while that revision stays fresh: a new revision is a new key, and one past the
freshness limit is none. So a revocation is honoured from the first verified revision
that contains it, and at the latest once the revision before it has aged past the limit:
an update decided on evidence up to 30 seconds old can still pass a revocation finalized
inside that window. A revocation finalized after admission does not undo the write: the
write that was admitted commits. That is the rule, not a race.

A relationship change in the same transaction carries its own credential in the context
of its call: `RequestContext::with_bearer` for a plain delegation, or
`RequestContext::with_request_delegation` for one bound to that one change. A
request-bound delegation covers one command, so a create and a grant beside it need two.

A provider that gives the node nothing to check a delegation against (the Cosmos
provider, which is passed its caller's token as it is) admits a verified write with a key
the node holds and with nothing else: `Delegation(token)` is refused there.

What is recorded. The staged relationship change keeps its token in its record and is
submitted with it. The owed registration keeps its authority in its record too
(`PendingRegistration::authority`): `node_key`, the delegation the create carried, or
nothing. Either way the credential is checked again as the command is sent, with the
checks made at admission, so a delegation that expired between the write and the commit
sends nothing.

### What is owed after the commit

A commit on an external provider returns before the provider holds what the transaction
owes it, and stands whatever the submission comes to. Four node calls say what it came
to and act on it. Each takes the context of an authenticated caller.

| Call | What it does | Who may call |
|---|---|---|
| `Node::owed_acp_as(context, actor)` | lists what `actor` owes: the registrations of documents it created and the changes it staged | `actor`, and the node's principal (`Node::node_context`) |
| `Node::await_owed_acp_as(context, handle, deadline)` | waits until what one committed transaction owes is settled or has come to rest, and says which | anyone authenticated; the node's principal waits for every entry of the transaction, an actor for its own, and another actor's entries neither show in its wait nor keep it |
| `Node::reauthorize_owed_acp_as(context, entry, credential)` | records a fresh credential with one owed registration or change and submits it | the entry's actor, and the node's principal |
| `Node::abandon_owed_acp_change_as(context, seq)` | deletes the record of a change the provider refused | the actor that staged it, and nobody else; the node's principal for the changes it staged itself |

To a caller who may not see an entry it does not exist (`OwedEntryNotFound`). No call
returns a recorded credential.

- **The report.** An entry (`acp::OwedEntry`) names what is owed (a registration, or an
  add or delete of one relationship with its place in the queue), its actor, the
  transaction that recorded it, and where it stands:
  - `staged`: still to be sent. `reason` holds why its last submission failed, when one
    did: the provider could not be reached, or it answered with an error;
  - `awaiting_authority`: nothing recorded with it lets the node send it. `reason` says
    what is wrong with the credential;
  - `refused`: the provider said no, in the words kept as the reason.

  `held_by` says what an entry waits behind: its document's owed registration, or an
  earlier change of its relationship. That change's place in the queue is given only
  when the same actor staged it; an entry says nothing of another actor's. `sendable`
  says whether a submission by the node could still move the entry: a staged change
  whose holder can move; a staged registration; or a registration awaiting authority
  that the node now has, by its own record again or by the delegation of a change its
  creator owes on the same document. `authority` says whether a key or a delegation is
  recorded, never the token. **A submission that failed after its commit is found
  here**; the commit returned success.

  **A change the provider never took is not reported as settled.** A record is also
  deleted without the provider holding the change: the provider refused a change
  nobody had checked, cannot take one of its kind, a later change of the relationship
  passed an unchecked one, or its actor abandoned it. Each leaves a note
  (`/acp-owed2/x/<seq>`: the change and the reason, no credential), shown as an entry
  with `discarded` set. A note is deleted when its actor is told: by the wait for its
  transaction that returns it, or by the answer to a fresh credential for it
  (`OwedOutcome::Discarded`). A wait by the node's principal tells the node, and deletes
  the notes of every actor of that transaction. A node keeps at most 1,000 notes
  (`MAX_DISCARD_NOTES`) and drops the oldest past that; a change staged outside an
  explicit transaction has no wait, so its note stays until then.
- **The wait.** `OwedWait::Settled` when the provider holds everything the transaction
  owed; `Parked(entries)` when nothing left is sendable, discarded entries included;
  `Pending { entries,
  provider_unavailable }` when the deadline passed, or the provider could not be
  reached, with entries still to send. The call first submits once what can be sent
  (the registrations the transaction owes, then the changes on the documents it staged
  on), then sleeps until a record changes. It does not poll. A deadline that passes
  drops the wait with whatever it was submitting still in flight, as a commit that stops
  waiting does. The transaction is named by its handle. Handle numbers start again when
  the node restarts, so a record that names its transaction also names the run of the
  process that made it (`acp::outbox::run_epoch`), and a wait, like the pass after a
  commit, matches only records of the running node: an entry an earlier run left under
  the same number is neither returned nor waited for. Its actor finds it in the report.
  Every record written wakes every wait, which then reads all that is owed again. An
  actor's wait submits what is owed on the documents of its own entries, in order with
  whatever else is owed there, and makes no submission on another actor's document;
  the node's principal's wait submits for the whole transaction. A wait has no bound of
  its own: the caller's deadline ends it, submission included.
- **A fresh credential.** `credential` is a `SignerCredential`, the entry's actor's own:
  a plain delegation, one bound to exactly the owed command, or `None` where the node
  holds a key for the actor. It is asked about the owed command by the function that
  admits a write (`DocumentACP::command_authority`); there is no second check. A
  credential that would not have admitted the command (expired, signed by somebody
  else, made for another node, bound to another command, a plain one presented as
  bound) fails with `OwedAuthorityUnavailable` and records nothing. An admitted one is
  recorded with the entry, which is staged again with no failure counted, and what is
  owed on the entry's document is submitted once, in order. The answer is
  `OwedOutcome::Settled`, the entry as it stands (`Owed(entry)`: parked again when
  the provider refused it once more, staged when the provider could not be reached), or
  `Discarded(entry)` when its record was deleted without the provider holding it.

  Every fresh credential is checked as a verified write's is, whatever write the entry
  came from: it will be used when no request is there to vouch for it. A provider that
  gives the node nothing to check a delegation against (the Cosmos provider) therefore
  takes no delegation here. An entry there is sent again with a key the node holds
  (`SignerCredential::None`), or by a later request that itself carries the actor's
  bearer.

  A delegation bound to a registration settles the registration and answers `Settled`:
  it is used for that one command. The changes staged beside the registration then go,
  each with the authority recorded with it. One whose own delegation has run out is
  parked as awaiting authority and takes its own credential by another call. A client
  that holds only request-bound delegations therefore presents one per owed entry, the
  registration first.

  A refused change takes a fresh credential too, and is submitted again.
- **Abandoning.** Only a change in state `refused` is abandoned: the provider answered
  it, so no request for it is still on the way. A change that is staged or awaits
  authority may have an earlier request in flight (a commit that stopped waiting leaves
  one), and deleting its record would not stop that request; it fails with
  `OwedChangeNotRefused` and is given a fresh credential instead. The record is deleted
  without the provider holding the change, the changes behind it on its relationship are
  submitted, and the deletion is logged with the change. The call has succeeded once the
  record is deleted, whatever that submission comes to. When a transaction that
  committed meanwhile changed what is owed on the relationship, nothing is written and
  the call fails with `OwedEntryConflict`, to be made again; a fresh credential for a
  change or a registration that loses the same way fails the same way.

  **An abandoned delete stops denying.** Its subject is answered by the provider again,
  which refused the delete and so still holds the relationship; other nodes never
  stopped honouring it. That is correct only because the actor that staged the delete
  takes its own revocation back: the delete then counts for nothing, as if it had not
  been made. Nobody else may lift it, not the document's owner, not the subject it
  names, and not the node's principal unless the node staged the delete itself (`INV_RevokedAtCommit` with `AbandonBy` in
  `proofs/tla/AcpOutbox.tla`). The provider may hold a change it refused on a second
  submission (see *A change submitted twice*); abandoning changes nothing on the
  provider either way.
- **A registration and its authority.** The create records what lets the node register
  the document as its creator once the request is gone: `node_key` when the node holds
  a key for the creator, else the delegation the request **itself carries** for the
  creator, asked about that registration and kept as it was presented (plain, or bound
  to that registration). With neither the registration is recorded as awaiting
  authority, with the reason; the create itself is not refused (a verified create is,
  before it writes, as above). The fields are added to the `/acp-pending/` record only
  when they say something, so a record written before them keeps its bytes and reads as
  owed with nothing recorded.

  Every submission of it, at the commit, at start, before a change on its document, or
  through one of the calls above, sends it one of three ways, each checked again: with
  the bearer the request being served carries for the creator, when that admits the
  registration (the commit's own submission goes as its request did, and a request
  that carries none sends none); else with what
  the record holds; else with the first delegation, in queue order, recorded with a
  change the same creator still owes on the document that admits it. The report's
  `sendable` and a pass decide by the same selection, so what is reported as sendable
  is what a pass sends. These are the three ways `INV_PresentedAuthority` allows.

  **Nothing a process keeps beside its requests is read.** A registration, an
  unregistration and every other command sent to the provider for an actor go under a
  bearer the request carries for that actor, or a key the node holds
  (`RequestContext::bearer_token_for` returns the carried bearer and nothing else). A
  token bound to a DID in the process (`defra_core::signing::set_request_bearer_token`)
  is presented by no request:
  - a create whose request carries no bearer for its creator, by a creator whose key
    the node does not hold, commits, sends nothing at its commit and records no
    authority. Its registration is `awaiting_authority` in the report and takes a
    credential through `reauthorize_owed_acp_as`, or settles when a later request
    carries the creator's bearer;
  - no later submission finds one either: not the pass at start, not the settlement of
    an owed change, not a wait, and not another request that touches the document;
  - a relationship change, an access decision and the unregistration of a deleted
    document made by a request that carries no bearer are refused by the provider for
    want of a credential, where they used to go under the process token.

  **A carried delegation is written to disk.** A create on an external provider by a
  caller whose key the node does not hold writes the delegation its request carried,
  in the clear, into the document's `/acp-pending/` record, as a staged change always
  has. It stays there until the registration settles, is given another credential, or
  the document is deleted. No API returns it.

  A registration nothing lets the node send is not sent: it stays `awaiting_authority`
  in the report until its creator hands in a credential. One the provider refuses
  outright is kept as `refused` and tried again at start. A provider that already holds
  the document needs no authority: the record is cleared.
- **Limits.**
  - The node's drainer sends an owed registration only ahead of a relationship change
    owed on its document. A registration owed alone is sent again at start and by the
    calls above, not between them.
  - Only a relationship change the provider refuses is published
    (`acp-owed-change-refused`). Nothing is published when an entry settles, when a
    change is parked as awaiting authority, or when a registration is parked. A caller
    learns it from the wait or the report.
  - The report reads the whole queue and every owed registration to find one actor's
    entries, and so does every wait each time a record is written.
  - An owed registration cannot be abandoned, parked or not. Its document exists, and
    without the record it would be unregistered on the provider. Deleting the document
    clears it.
  - Registrations are not submitted one at a time. A failure is noted only on the
    record the submission was made for, so one that fails late does not park a
    registration that was given a fresh credential meanwhile.

The models are `proofs/tla/VerifiedRemoteWrite.tla` (admission, a submission that fails
after the commit and a fresh credential for it) and `proofs/tla/AcpOutbox.tla` (the
registration's recorded authority, the report of a failure, abandoning), with
`VerifiedRemoteAuthority.lean` for the rule itself.

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

## Parent-authorized document creation

A resource can declare which parent admits a new document. In this policy
fragment, registering a job requires the creator to own the named node.

```yaml
- name: jobs
  permissions: [{name: read}, {name: update}, {name: delete}]
  relations: [{name: parent, types: [nodes]}]
  create: {parent: parent, requires: owner}
```

Bind the child collection to the document field that carries the parent's ID.

```graphql
type Job @policy(id: "POLICY_ID", resource: "jobs", parentField: "nodeID") {
  nodeID: String!
}
```

The policy supplies the parent resource type. The field supplies one object ID.
Local registration checks the creator against that parent and writes the owner
and parent relationship in the document transaction. With Vera, the pending
registration keeps both parent identifiers across a restart. A delegation
bound to the registration covers both as well. Vera checks the acting identity
when it executes the command. A later grant on the parent relation requires
permission on the proposed target.
