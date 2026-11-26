---- MODULE ArchiveOnDelete ----
\* The story, in plain words.
\*
\* A document's access rules live on a Vera deployment: its owner, and the
\* relationships (a reader, say) granted on it. A DefraDB node changes them
\* through its outbox: each change is recorded in the transaction that makes
\* it ("owed") and sent to the deployment afterwards, in order, by the
\* node's drainer. An owed revocation already denies on the node.
\*
\* When the document is deleted, its access rules must go too: otherwise a
\* former reader still holds its grant on the deployment, and a later
\* document with the same identifier inherits it. So the delete records, in
\* its own transaction, an owed "archive" of the object. Vera removes every
\* relationship of an archived object and its owner no longer counts. Anyone
\* the policy lets delete may delete: the owner, or a writer. Someone the
\* policy does not let delete (a stranger) is refused before the delete
\* commits: nothing is deleted. Only the owner may archive on Vera, so the
\* archive is made as the owner whoever deleted, and sent only under the
\* owner's authority: a key the node holds for the owner, or the owner's own
\* delegation. When the node has none, the archive is parked until the owner
\* hands one in; it is never sent under the writer's authority.
\*
\* Changes still owed on the object when it is deleted cannot matter any
\* more: those not yet handed to the worker are dropped with the delete.
\* One already handed to the worker is sent before the archive (the
\* worker's requests go in sequence), and Vera refuses any change to an
\* archived object, so no grant lands after the archive. From the delete's
\* commit on, the owed archive denies everyone on the object, the owner
\* too, until the deployment holds it. One read is left while it is owed:
\* an owner that deleted the document itself still reads its commit
\* history, so it can see the delete it just made. After a writer's delete
\* nobody does: the archive's actor (the owner) did not make it.
\*
\* Two more things can stand behind a read of the object. The owner's
\* registration can still be owed (the create recorded it, the deployment
\* has not confirmed it): until it lands only the creator reads, and the
\* outbox holds every change on the object behind it. And the deployment
\* can lack the object (never registered there, or dropped by it): an
\* object it does not have is public by the unregistered rule. Neither may
\* reopen a deleted document, so the owed archive decides first, before
\* the owed registration and before the registration check.
\*
\* The outbox also bounds how much may be owed at once, three ways: every
\* owed change counts against its actor's admission and against the
\* node's, and against what its transaction may stage; a new change is
\* refused once any of those counts reaches its bound. An archive is never
\* refused (a delete is not), so it counts against none of them: were it
\* charged to the owner, a writer deleting many of the owner's documents,
\* each archive parked for want of the owner's authority, would lock the
\* owner out of staging anything new; were it charged to the node, enough
\* of them would lock everyone out; were it charged to its transaction, a
\* transaction that deletes many documents could stage nothing more of its
\* own. There is one archive per deleted document, so the documents bound
\* them. The model runs every step in one transaction, so its count is
\* the transaction's.
\*
\* What must never happen:
\*   * a deleted document grants anything to anyone;
\*   * anyone but its deleter reads a deleted document's commit history;
\*   * a grant that holds on the deployment after the archive settled;
\*   * a document deleted by someone the policy does not let delete;
\*   * an archive owed, or sent, as anyone but the owner;
\*   * a delete counts against anyone's admission of new changes, the
\*     node's and its own transaction's included.
\* What must happen: the archive settles, and the deployment then holds no
\* relationship and no live owner for the object.
\*
\* Code this models (defradb.rs, vera.rs at 45cd086b):
\*   crates/engine/query/src/runner/mutation.rs   the delete permission check before the
\*       delete is made
\*   crates/engine/acp/src/outbox/staging.rs      TransactionOutbox::stage_archive (the
\*       archive made as the owner under the owner's authority or parked, earlier unsent
\*       changes on the object dropped)
\*   crates/engine/acp/src/outbox/deny.rs         owed_denial (an owed archive denies all),
\*       archive_actor (its actor, when that actor deleted the document itself)
\*   crates/engine/query/src/txn/context.rs       decided_by_owed, spared_by_owed_archive
\*       (the archive's actor keeps the commit history), decided_by_owed_archive (an owed
\*       archive decides before an owed registration or the registration check)
\*   crates/engine/query/src/txn/read_access.rs   OverlayChecker::object_access
\*   crates/engine/acp/src/external.rs            ExternalDocumentAcp::check_doc_access (the
\*       owed archive before a registration still owed)
\*   crates/engine/acp/src/pending.rs             PendingRegistrations (the registration
\*       still owed to the creator)
\*   crates/engine/acp/src/outbox/report.rs       OwedHold (a change waits behind its
\*       object's owed registration)
\*   crates/engine/acp/src/outbox/ops.rs          account (the object's archive count; an
\*       archive's record is outside the node's admission count and its actor's,
\*       StageMarks and OwedChange::outside_admission_counts)
\*   crates/engine/acp/src/outbox/changes.rs      OwedChanges::stage (the node's bound,
\*       the per-actor bound, read from those counts, and the transaction's bound, read
\*       from its StagedCount), OwedChanges::stage_archive (leaves that StagedCount as is)
\*   crates/engine/acp/src/outbox/submission.rs   relationship_call (OwedOp::Archive)
\*   crates/database/db/src/write/autocommit/helpers.rs  unregister_deleted_doc (stages
\*       the archive in the delete's transaction)
\*   vera-modules:src/acp/object_archive.rs  cmd_archive_object (owner
\*       only, relationships removed); vera-modules:src/acp/management.rs (no
\*       change to an archived object)
\*
\* Correspondence table (model -> Rust):
\*   queue, inflight      the outbox queue in sequence order; a change whose request the
\*                        worker journaled (its in-flight mark with a hash)
\*   Delete, CheckPermission  the mutation runner's delete check refuses a stranger
\*   ArchiveAsOwner       stage_archive records the owner as the archive's actor and asks
\*                        command_authority for the owner, never for the deleter
\*   Drop                 stage_archive discards earlier unsent changes on the object,
\*                        parked ones included
\*   Park, Renew          a change parked awaiting authority; reauthorize_owed
\*   ParkArchive          stage_archive parks the archive when nothing lets the node sign
\*                        for the owner (OwedAuthorityUnavailable): no key for it, and no
\*                        delegation of the owner's own delete; it still denies
\*   RenewArchive         reauthorize_owed by the owner (the archive's actor)
\*   Settle, VeraRefuses  the drainer's pass; Vera's management check refuses a change to an
\*                        archived object
\*   Overlay              owed_denial: an unsettled archive on the object denies everyone
\*   owedReg, unknown     a PendingRegistrations record for the object; the provider's
\*                        is_doc_registered answering no
\*   RegistrationLands, ProviderDrops  the registration drainer clears the record once the
\*                        provider holds it; the provider loses the object
\*   Decide, First        ExternalDocumentAcp::check_doc_access (archive_owed, then owed,
\*                        then the provider) and, through the overlay,
\*                        check_doc_access_with_projection,
\*                        check_doc_access_with_prepared_inputs, doc_read_access and
\*                        OverlayChecker::object_access: decided_by_owed_archive, then
\*                        owed_creator, then is_doc_registered_with_projection (the
\*                        unregistered rule), then the provider. First = "archive" is
\*                        that order; the other values put a later check ahead of it
\*   ReadHistory, HistoryActorOnly   check_commit_read_access_as -> decided_by_owed:
\*                        OwedDenial::Archived spares only the archive's actor, and only
\*                        for the commit history. archive_actor names nobody while a
\*                        delete of that actor's owner relation is owed too (the model
\*                        has one owner, whose owner relation nobody deletes)
\*   SpareOnlyDeleter     OwedChange::deleted_by_actor: archive_actor names the actor only
\*                        when it made the delete
\*   charged, ArchiveUncounted  the per-actor admission count (keys::actor_key) that
\*                        ops::account merges; an archive staged with
\*                        outside_admission_counts leaves it untouched
\*   nodeCharged, ArchiveOffNode  the node's admission count (keys::UNSETTLED_KEY) that
\*                        ops::account merges and OwedChanges::stage checks; an archive
\*                        staged with outside_admission_counts leaves it untouched too
\*   txnStaged, ArchiveOffTxn  the transaction's StagedCount, checked by OwedChanges::stage
\*                        against MAX_OWED_CHANGES_PER_TRANSACTION; OwedChanges::stage_archive
\*                        does not add to it
EXTENDS Naturals, Sequences

CONSTANTS
    CheckPermission, \* TRUE: a delete by someone the policy does not let delete is refused
    ArchiveAsOwner,  \* TRUE: the archive is made as the owner, whoever deleted
    Drop,         \* TRUE: earlier unsent changes on the object go with the delete
    VeraRefuses,  \* TRUE: Vera refuses a change to an archived object
    Overlay,      \* TRUE: an owed archive denies everyone on the object
    HistoryActorOnly, \* TRUE: an owed archive leaves the commit history to its actor alone
    SpareOnlyDeleter, \* TRUE: and to that actor only when it made the delete
    ArchiveUncounted, \* TRUE: an archive counts against no actor's admission
    ArchiveOffNode,   \* TRUE: nor against the node's
    ArchiveOffTxn,    \* TRUE: nor against what its transaction may stage
    First         \* which check a read of the object asks first: "archive" (the owed
                  \* archive, as the code does), "owedRegistration" (the registration
                  \* still owed to the creator) or "unregistered" (the unregistered rule)

\* Who acts: the owner, a writer the policy lets delete, and a stranger it
\* does not.
People == {"owner", "writer", "stranger"}

\* Whom the policy lets delete.
Deleters == {"owner", "writer"}

\* Who asks for the commit history: they, and the reader the owner granted.
Askers == People \cup {"reader"}

VARIABLES
    owedReg,     \* TRUE while the owner's registration is owed to the deployment
    unknown,     \* TRUE while the deployment does not have the object
    doc,         \* "live" or "deleted" on this node
    deleter,     \* who deleted it, "" while live
    queue,       \* owed changes in order: records [op, actor, inflight]
    parked,      \* grants parked as awaiting authority (their actor must renew it)
    parkedArchive, \* TRUE while the archive waits for its owner's authority
    renewed,     \* TRUE once the owner handed in authority for the archive
    granted,     \* TRUE while the reader's grant holds on the deployment
    archived,    \* TRUE once the deployment archived the object
    grants,      \* how many grants the owner may still stage (keeps the model finite)
    servedWide,  \* TRUE once a decision allowed the reader on a deleted document
    lateGrant,   \* TRUE once a grant held on the deployment after the archive
    servedHistory, \* TRUE once someone other than its deleter read a deleted document's history
    charged,       \* each actor's admission count: the owed changes charged to it
    nodeCharged,   \* the node's admission count: the owed changes charged to the node
    txnStaged      \* the transaction's count: the changes it staged, settled or not

vars == <<owedReg, unknown, doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>

\* What an archive adds to its actor's admission count: nothing, or one
\* with ~ArchiveUncounted.
ArchiveCharge == IF ArchiveUncounted THEN 0 ELSE 1

\* What an archive adds to the node's admission count: nothing, or one
\* with ~ArchiveOffNode.
NodeArchiveCharge == IF ArchiveOffNode THEN 0 ELSE 1

\* What an archive adds to its transaction's count: nothing, or one with
\* ~ArchiveOffTxn.
TxnArchiveCharge == IF ArchiveOffTxn THEN 0 ELSE 1

\* How many grants are owed, queued or parked: all of them the owner's.
OwedGrants == Len(SelectSeq(queue, LAMBDA c : c.op = "grant")) + parked

\* How many queued grants a delete drops: those not handed to the worker.
DroppedGrants == Len(SelectSeq(queue, LAMBDA c : c.op = "grant" /\ ~c.inflight))

\* Who the archive is made as: the owner, or with ~ArchiveAsOwner whoever
\* deleted.
ArchiveActor == IF ArchiveAsOwner THEN "owner" ELSE deleter

\* Whether an archive is owed.
ArchiveOwed == parkedArchive \/ \E i \in DOMAIN queue : queue[i].op = "archive"

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ owedReg \in BOOLEAN /\ unknown \in BOOLEAN                       \* the registration
    /\ doc \in {"live", "deleted"} /\ deleter \in People \cup {""}     \* the document
    /\ queue \in Seq([op : {"grant", "archive"}, actor : People, inflight : BOOLEAN])  \* the outbox
    /\ parked \in 0..2 /\ parkedArchive \in BOOLEAN /\ renewed \in BOOLEAN  \* parked changes
    /\ granted \in BOOLEAN /\ archived \in BOOLEAN                     \* the deployment
    /\ grants \in 0..2                                                  \* a count
    /\ servedWide \in BOOLEAN /\ lateGrant \in BOOLEAN                 \* history flags
    /\ servedHistory \in BOOLEAN                                      \* one more
    /\ charged \in [People -> 0..3]                                  \* small counts
    /\ nodeCharged \in 0..3                                          \* and the node's
    /\ txnStaged \in 0..3                                            \* and the transaction's

\* At the start: a live document, nothing owed, the reader not yet granted.
Init ==
    /\ owedReg \in BOOLEAN /\ unknown \in BOOLEAN  \* owed or not, held by the deployment or not
    /\ owedReg => unknown                       \* an owed registration is not on the deployment yet
    /\ doc = "live" /\ deleter = ""             \* a live document
    /\ queue = <<>> /\ parked = 0 /\ parkedArchive = FALSE /\ renewed = FALSE  \* nothing owed
    /\ granted = FALSE /\ archived = FALSE      \* the deployment holds the owner only
    /\ grants = 2                               \* two grants may be staged
    /\ servedWide = FALSE /\ lateGrant = FALSE  \* nothing served yet
    /\ servedHistory = FALSE                    \* no history read yet
    /\ charged = [p \in People |-> 0]           \* nobody owes anything
    /\ nodeCharged = 0                          \* and the node owes nothing
    /\ txnStaged = 0                            \* and the transaction has staged nothing

\* The owner grants the reader: an owed change.
StageGrant ==
    /\ doc = "live" /\ grants > 0               \* a live document, a grant left to make
    /\ queue' = Append(queue, [op |-> "grant", actor |-> "owner", inflight |-> FALSE])
    /\ grants' = grants - 1                     \* one fewer to make
    /\ charged' = [charged EXCEPT !["owner"] = @ + 1]  \* charged to the owner, its actor
    /\ nodeCharged' = nodeCharged + 1           \* and to the node
    /\ txnStaged' = txnStaged + 1               \* and to the transaction staging it
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parked, parkedArchive, renewed, granted, archived, servedWide, lateGrant, servedHistory>>  \* nothing else moves

\* The drainer hands the head change to the worker: its request is journaled
\* and will be sent before anything journaled after it.
Journal ==
    /\ queue # <<>> /\ ~queue[1].inflight      \* the head is not yet handed over
    /\ ~owedReg                                 \* it waits behind the object's owed registration
    /\ queue' = [queue EXCEPT ![1].inflight = TRUE]  \* now it is
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* Someone deletes the document. With CheckPermission only whom the policy
\* lets delete may: anyone else is refused and nothing commits. The delete
\* records the archive in its own transaction, as ArchiveActor; with Drop,
\* earlier changes not yet handed to the worker go with it, and leave the
\* owner's admission count and the node's. The archive adds ArchiveCharge
\* to its actor's, NodeArchiveCharge to the node's and TxnArchiveCharge to
\* the transaction's; a dropped change stays counted there, as it was staged.
Delete ==
    /\ doc = "live"                             \* there is a document to delete
    /\ \E who \in People :
        /\ (~CheckPermission \/ who \in Deleters)  \* the refusal before commit
        /\ doc' = "deleted" /\ deleter' = who   \* the delete commits
        /\ LET kept == IF Drop THEN SelectSeq(queue, LAMBDA c : c.inflight) ELSE queue
               actor == IF ArchiveAsOwner THEN "owner" ELSE who
           IN queue' = Append(kept, [op |-> "archive", actor |-> actor, inflight |-> FALSE])
        /\ parked' = IF Drop THEN 0 ELSE parked  \* parked grants go too
        /\ LET actor == IF ArchiveAsOwner THEN "owner" ELSE who   \* the archive's actor
               dropped == IF Drop THEN DroppedGrants + parked ELSE 0  \* the owner's grants gone
               less == [charged EXCEPT !["owner"] = @ - dropped]  \* they no longer count
           IN charged' = [less EXCEPT ![actor] = @ + ArchiveCharge]  \* the archive's charge
        /\ LET dropped == IF Drop THEN DroppedGrants + parked ELSE 0  \* the grants gone
           IN nodeCharged' = nodeCharged - dropped + NodeArchiveCharge  \* off the node, archive on
        /\ txnStaged' = txnStaged + TxnArchiveCharge  \* what the archive adds to the transaction's count
        /\ UNCHANGED <<parkedArchive, renewed>>
    /\ UNCHANGED <<owedReg, unknown, granted, archived, grants, servedWide, lateGrant, servedHistory>>  \* the deployment has not seen it

\* The head change, handed to the worker, is sent and executed. A grant on an
\* archived object is refused when VeraRefuses. An archive by the owner
\* removes every relationship; one by anyone else is refused (and dropped,
\* reported to its actor).
Settle ==
    /\ queue # <<>> /\ queue[1].inflight       \* the head's request was journaled
    /\ LET c == queue[1] IN
        IF c.op = "grant"
        THEN /\ granted' = (granted \/ ~(archived /\ VeraRefuses))  \* applied unless refused
             /\ lateGrant' = (lateGrant \/ (archived /\ ~VeraRefuses))  \* landed after the archive?
             /\ UNCHANGED archived
        ELSE IF c.actor = "owner"
        THEN /\ archived' = TRUE /\ granted' = FALSE              \* every relationship removed
             /\ UNCHANGED lateGrant
        ELSE UNCHANGED <<archived, granted, lateGrant, servedHistory>>          \* only the owner may archive
    /\ queue' = Tail(queue)                     \* settled or refused, it leaves the queue
    /\ charged' = [charged EXCEPT ![queue[1].actor] =   \* and its actor's count
                      @ - (IF queue[1].op = "grant" THEN 1 ELSE ArchiveCharge)]
    /\ nodeCharged' = nodeCharged -               \* and the node's
                      (IF queue[1].op = "grant" THEN 1 ELSE NodeArchiveCharge)
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parked, parkedArchive, renewed, grants, servedWide, servedHistory, txnStaged>>  \* nothing else moves; a settled change stays staged

\* The head grant's delegation has expired: it is parked as awaiting
\* authority and the changes behind it go on.
Park ==
    /\ queue # <<>> /\ queue[1].op = "grant" /\ ~queue[1].inflight  \* an unsent grant at the head
    /\ queue' = Tail(queue) /\ parked' = parked + 1  \* it waits aside
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* Its actor hands in a fresh delegation: the parked grant is owed again,
\* behind whatever is owed now.
Renew ==
    /\ parked > 0                                \* a grant waits aside
    /\ queue' = Append(queue, [op |-> "grant", actor |-> "owner", inflight |-> FALSE])
    /\ parked' = parked - 1
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* Nothing lets the node sign for the archive's actor (no key for it, and a
\* delete that carried no delegation of its): the archive at the head waits
\* for that authority. It still denies (ArchiveOwed).
ParkArchive ==
    /\ queue # <<>> /\ queue[1].op = "archive" /\ ~queue[1].inflight  \* an unsent archive at the head
    /\ ~renewed                                  \* not the one sent under a credential handed in
    /\ queue' = Tail(queue) /\ parkedArchive' = TRUE  \* it waits aside
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parked, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* The archive's actor hands in a credential: the archive is owed again.
RenewArchive ==
    /\ parkedArchive                             \* the archive waits aside
    /\ queue' = Append(queue, [op |-> "archive", actor |-> ArchiveActor, inflight |-> FALSE])
    /\ parkedArchive' = FALSE /\ renewed' = TRUE  \* now it carries the owner's credential
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, parked, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* The owner's owed registration lands: the deployment now holds the
\* object, and the record is cleared. A change waiting behind it can go.
RegistrationLands ==
    /\ owedReg                                  \* a registration is owed
    /\ owedReg' = FALSE /\ unknown' = FALSE     \* the deployment holds it, the record goes
    /\ UNCHANGED <<doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* The deployment loses an object it has not archived (a reset, say). The
\* model counts a settled archive as the deployment's answer for the
\* object, so it is lost only before that.
ProviderDrops ==
    /\ ~unknown /\ ~archived                    \* held, and not archived yet
    /\ unknown' = TRUE                          \* now the deployment does not have it
    /\ UNCHANGED <<owedReg, doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* The four checks a read of the object can meet, each saying what it
\* decides for `who`, or that it lets the next one decide.
\* The owed archive: it decides (denies) while an archive is owed.
ArchiveDecides == Overlay /\ ArchiveOwed
\* The owed registration: it decides (only the creator, the owner) while owed.
RegistrationDecides == owedReg
\* The unregistered rule: an object the deployment does not have, with no
\* registration owed and no archive settled, is public.
UnregisteredDecides == unknown /\ ~owedReg /\ ~archived
\* The deployment: its owner, and the reader while its grant holds, on an
\* object it has not archived.
DeploymentAllows(who) == ~archived /\ (who = "owner" \/ (who = "reader" /\ granted))

\* The order the code asks them in: the owed archive first, then the owed
\* registration, then the unregistered rule, then the deployment.
ArchiveFirst(who) ==
    IF ArchiveDecides THEN FALSE                  \* the owed archive denies
    ELSE IF RegistrationDecides THEN who = "owner"  \* the creator alone
    ELSE IF UnregisteredDecides THEN TRUE         \* public
    ELSE DeploymentAllows(who)                    \* the deployment answers

\* The bug before the fix: the owed registration asked before the archive.
OwedRegistrationFirst(who) ==
    IF RegistrationDecides THEN who = "owner"     \* the creator, deleted or not
    ELSE IF ArchiveDecides THEN FALSE             \* the archive comes second
    ELSE IF UnregisteredDecides THEN TRUE         \* public
    ELSE DeploymentAllows(who)                    \* the deployment answers

\* The other bug before the fix: the unregistered rule asked before the archive.
UnregisteredFirst(who) ==
    IF UnregisteredDecides THEN TRUE              \* public, deleted or not
    ELSE IF ArchiveDecides THEN FALSE             \* the archive comes second
    ELSE IF RegistrationDecides THEN who = "owner"  \* the creator alone
    ELSE DeploymentAllows(who)                    \* the deployment answers

\* What a read of the object decides for `who`, in the order First names.
Allows(who) ==
    CASE First = "archive"          -> ArchiveFirst(who)           \* the code's order
      [] First = "owedRegistration" -> OwedRegistrationFirst(who)  \* registration first
      [] First = "unregistered"     -> UnregisteredFirst(who)      \* the rule first

\* Someone asks for access to the object, and the checks above answer.
Decide ==
    /\ \E who \in Askers :
        servedWide' = (servedWide \/ (Allows(who) /\ doc = "deleted"))  \* allowed on a deleted document?
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, lateGrant, servedHistory, charged, nodeCharged, txnStaged>>  \* nothing else moves

\* Someone asks for the document's commit history. While the archive is
\* owed the node answers by itself: with HistoryActorOnly only the
\* archive's actor reads it, and with SpareOnlyDeleter only when that actor
\* made the delete. Otherwise the other checks
\* answer, in the code's order: the owed registration, the unregistered
\* rule, then the deployment (the owner of an object it has not archived,
\* and the reader while its grant holds).
ReadHistory ==
    /\ \E who \in Askers :
        LET allows == IF Overlay /\ ArchiveOwed                       \* the owed archive answers
                      THEN \/ ~HistoryActorOnly                       \* everyone
                           \/ /\ who = ArchiveActor                    \* its actor alone,
                              /\ (~SpareOnlyDeleter \/ who = deleter)   \* if it deleted
                      ELSE ArchiveFirst(who)                          \* the rest, in the code's order
        IN servedHistory' = (servedHistory \/ (allows /\ doc = "deleted" /\ who # deleter))  \* someone else's read?
    /\ UNCHANGED <<owedReg, unknown, doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant, charged, nodeCharged, txnStaged>>  \* nothing else moves

Next == RegistrationLands \/ ProviderDrops \/ StageGrant \/ Journal \/ Delete \/ Settle \/ Park \/ Renew \/ ParkArchive \/ RenewArchive
        \/ Decide \/ ReadHistory   \* one of these happens at each step

\* The drainer keeps handing over and sending what is owed, the owed
\* registration among it, and an owner whose archive waits for authority
\* hands one in.
Spec == Init /\ [][Next]_vars /\ WF_vars(RegistrationLands) /\ WF_vars(Journal) /\ WF_vars(Settle) /\ WF_vars(RenewArchive)

\* A deleted document grants nothing to anyone.
INV_DeletedGrantsNothing == ~servedWide

\* No grant holds on the deployment after the archive settled.
INV_NoGrantAfterArchive == ~lateGrant

\* Nobody but its deleter reads a deleted document's commit history.
INV_HistoryOnlyToDeleter == ~servedHistory

\* A document is deleted only by someone the policy lets delete.
INV_DeletedByPermitted == doc = "deleted" => deleter \in Deleters

\* Every archive owed, queued or parked, is made as the owner: none is ever
\* sent under anyone else's authority.
INV_ArchiveAsOwner ==
    /\ \A i \in DOMAIN queue : queue[i].op = "archive" => queue[i].actor = "owner"
    /\ parkedArchive => ArchiveActor = "owner"

\* A delete counts against nobody's admission, the node's and its
\* transaction's included: each actor's count is what it owes besides
\* archives, the owner's grants and nobody else's, the node's count is
\* those grants alone, and the transaction's is the grants it staged.
INV_DeleteChargesNobody ==
    /\ \A p \in People : charged[p] = IF p = "owner" THEN OwedGrants ELSE 0  \* per actor
    /\ nodeCharged = OwedGrants                                           \* the node
    /\ txnStaged = 2 - grants                                             \* the transaction

\* A deleted document ends archived, with no relationship left.
LIVE_Retired == (doc = "deleted") ~> (archived /\ ~granted)
====
