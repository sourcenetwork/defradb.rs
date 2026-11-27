---- MODULE OwnerTransfer ----
\* The story, in plain words.
\*
\* A document's access rules live on a Vera deployment: who owns it, and the
\* relationships (a reader, say) granted on it. A DefraDB node changes them
\* through its outbox: each change is written down in the transaction that
\* makes it ("owed") and sent to the deployment after the commit, by the
\* node's drainer.
\*
\* A creator "c" creates a document and, in the same transaction, grants a
\* relationship to "g" and then gives the document away: it transfers its
\* ownership to "n". The create owes the document's registration, which the
\* deployment takes first. The grant and the transfer are owed after it, in
\* that order. The creator may instead give the document away later, in a
\* transaction of its own. Later someone may stage one more grant, to "x":
\* the owner of the moment, or anyone while the deployment cannot be asked
\* (an add is then staged unchecked, and the deployment decides it).
\*
\* A change takes its place in the queue (its sequence) when it is staged,
\* not when its transaction commits. Two transactions open at once, one
\* staging the transfer and one the later grant, can take their places in
\* one order and commit in the other. The queue would then send the later
\* committer's change first: a grant the creator committed while it still
\* owned the object would reach the deployment after the transfer and be
\* refused, or a grant committed after the transfer would land under the
\* creator. So of a transfer and another change staged on one object at
\* once, the one that commits second is aborted (CommitConflict), and its
\* caller stages it again behind the first.
\*
\* The deployment takes a change only from the object's owner at the time
\* it arrives. So the grant must arrive while "c" still owns the object, and
\* the later grant once "n" does: the transfer goes only after every earlier
\* change on its object was decided, and every later change on its object
\* waits for the transfer to be decided (ObjectOrder).
\*
\* The transfer is not atomic with the create. Between its commit and the
\* moment the deployment applies it, the deployment still names "c" as the
\* owner. The node must not let "c" read on that word, because the committed
\* changes, all applied, make "n" the owner and leave "c" nothing: from the
\* transfer's commit the node denies "c" on the object (DenyFromCommit), as
\* an owed revocation does. Nor may it let "n" in before the deployment does
\* (ProviderGrants): an owed transfer grants nothing, as an owed add grants
\* nothing.
\*
\* What else can happen: the node crashes and restarts; the deployment is
\* unreachable for a while; a change's credential lapses and is renewed; the
\* deployment answers a change with an error that says nothing about whether
\* it will ever take it, and a change it always answers so ("poisoned") is
\* refused after a bounded number of tries; the actor of a refused change
\* abandons it.
\*
\* What must never happen:
\*   * the node honours an access that the committed changes, all applied,
\*     would not give, or that the deployment does not give
\*     - INV_NoWidening;
\*   * a change the creator committed before the transfer reaches the
\*     deployment after the transfer did, where the deployment no longer
\*     takes it from the creator - INV_EarlierUnderOwner;
\*   * a change committed after the transfer is decided while the transfer
\*     is still owed - INV_LaterAfterTransfer;
\*   * the queue orders changes around a transfer otherwise than their
\*     transactions committed - INV_QueueFollowsCommits;
\*   * a change is sent while the document's registration is still owed
\*     - INV_RegistrationFirst;
\*   * the transfer takes effect twice, or is reported refused though it took
\*     effect - INV_TransferOnce, INV_RefusedTookNoEffect;
\*   * a change is removed without the deployment holding it and its actor
\*     is told nothing - INV_FailureReported.
\* What must happen: every committed change is decided (applied, refused,
\* or removed with a note) - LIVE_Decided.
\*
\* Code this models:
\*   crates/engine/acp/src/outbox/staging.rs      TransactionOutbox::stage_transfer (who may
\*       stage it: the owner, as the owed registration or the deployment names it, with no
\*       owed transfer or archive denying it)
\*   crates/engine/acp/src/outbox/ops.rs          account (an owed transfer counts as taking
\*       the owner relation from its actor, which denies it), stage (a transfer's validated
\*       scan of its object's live keys and its write of the object's transfer key, read
\*       validated by every other staging: CommitConflict)
\*   crates/engine/acp/src/outbox/keys.rs         transfer_key
\*   crates/engine/acp/src/outbox/deny.rs         owed_denial (OwedDenial::Always)
\*   crates/engine/query/src/txn/context.rs       decided_by_owed_archive (an owed transfer
\*       denies its old owner before an owed registration could let it in)
\*   crates/engine/acp/src/external.rs            ExternalDocumentAcp::check_doc_access (the
\*       same, through the provider)
\*   crates/engine/acp/src/external/transfer_order.rs  held_by_transfer (ObjectOrder)
\*   crates/engine/acp/src/external/settle.rs     submittable, register_first (RegFirst),
\*       conclude and confirmed_transfer (KeepRefused, OwnerConfirms), MAX_FAILED_SUBMISSIONS
\*       (BoundedRetries)
\*   crates/engine/acp/src/external/drain.rs      joins_request (ObjectOrder inside one
\*       request), send_request and recover_marked (Marker)
\* In the model these are StageCreate, StageGrant, StageTransfer, StageLater,
\* Commit, CommitAbort, Rollback, Mark, Land, Fail, Lookup, Crash,
\* RegisterOnChain, ClearOwed, Lapse, Reauthorize and Abandon.
\* Where the code differs from the model:
\*   * the code's conflict is regolith's: a validated read that a commit
\*     after it invalidates aborts the reader at its own commit. A settle
\*     deletes a live key, so one that commits while a transfer is staged on
\*     the object aborts the transfer too; the model's settles abort nothing,
\*     and an abort only ever narrows what commits;
\*   * the owner's own grant beside a transfer that commits first is
\*     already aborted by its read of the owed deletes naming it (the
\*     transfer merges into that count); the transfer key covers everyone
\*     else's changes, and archives;
\*   * the code sends the changes one transaction staged one after the other
\*     in one request, applied in order or not at all; a refused request is
\*     sent again one change at a time. Inside a request the order is the
\*     order below, so the model's one change at a time covers it;
\*   * the code refuses, in the transaction that staged a transfer, any later
\*     change on the object by the old owner: the transaction sees its own
\*     owed transfer deny it. The model's creator stages its grant first;
\*   * a change staged unchecked that waits for authority holds nothing on
\*     its object, as it holds nothing on its tuple. The model's changes
\*     before the transfer are always checked;
\*   * a provider without a journal (Marker = FALSE) sends a change again
\*     after a crash between its effect and the deletion of its record. The
\*     second transfer is refused, and the node settles it as applied because
\*     the provider names the new owner (OwnerConfirms): MC_OwnerTransfer_Green_NoMarker.
\* Not modelled: several documents, deleting the document (its archive
\* drops an unsent transfer with a note: ArchiveOnDelete.tla), reads inside
\* the open transaction (DeferredAcp.tla), and the registration's own
\* failures (AcpOutbox.tla).

