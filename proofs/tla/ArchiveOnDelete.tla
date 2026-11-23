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
\* relationship of an archived object and its owner no longer counts. Only
\* the owner may archive, so a delete by anyone else (a writer the policy
\* lets delete) is refused before it commits: nothing is deleted.
\*
\* Changes still owed on the object when it is deleted cannot matter any
\* more: those not yet handed to the worker are dropped with the delete.
\* One already handed to the worker is sent before the archive (the
\* worker's requests go in sequence), and Vera refuses any change to an
\* archived object, so no grant lands after the archive. From the delete's
\* commit on, the owed archive denies everyone on the object, the owner
\* too, until the deployment holds it.
\*
\* What must never happen:
\*   * a deleted document grants anything to anyone;
\*   * a grant that holds on the deployment after the archive settled;
\*   * a document deleted by someone who may not archive it.
\* What must happen: the archive settles, and the deployment then holds no
\* relationship and no live owner for the object.
\*
\* Code this models (defradb.rs, vera.rs at 45cd086b):
\*   crates/engine/acp/src/outbox/staging.rs      TransactionOutbox::stage_archive (owner
\*       check before commit, earlier unsent changes on the object dropped)
\*   crates/engine/acp/src/outbox/deny.rs         owed_denial (an owed archive denies all)
\*   crates/engine/acp/src/outbox/ops.rs          account (the object's archive count)
\*   crates/engine/acp/src/outbox/submission.rs   relationship_call (OwedOp::Archive)
\*   crates/database/db/src/write/autocommit/helpers.rs  unregister_deleted_doc (stages
\*       the archive in the delete's transaction)
\*   vera.rs crates/vera-modules/src/acp/object_archive.rs  cmd_archive_object (owner
\*       only, relationships removed), management.rs (no change to an archived object)
\*
\* Correspondence table (model -> Rust):
\*   queue, inflight      the outbox queue in sequence order; a change whose request the
\*                        worker journaled (its in-flight mark with a hash)
\*   Delete, CheckOwner   unregister_deleted_doc -> stage_archive; NotOwner refuses the delete
\*   Drop                 stage_archive discards earlier unsent changes on the object,
\*                        parked ones included
\*   Park, Renew          a change parked awaiting authority; reauthorize_owed
\*   ParkArchive          stage_archive parks the owner's archive when its request lets the
\*                        node sign nothing (OwedAuthorityUnavailable); it still denies
\*   Settle, VeraRefuses  the drainer's pass; Vera's management check refuses a change to an
\*                        archived object
\*   Overlay              owed_denial: an unsettled archive on the object denies everyone
\*   Decide               check_doc_access through ExternalDocumentAcp
EXTENDS Naturals, Sequences

CONSTANTS
    CheckOwner,   \* TRUE: a delete by anyone but the owner is refused before commit
    Drop,         \* TRUE: earlier unsent changes on the object go with the delete
    VeraRefuses,  \* TRUE: Vera refuses a change to an archived object
    Overlay       \* TRUE: an owed archive denies everyone on the object

\* Who acts: the owner, and a writer the policy lets delete.
People == {"owner", "writer"}

VARIABLES
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
    lateGrant    \* TRUE once a grant held on the deployment after the archive

vars == <<doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant>>

\* Whether an archive is owed.
ArchiveOwed == parkedArchive \/ \E i \in DOMAIN queue : queue[i].op = "archive"

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ doc \in {"live", "deleted"} /\ deleter \in People \cup {""}     \* the document
    /\ queue \in Seq([op : {"grant", "archive"}, actor : People, inflight : BOOLEAN])  \* the outbox
    /\ parked \in 0..2 /\ parkedArchive \in BOOLEAN /\ renewed \in BOOLEAN  \* parked changes
    /\ granted \in BOOLEAN /\ archived \in BOOLEAN                     \* the deployment
    /\ grants \in 0..2                                                  \* a count
    /\ servedWide \in BOOLEAN /\ lateGrant \in BOOLEAN                 \* history flags

\* At the start: a live document, nothing owed, the reader not yet granted.
Init ==
    /\ doc = "live" /\ deleter = ""             \* a live document
    /\ queue = <<>> /\ parked = 0 /\ parkedArchive = FALSE /\ renewed = FALSE  \* nothing owed
    /\ granted = FALSE /\ archived = FALSE      \* the deployment holds the owner only
    /\ grants = 2                               \* two grants may be staged
    /\ servedWide = FALSE /\ lateGrant = FALSE  \* nothing served yet

\* The owner grants the reader: an owed change.
StageGrant ==
    /\ doc = "live" /\ grants > 0               \* a live document, a grant left to make
    /\ queue' = Append(queue, [op |-> "grant", actor |-> "owner", inflight |-> FALSE])
    /\ grants' = grants - 1                     \* one fewer to make
    /\ UNCHANGED <<doc, deleter, parked, parkedArchive, renewed, granted, archived, servedWide, lateGrant>>  \* nothing else moves

\* The drainer hands the head change to the worker: its request is journaled
\* and will be sent before anything journaled after it.
Journal ==
    /\ queue # <<>> /\ ~queue[1].inflight      \* the head is not yet handed over
    /\ queue' = [queue EXCEPT ![1].inflight = TRUE]  \* now it is
    /\ UNCHANGED <<doc, deleter, parked, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant>>  \* nothing else moves

\* Someone deletes the document. With CheckOwner only the owner may: anyone
\* else is refused and nothing commits. The delete records the archive in
\* its own transaction; with Drop, earlier changes not yet handed to the
\* worker go with it.
Delete ==
    /\ doc = "live"                             \* there is a document to delete
    /\ \E who \in People :
        /\ (~CheckOwner \/ who = "owner")       \* the refusal before commit
        /\ doc' = "deleted" /\ deleter' = who   \* the delete commits
        /\ LET kept == IF Drop THEN SelectSeq(queue, LAMBDA c : c.inflight) ELSE queue
           IN queue' = Append(kept, [op |-> "archive", actor |-> who, inflight |-> FALSE])
        /\ parked' = IF Drop THEN 0 ELSE parked  \* parked grants go too
        /\ UNCHANGED <<parkedArchive, renewed>>
    /\ UNCHANGED <<granted, archived, grants, servedWide, lateGrant>>  \* the deployment has not seen it

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
        ELSE UNCHANGED <<archived, granted, lateGrant>>          \* only the owner may archive
    /\ queue' = Tail(queue)                     \* settled or refused, it leaves the queue
    /\ UNCHANGED <<doc, deleter, parked, parkedArchive, renewed, grants, servedWide>>  \* nothing else moves

\* The head grant's delegation has expired: it is parked as awaiting
\* authority and the changes behind it go on.
Park ==
    /\ queue # <<>> /\ queue[1].op = "grant" /\ ~queue[1].inflight  \* an unsent grant at the head
    /\ queue' = Tail(queue) /\ parked' = parked + 1  \* it waits aside
    /\ UNCHANGED <<doc, deleter, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant>>  \* nothing else moves

\* Its actor hands in a fresh delegation: the parked grant is owed again,
\* behind whatever is owed now.
Renew ==
    /\ parked > 0                                \* a grant waits aside
    /\ queue' = Append(queue, [op |-> "grant", actor |-> "owner", inflight |-> FALSE])
    /\ parked' = parked - 1
    /\ UNCHANGED <<doc, deleter, parkedArchive, renewed, granted, archived, grants, servedWide, lateGrant>>  \* nothing else moves

\* The owner deleted through a request that lets the node sign nothing for
\* it: the archive at the head waits for the owner's authority. It still
\* denies (ArchiveOwed).
ParkArchive ==
    /\ queue # <<>> /\ queue[1].op = "archive" /\ ~queue[1].inflight  \* an unsent archive at the head
    /\ ~renewed                                  \* not the one sent under a credential handed in
    /\ queue' = Tail(queue) /\ parkedArchive' = TRUE  \* it waits aside
    /\ UNCHANGED <<doc, deleter, parked, renewed, granted, archived, grants, servedWide, lateGrant>>  \* nothing else moves

\* The owner hands in a credential: the archive is owed again.
RenewArchive ==
    /\ parkedArchive                             \* the archive waits aside
    /\ queue' = Append(queue, [op |-> "archive", actor |-> "owner", inflight |-> FALSE])
    /\ parkedArchive' = FALSE /\ renewed' = TRUE  \* now it carries the owner's credential
    /\ UNCHANGED <<doc, deleter, parked, granted, archived, grants, servedWide, lateGrant>>  \* nothing else moves

\* The reader asks for access to the object. The deployment allows a held
\* grant on an object it has not archived; an owed archive denies first.
Decide ==
    /\ LET allows == granted /\ ~archived /\ ~(Overlay /\ ArchiveOwed)
       IN servedWide' = (servedWide \/ (allows /\ doc = "deleted"))  \* allowed on a deleted document?
    /\ UNCHANGED <<doc, deleter, queue, parked, parkedArchive, renewed, granted, archived, grants, lateGrant>>  \* nothing else moves

Next == StageGrant \/ Journal \/ Delete \/ Settle \/ Park \/ Renew \/ ParkArchive \/ RenewArchive
        \/ Decide   \* one of these happens at each step

\* The drainer keeps handing over and sending what is owed, and an owner
\* whose archive waits for authority hands one in.
Spec == Init /\ [][Next]_vars /\ WF_vars(Journal) /\ WF_vars(Settle) /\ WF_vars(RenewArchive)

\* A deleted document grants nothing to anyone.
INV_DeletedGrantsNothing == ~servedWide

\* No grant holds on the deployment after the archive settled.
INV_NoGrantAfterArchive == ~lateGrant

\* A document is deleted only by someone who may archive it.
INV_DeletedByOwner == doc = "deleted" => deleter = "owner"

\* A deleted document ends archived, with no relationship left.
LIVE_Retired == (doc = "deleted") ~> (archived /\ ~granted)
====