EXTENDS Naturals  \* numbers

CONSTANTS
  RegFirst,        \* TRUE: no change is sent while the registration is owed
  ObjectOrder,     \* TRUE: a transfer waits for its object's earlier changes, and later ones for it
  CommitConflict,  \* TRUE: a transfer and another change on its object, staged concurrently, cannot both commit
  DenyFromCommit,  \* TRUE: an owed transfer denies its old owner on this node
  ProviderGrants,  \* TRUE: the new owner is let in only once the deployment names it
  Marker,          \* TRUE: the in-flight mark survives a crash, so a landed change is decided by its receipt
  OwnerConfirms,   \* TRUE: a refused transfer whose new owner the deployment names is settled
  KeepRefused,     \* TRUE: a refused change is kept as refused, which its actor sees
  BoundedRetries,  \* TRUE: a change the deployment keeps answering with an error is refused at last
  Poisoned,        \* the slots the deployment answers only with an error
  MaxFails,        \* how many such answers make a refusal
  MaxCrashes,      \* how often the node may crash
  MaxLapses,       \* how often a credential may lapse
  MaxAborts,       \* how often a transaction may be aborted by a conflict
  OwnTransfer      \* TRUE: the transfer may also be staged in a transaction of its own

\* The changes on the object, by role: 1 the creator's grant to "g" in the
\* create's transaction, 2 the transfer from "c" to "n" (in the create's
\* transaction or in one of its own), 3 a later grant to "x" in one of its
\* own. Where a change stands in the queue is its sequence, `seq`, taken
\* when it is staged; transactions open at once take theirs in the order
\* they stage, whatever order they then commit in.
Slots == {1, 2, 3}
\* The transfer's slot.
T == 2
\* The transactions: the create's, the transfer's own, the later grant's.
Txns == {"create", "transfer", "later"}
\* Who asks the node for access: creator, new owner, grantee, later grantee.
Readers == {"c", "n", "g", "x"}
\* The steps of a change still to be delivered.
LiveStates == {"staged", "inflight", "awaiting"}
\* The largest sequence: three changes, each staged again after an abort.
MaxSeq == 3 + MaxAborts

ASSUME Poisoned \subseteq Slots                         \* known slots
ASSUME MaxFails \in Nat /\ MaxCrashes \in Nat /\ MaxLapses \in Nat  \* bounds are numbers
ASSUME MaxAborts \in Nat /\ OwnTransfer \in BOOLEAN     \* a bound and a switch

VARIABLES
  doc,          \* "absent", "open" (created in the open transaction), "exists"
  owedReg,      \* TRUE while the node owes the document's registration
  st,           \* per slot: where its change stands (see TypeOK)
  txn,          \* per slot: the transaction that staged it ("-" while free)
  seq,          \* per slot: its place in the queue (0 while free)
  next,         \* the next sequence to hand out
  late,         \* per slot: the changes another transaction committed while it was open
  cnum,         \* per slot: how many commits came before its transaction's (0 while open)
  commits,      \* how many transactions committed
  actor,        \* per slot: who staged it ("-" before anyone did)
  chk,          \* per slot: TRUE when its actor was checked at staging
  chainOwner,   \* the deployment's owner of the document, "none" before it is registered
  chainTuples,  \* the grantees the deployment holds
  effects,      \* per slot: how often the change took effect on the deployment
  receipt,      \* per slot: what the deployment answered, "none", "ok" or "rejected"
  under,        \* per slot: the deployment's owner when it decided the change, "-" before
  checked,      \* per slot, in memory only: "sent, and known not to have landed yet"
  fails,        \* per slot: errors the deployment answered in a row
  noted,        \* the slots removed without the deployment holding them whose actor was told
  up,           \* TRUE while the deployment can be reached
  crashes,      \* how often the node crashed
  lapses,       \* how often a credential lapsed
  aborts        \* how often a transaction was aborted by a conflict

vars == <<doc, owedReg, st, txn, seq, next, late, cnum, commits, actor, chk, chainOwner,
          chainTuples, effects, receipt, under, checked, fails, noted, up,
          crashes, lapses, aborts>>  \* every variable

\* What a step about staging and committing leaves alone.
sending == <<chainOwner, chainTuples, effects, receipt, under, checked, fails,
             noted, up, crashes, lapses>>
\* What a step about sending leaves alone.
staging == <<doc, owedReg, txn, seq, next, late, cnum, commits, actor, chk, aborts>>

TypeOK ==
  /\ doc \in {"absent", "open", "exists"}                           \* a known document state
  /\ owedReg \in BOOLEAN                                            \* a flag
  /\ st \in [Slots -> {"free", "open", "staged", "inflight", "awaiting",
                       "settled", "refused", "dropped", "abandoned"}]  \* a known step each
  /\ txn \in [Slots -> Txns \cup {"-"}]                            \* a transaction or none each
  /\ seq \in [Slots -> 0..MaxSeq] /\ next \in 1..(MaxSeq + 1)      \* places in the queue
  /\ late \in [Slots -> SUBSET Slots]                               \* some slots each
  /\ cnum \in [Slots -> 0..3] /\ commits \in 0..3                    \* commit order
  /\ actor \in [Slots -> {"-", "c", "n"}]                           \* somebody or nobody each
  /\ chk \in [Slots -> BOOLEAN]                                     \* a flag each
  /\ chainOwner \in {"none", "c", "n"}                              \* a known owner
  /\ chainTuples \subseteq {"g", "x"}                               \* some grantees
  /\ effects \in [Slots -> 0..2]                                    \* a small count each
  /\ receipt \in [Slots -> {"none", "ok", "rejected"}]              \* a known answer each
  /\ under \in [Slots -> {"-", "none", "c", "n"}]                   \* an owner or nobody each
  /\ checked \in [Slots -> BOOLEAN]                                 \* a flag each
  /\ fails \in [Slots -> 0..MaxFails]                               \* a small count each
  /\ noted \subseteq Slots                                          \* some slots
  /\ up \in BOOLEAN                                                 \* a flag
  /\ crashes \in 0..MaxCrashes /\ lapses \in 0..MaxLapses           \* small counts
  /\ aborts \in 0..MaxAborts                                        \* a small count

Init ==
  /\ doc = "absent"                                     \* no document yet
  /\ owedReg = FALSE                                    \* nothing owed
  /\ st = [s \in Slots |-> "free"]                      \* no slot taken
  /\ txn = [s \in Slots |-> "-"]                        \* by no transaction
  /\ seq = [s \in Slots |-> 0] /\ next = 1              \* no place handed out
  /\ late = [s \in Slots |-> {}]                        \* nothing committed beside anything
  /\ cnum = [s \in Slots |-> 0] /\ commits = 0          \* nothing committed yet
  /\ actor = [s \in Slots |-> "-"]                      \* nobody staged anything
  /\ chk = [s \in Slots |-> FALSE]                      \* nothing checked
  /\ chainOwner = "none"                                \* the deployment does not know it
  /\ chainTuples = {}                                   \* and holds no grant
  /\ effects = [s \in Slots |-> 0]                      \* nothing took effect
  /\ receipt = [s \in Slots |-> "none"]                 \* nothing answered
  /\ under = [s \in Slots |-> "-"]                      \* nothing decided
  /\ checked = [s \in Slots |-> FALSE]                  \* the loop remembers nothing
  /\ fails = [s \in Slots |-> 0]                        \* no error yet
  /\ noted = {}                                         \* nothing removed
  /\ up = TRUE                                          \* the deployment answers
  /\ crashes = 0 /\ lapses = 0 /\ aborts = 0            \* nothing went wrong yet

\* ---- Words used below ----

\* The change is owed and still to be delivered.
Live(s) == st[s] \in LiveStates
\* The node keeps a record of the change: owed, or parked as refused.
Kept(s) == Live(s) \/ st[s] = "refused"
\* The change belongs to a committed transaction and its actor still stands
\* by it: everything but a free, open or abandoned slot.
Counts(s) == st[s] \in LiveStates \cup {"settled", "refused", "dropped"}
\* The slots transaction x has staged and not yet committed.
OpenIn(x) == {s \in Slots : st[s] = "open" /\ txn[s] = x}
\* Change r goes before change s in the queue.
Before(r, s) == seq[r] < seq[s]

\* ---- The two states the node's answers are measured against ----

\* The owner once every committed change is applied: "n" when the transfer counts.
IntendedOwner == IF Counts(T) THEN "n" ELSE "c"
\* The grantees once every committed change is applied.
IntendedTuples == {IF s = 1 THEN "g" ELSE "x" : s \in {r \in {1, 3} : Counts(r)}}
\* The committed changes, all applied, let this reader in.
IntendedAllows(who) == who = IntendedOwner \/ who \in IntendedTuples
\* The deployment lets this reader in now: its owner, or a grantee it holds.
ProviderAllows(who) == up /\ (who = chainOwner \/ who \in chainTuples)

\* ---- What the node answers ----

\* An owed transfer is kept: it denies its old owner while DenyFromCommit holds.
TransferKept == Kept(T)
\* The node's answer to one reader of the committed document.
\*   * An owed transfer denies its old owner, before anything else is asked.
\*   * While the registration is owed, only the creator reads.
\*   * Else the deployment decides; the bug of ProviderGrants = FALSE also
\*     lets the new owner of an owed transfer in.
Honours(who) ==
  /\ doc = "exists"                                                \* a committed document
  /\ ~(DenyFromCommit /\ TransferKept /\ who = "c")                \* not the old owner of an owed transfer
  /\ IF owedReg
     THEN who = "c"                                                \* owed: the creator alone
     ELSE \/ ProviderAllows(who)                                   \* else the deployment says yes,
          \/ ~ProviderGrants /\ TransferKept /\ who = "n"          \* or (bug) the owed transfer does

\* ---- Staging and committing ----

\* Stage change s in transaction x as `who`, checked or not: it takes the
\* next place in the queue now, before its transaction commits.
Take(s, x, who, checked_) ==
  /\ st[s] = "free" /\ next <= MaxSeq                   \* a free slot, a place left
  /\ st' = [st EXCEPT ![s] = "open"]                    \* staged, uncommitted
  /\ txn' = [txn EXCEPT ![s] = x]                       \* in x
  /\ seq' = [seq EXCEPT ![s] = next] /\ next' = next + 1  \* its place, taken now
  /\ late' = [late EXCEPT ![s] = {}]                   \* nothing committed beside it yet
  /\ actor' = [actor EXCEPT ![s] = who]                 \* by who
  /\ chk' = [chk EXCEPT ![s] = checked_]                \* checked or not

\* The create's transaction creates the document; its registration is owed with it.
StageCreate ==
  /\ doc = "absent"                                     \* no document yet
  /\ doc' = "open"                                      \* now one, uncommitted
  /\ UNCHANGED <<owedReg, st, txn, seq, next, late, cnum, commits, actor, chk, aborts>>
  /\ UNCHANGED sending

\* The creator grants "g" in the create's transaction. The owed registration
\* makes it the only authority, so the check passes without asking anyone.
StageGrant ==
  /\ doc = "open" /\ st[T] = "free"                     \* in the create, before the transfer
  /\ Take(1, "create", "c", TRUE)                       \* by the creator, checked
  /\ UNCHANGED <<doc, owedReg, cnum, commits, aborts>>
  /\ UNCHANGED sending

\* The owner as the node sees it at staging: the owed registration's
\* creator, or the owner the deployment names.
SeenOwner(who) == (doc = "open" \/ owedReg) => who = "c"

\* The creator gives the document to "n": in the create's transaction, its
\* last change there, or in a transaction of its own once the document is
\* committed. Only the owner may, a transfer is never staged unchecked, and
\* an owed transfer already denies its old owner.
StageTransfer ==
  /\ \/ doc = "open" /\ Take(T, "create", "c", TRUE)    \* in the create,
     \/ /\ OwnTransfer /\ doc = "exists"                \* or in its own,
        /\ owedReg \/ (up /\ chainOwner = "c")          \*   checked against the owner
        /\ Take(T, "transfer", "c", TRUE)
  /\ UNCHANGED <<doc, owedReg, cnum, commits, aborts>>
  /\ UNCHANGED sending

\* A later transaction grants "x" as `who`. The check sees what is
\* committed: the old owner of a committed, owed transfer is refused
\* (DenyFromCommit); else the owner the deployment names passes, and while
\* the deployment cannot be asked an add is staged unchecked. A transfer
\* still open in another transaction is not seen.
StageLater(who) ==
  /\ doc = "exists" /\ ~owedReg                          \* the document is committed and registered
  /\ ~(DenyFromCommit /\ TransferKept /\ who = "c")     \* not the old owner of an owed transfer
  /\ \/ up /\ who = chainOwner /\ Take(3, "later", who, TRUE)  \* the owner, checked,
     \/ ~up /\ Take(3, "later", who, FALSE)                    \* or unchecked while unreachable
  /\ UNCHANGED <<doc, owedReg, cnum, commits, aborts>>
  /\ UNCHANGED sending

\* Transaction x and a change r another transaction committed after x
\* staged something on the object conflict when one of them is a transfer:
\* the transfer's staging reads, validated, every change staged on its
\* object, and every other change's staging reads, validated, the object's
\* transfer mark, which a transfer writes. The later of the two to commit is
\* aborted, so changes on one object around a transfer commit in the order
\* of their sequences.
Conflicts(x) ==
  \E s \in OpenIn(x), r \in Slots :
    /\ r \in late[s]                                     \* r committed while s was open
    /\ s = T \/ r = T                                    \* and a transfer is one of them

\* Transaction x commits: its changes are owed from now on, and the create's
\* registration with them. With CommitConflict, a transaction that conflicts
\* is aborted instead and its changes are gone (CommitAbort).
Commit(x) ==
  /\ OpenIn(x) # {} \/ (x = "create" /\ doc = "open")   \* x has something to commit
  /\ ~(CommitConflict /\ Conflicts(x))                   \* and no conflict aborts it
  /\ doc' = (IF x = "create" THEN "exists" ELSE doc)    \* the create's document is there
  /\ owedReg' = (owedReg \/ x = "create")               \* its registration is owed
  /\ st' = [s \in Slots |-> IF s \in OpenIn(x) THEN "staged" ELSE st[s]]  \* owed now
  /\ late' = [s \in Slots |-> IF st[s] = "open" /\ txn[s] # x
                              THEN late[s] \cup OpenIn(x) ELSE late[s]]  \* seen by the open ones
  /\ cnum' = [s \in Slots |-> IF s \in OpenIn(x) THEN commits + 1 ELSE cnum[s]]  \* its turn
  /\ commits' = IF commits < 3 THEN commits + 1 ELSE commits  \* one more
  /\ UNCHANGED <<txn, seq, next, actor, chk, aborts>>
  /\ UNCHANGED sending

\* What rolling back the open slots `S` leaves: free slots, nobody's.
Free(S) ==
  /\ st' = [s \in Slots |-> IF s \in S THEN "free" ELSE st[s]]
  /\ txn' = [s \in Slots |-> IF s \in S THEN "-" ELSE txn[s]]
  /\ seq' = [s \in Slots |-> IF s \in S THEN 0 ELSE seq[s]]
  /\ actor' = [s \in Slots |-> IF s \in S THEN "-" ELSE actor[s]]

\* A conflict aborts transaction x at its commit: nothing of it is left, and
\* its caller may stage again, taking new places in the queue.
CommitAbort(x) ==
  /\ CommitConflict /\ Conflicts(x) /\ aborts < MaxAborts  \* a conflict, and aborts are not endless
  /\ Free(OpenIn(x))                                     \* its changes go
  /\ doc' = (IF x = "create" THEN "absent" ELSE doc)    \* and the create's document
  /\ aborts' = aborts + 1                               \* one more
  /\ UNCHANGED <<owedReg, next, late, cnum, commits, chk>>
  /\ UNCHANGED sending

\* Transaction x rolls back of its own accord: nothing of it is left.
Rollback(x) ==
  /\ OpenIn(x) # {} \/ (x = "create" /\ doc = "open")   \* x is open
  /\ Free(OpenIn(x))                                     \* its changes go
  /\ doc' = (IF x = "create" THEN "absent" ELSE doc)    \* and the create's document
  /\ UNCHANGED <<owedReg, next, late, cnum, commits, chk, aborts>>
  /\ UNCHANGED sending

\* ---- The registration ----

\* The deployment registers the creator as the owner.
RegisterOnChain ==
  /\ owedReg /\ up /\ chainOwner = "none"               \* owed, reachable, not yet there
  /\ chainOwner' = "c"                                  \* the creator owns it
  /\ UNCHANGED <<chainTuples, effects, receipt, under, checked, fails, noted, up,
                 crashes, lapses, st>>
  /\ UNCHANGED staging

\* The node clears its record of the registration once the deployment holds it.
ClearOwed ==
  /\ owedReg /\ up /\ chainOwner # "none"               \* the deployment holds the document
  /\ owedReg' = FALSE                                   \* nothing owed any more
  /\ UNCHANGED <<doc, txn, seq, next, late, cnum, commits, actor, chk, aborts, st>>
  /\ UNCHANGED sending

\* ---- Sending the changes ----

\* An earlier change, or the transfer, holds this one back: the transfer
\* waits for every earlier change of its object still to be delivered, and
\* a later change waits for a transfer still to be delivered.
HeldByOrder(s) ==
  \E r \in Slots : Before(r, s) /\ Live(r) /\ (s = T \/ r = T)

\* The drainer may send this change now.
Ready(s) ==
  /\ st[s] = "staged"                                   \* owed and not yet sent
  /\ RegFirst => ~owedReg                               \* the registration is settled
  /\ ObjectOrder => ~HeldByOrder(s)                     \* its turn on the object has come

\* The drainer marks the change in flight, durably, before it is sent.
Mark(s) ==
  /\ Ready(s)                                           \* it may go
  /\ st' = [st EXCEPT ![s] = "inflight"]                \* marked
  /\ checked' = [checked EXCEPT ![s] = TRUE]            \* known not to have landed
  /\ receipt' = [receipt EXCEPT ![s] = "none"]          \* a new request, with no answer yet
  /\ UNCHANGED <<chainOwner, chainTuples, effects, under, fails, noted, up, crashes, lapses>>
  /\ UNCHANGED staging

\* The deployment takes a change only from the object's owner at the time.
Accepts(s) == chainOwner = actor[s]

\* The request lands. The deployment applies a change from the owner and
\* rejects any other, and records which; the drainer must ask.
Land(s) ==
  /\ st[s] = "inflight" /\ checked[s] /\ up             \* in flight, known not landed, reachable
  /\ s \notin Poisoned                                  \* a change it answers at all
  /\ IF Accepts(s)
     THEN /\ chainOwner' = (IF s = T THEN "n" ELSE chainOwner)  \* a transfer moves the owner,
          /\ chainTuples' = (IF s = 1 THEN chainTuples \cup {"g"}  \* a grant adds its grantee
                             ELSE IF s = 3 THEN chainTuples \cup {"x"}
                             ELSE chainTuples)
          /\ effects' = [effects EXCEPT ![s] = IF @ < 2 THEN @ + 1 ELSE @]  \* counted
          /\ receipt' = [receipt EXCEPT ![s] = "ok"]    \* and receipted
     ELSE /\ receipt' = [receipt EXCEPT ![s] = "rejected"]  \* refused, receipted,
          /\ UNCHANGED <<chainOwner, chainTuples, effects>>  \* nothing changes
  /\ under' = [under EXCEPT ![s] = chainOwner]          \* decided under this owner
  /\ checked' = [checked EXCEPT ![s] = FALSE]           \* the drainer must ask
  /\ UNCHANGED <<st, fails, noted, up, crashes, lapses>>
  /\ UNCHANGED staging

\* Where a change the deployment refused ends: kept as refused when its
\* actor was checked (its actor sees it), removed with a note when nobody
\* vouched for it; the bug of KeepRefused = FALSE removes it silently.
Refusal(s) == IF chk[s] /\ KeepRefused THEN "refused" ELSE "dropped"
\* Whom a removal tells.
Told(s) == IF chk[s] /\ ~KeepRefused THEN noted ELSE noted \cup {s}
\* Where a refusal of this change ends: a transfer whose new owner the
\* deployment already names took effect, so it is settled
\* (OwnerConfirms); anything else is refused.
Declined(s) ==
  IF s = T /\ OwnerConfirms /\ chainOwner = "n" THEN "settled" ELSE Refusal(s)

\* The deployment answers with an error that does not say whether it will
\* ever take the change; nothing took effect. Enough of them in a row are a
\* refusal, unless (the bug) the node tries for ever.
Fail(s) ==
  /\ st[s] = "inflight" /\ checked[s] /\ up             \* in flight, reachable
  /\ IF BoundedRetries /\ fails[s] + 1 >= MaxFails
     THEN /\ st' = [st EXCEPT ![s] = Declined(s)]       \* taken as refused
          /\ noted' = (IF Declined(s) = "dropped" THEN Told(s) ELSE noted)  \* a removal is told
          /\ fails' = fails                             \* the count stays
     ELSE /\ st' = [st EXCEPT ![s] = "staged"]          \* sent again later
          /\ fails' = [fails EXCEPT ![s] = IF @ < MaxFails THEN @ + 1 ELSE @]  \* one more
          /\ noted' = noted                             \* nothing to tell
  /\ checked' = [checked EXCEPT ![s] = FALSE]           \* the request is answered
  /\ UNCHANGED <<chainOwner, chainTuples, effects, receipt, under, up, crashes, lapses>>
  /\ UNCHANGED staging

\* The drainer asks for the receipt of a change in flight: also the first
\* thing it does with one after a restart. A refused transfer whose new
\* owner the deployment already names took effect (OwnerConfirms).
Lookup(s) ==
  /\ st[s] = "inflight" /\ ~checked[s] /\ up            \* in flight, outcome unknown
  /\ CASE receipt[s] = "ok" ->                          \* it landed:
            /\ st' = [st EXCEPT ![s] = "settled"]       \*   the record is deleted
            /\ UNCHANGED <<checked, noted>>
       [] receipt[s] = "rejected" ->                    \* it was refused:
            /\ st' = [st EXCEPT ![s] = Declined(s)]     \*   settled, parked, or removed
            /\ noted' = (IF Declined(s) = "dropped" THEN Told(s) ELSE noted)
            /\ UNCHANGED checked
       [] OTHER ->                                      \* no receipt:
            /\ checked' = [checked EXCEPT ![s] = TRUE]  \*   it may be sent again
            /\ UNCHANGED <<st, noted>>
  /\ UNCHANGED <<chainOwner, chainTuples, effects, receipt, under, fails, up, crashes, lapses>>
  /\ UNCHANGED staging

\* ---- The world ----

\* The node crashes and restarts. Every open transaction is rolled back, the
\* drainer forgets what it knew, and without a durable mark a change in
\* flight reads as never sent.
Crash ==
  /\ crashes < MaxCrashes                               \* crashes are not endless
  /\ crashes' = crashes + 1                             \* one more
  /\ doc' = (IF doc = "open" THEN "absent" ELSE doc)    \* the open create is gone
  /\ st' = [s \in Slots |->
              IF st[s] = "open" THEN "free"             \* with every open change
              ELSE IF st[s] = "inflight" /\ ~Marker THEN "staged"  \* (bug) the mark is lost
              ELSE st[s]]
  /\ txn' = [s \in Slots |-> IF st[s] = "open" THEN "-" ELSE txn[s]]  \* forgotten
  /\ seq' = [s \in Slots |-> IF st[s] = "open" THEN 0 ELSE seq[s]]
  /\ actor' = [s \in Slots |-> IF st[s] = "open" THEN "-" ELSE actor[s]]
  /\ checked' = [s \in Slots |-> FALSE]                 \* the drainer remembers nothing
  /\ UNCHANGED <<owedReg, next, late, cnum, commits, chk, aborts>>
  /\ UNCHANGED <<chainOwner, chainTuples, effects, receipt, under, fails, noted, up, lapses>>

\* A staged change's credential lapses: it waits for a fresh one.
Lapse(s) ==
  /\ st[s] = "staged" /\ lapses < MaxLapses             \* owed, and lapses are not endless
  /\ st' = [st EXCEPT ![s] = "awaiting"]                \* parked for authority
  /\ lapses' = lapses + 1                               \* one more
  /\ UNCHANGED <<chainOwner, chainTuples, effects, receipt, under, checked, fails, noted,
                 up, crashes>>
  /\ UNCHANGED staging

\* Its actor hands in a fresh credential: owed again.
Reauthorize(s) ==
  /\ st[s] = "awaiting"                                 \* parked for authority
  /\ st' = [st EXCEPT ![s] = "staged"]                  \* owed again
  /\ UNCHANGED sending
  /\ UNCHANGED staging

\* The creator gives up its refused transfer: it counts for nothing any
\* more. (Abandoning a grant is AcpOutbox.tla's.)
Abandon(s) ==
  /\ s = T /\ st[s] = "refused"                         \* the transfer, refused and kept
  /\ st' = [st EXCEPT ![s] = "abandoned"]               \* withdrawn by its actor
  /\ UNCHANGED sending
  /\ UNCHANGED staging

\* The deployment goes away, or comes back.
ProviderDown ==
  /\ up /\ up' = FALSE                                  \* unreachable now
  /\ UNCHANGED <<st, chainOwner, chainTuples, effects, receipt, under, checked, fails,
                 noted, crashes, lapses>>
  /\ UNCHANGED staging
ProviderUp ==
  /\ ~up /\ up' = TRUE                                  \* reachable again
  /\ UNCHANGED <<st, chainOwner, chainTuples, effects, receipt, under, checked, fails,
                 noted, crashes, lapses>>
  /\ UNCHANGED staging

\* Any one thing happens.
Next ==
  \/ StageCreate \/ StageGrant \/ StageTransfer          \* staging in the create
  \/ \E who \in {"c", "n"} : StageLater(who)            \* a later grant
  \/ \E x \in Txns : Commit(x) \/ CommitAbort(x) \/ Rollback(x)  \* ending transactions
  \/ RegisterOnChain \/ ClearOwed                       \* the registration
  \/ \E s \in Slots : Mark(s) \/ Land(s) \/ Fail(s) \/ Lookup(s)  \* sending
  \/ \E s \in Slots : Lapse(s) \/ Reauthorize(s) \/ Abandon(s)    \* authority, giving up
  \/ Crash \/ ProviderDown \/ ProviderUp                \* the world

\* Safety: start, then take steps.
Spec == Init /\ [][Next]_vars

\* Liveness adds what is promised: the deployment comes back, the
\* registration settles, the drainer takes its turns, a request lands or is
\* answered, and a lapsed credential is renewed. Staging, committing,
\* crashes, outages, lapses and abandoning are promised nothing.
SpecLive ==
  /\ Spec                                               \* all of the above
  /\ WF_vars(ProviderUp)                                \* the deployment comes back
  /\ SF_vars(RegisterOnChain) /\ SF_vars(ClearOwed)     \* the registration settles
  /\ \A s \in Slots : WF_vars(Mark(s))                  \* the drainer sends,
  /\ \A s \in Slots : SF_vars(Land(s))                  \* a request lands,
  /\ \A s \in Slots : SF_vars(Fail(s))                  \* or is answered with an error,
  /\ \A s \in Slots : SF_vars(Lookup(s))                \* its receipt is read,
  /\ \A s \in Slots : WF_vars(Reauthorize(s))           \* and fresh authority arrives

\* ---- What must hold ----

\* The node never honours more than the committed changes, all applied,
\* give, nor more than the deployment gives, but for the creator of a
\* registration still owed, whom the deployment registers first.
INV_NoWidening ==
  \A who \in Readers :
    Honours(who) => /\ IntendedAllows(who)
                    /\ ProviderAllows(who) \/ (owedReg /\ who = "c")

\* Change s committed before the transfer: in an earlier transaction, or in
\* the same one and staged before it.
CommittedBefore(s) ==
  /\ Counts(s) /\ Counts(T) /\ s # T                    \* both committed
  /\ IF txn[s] = txn[T] THEN Before(s, T) ELSE cnum[s] < cnum[T]

\* A change by the creator that committed before the transfer is decided by
\* the deployment while the creator still owns the object.
INV_EarlierUnderOwner ==
  \A s \in Slots : CommittedBefore(s) /\ actor[s] = "c" => under[s] \in {"-", "c"}

\* A change committed after the transfer is decided only once the transfer is.
INV_LaterAfterTransfer ==
  \A s \in Slots : (Counts(s) /\ Counts(T) /\ s # T /\ ~CommittedBefore(s) /\ under[s] # "-")
    => ~Live(T)

\* Changes on the object around a transfer hold their places in the queue in
\* the order their transactions committed in.
INV_QueueFollowsCommits ==
  \A s \in Slots : (Counts(s) /\ Counts(T) /\ s # T) => (CommittedBefore(s) <=> Before(s, T))

\* No change goes out while the registration is owed: one that went finds
\* the document registered.
INV_RegistrationFirst ==
  \A s \in Slots : st[s] \in {"inflight", "settled", "refused"} \/ receipt[s] # "none"
    => (~owedReg /\ chainOwner # "none")

\* The transfer takes effect at most once.
INV_TransferOnce == effects[T] <= 1

\* A transfer reported refused took no effect. (A grant sent again by a
\* provider without a journal may land twice and then be refused; that is
\* AcpOutbox.tla's INV_AtMostOneEffect, which holds only with a journal.)
INV_RefusedTookNoEffect == st[T] = "refused" => effects[T] = 0

\* A change removed without the deployment holding it leaves a note for its actor.
INV_FailureReported == \A s \in Slots : st[s] = "dropped" => s \in noted

\* Every committed change ends decided, and stays so.
LIVE_Decided == <>[](\A s \in Slots : ~Live(s))

====
