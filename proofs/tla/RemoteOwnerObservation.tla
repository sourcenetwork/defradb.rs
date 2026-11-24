------------------------ MODULE RemoteOwnerObservation ------------------------
\* The story, in plain words.
\*
\* An embedder holds a transaction on a Node and asks who owns a document,
\* any number of times, then decides something in the same transaction and
\* commits. The Node's document ACP is a remote provider (vera.rs): it cannot
\* join the transaction, so its answers do not live in the transaction's view
\* and nothing in the storage engine sees them change.
\*
\* Which answer the Node gives:
\*   - a registration this transaction, or an earlier committed one, still
\*     owes the provider names its creator as the owner. That record is read
\*     from the transaction's own view as a validated point read. It is the
\*     same rule reads and verified writes apply to an owed registration: the
\*     creator reads and updates its document while the registration is owed.
\*     An owed creator is the Node's own claim, not the provider's word;
\*   - otherwise the provider's owner, read at the edge (outside the
\*     transaction, as a prepared input) and kept in the transaction once the
\*     whole lookup answered.
\*
\* What happens at commit:
\*   - the commit first looks at what the transaction kept, in the same step
\*     on the transaction's core that commits it (commit_unless_told_as). With
\*     no provider answer kept, it commits there and then;
\*   - with answers kept, it does not commit: the end has begun, so from then
\*     on no request joins the transaction (a lookup or a write finds no
\*     transaction), and only another commit or a discard reaches it. The edge
\*     reads every kept answer again from the provider, outside the
\*     transaction, at or after the head known when the commit began, in one
\*     round. The commit then compares every answer the transaction kept, not
\*     only those it saw at the look, and fails with a conflict ("owner
\*     changed") when any differs;
\*   - a record the answer was read from changes before the commit (an earlier
\*     transaction's registration settles): the validated read conflicts;
\*   - the provider cannot be reached: the commit is refused, retryably, and
\*     nothing commits;
\*   - the registration is contested at settlement (the provider already holds
\*     the document under another owner): settlement keeps the provider's
\*     owner and the replica yields, by the existing rule. A decision committed
\*     on the owed creator before that rested on the Node's own claim, exactly
\*     as reads and writes did; the contest voids the claim, it does not make
\*     the commit fail.
\*
\* The window, stated and not hidden: the decision rests on an owner the
\* provider confirmed at a revision no older than the commit's start. A
\* change finalized between the confirmation read and the local commit is not
\* caught. It cannot be: the provider's revisions and the local commit share
\* no order, so no read the Node makes can be the last word at the instant
\* its storage commits. MC_RemoteOwnerObservation_Red_WindowWitness shows the
\* model allows it.
\*
\* What must never happen:
\*   - a provider answer commits that the provider held at no revision since
\*     the commit began (a stale owner);
\*   - an answer the transaction kept commits without being confirmed, one
\*     kept after the commit looked included;
\*   - a request joins the transaction after its commit began, one that
\*     passed the caller's early check before the commit began included;
\*   - an owed registration is ignored: the answer comes from the provider
\*     while a registration is owed, reporting no owner (or another one) for a
\*     document its creator can already read;
\*   - a commit on an owed creator after the record it was read from changed;
\*   - a commit while the provider could not confirm.
\*
\* Rust owners: crates/engine/query/src/runner/owners.rs owner_job,
\* remote_owner; crates/engine/query/src/txn/observed_owners.rs ObservedOwners;
\* crates/engine/query/src/runner/owner_confirmation.rs commit_confirming;
\* crates/database/db/src/txn/registry/owned.rs OwnedTxn::commit_unless_told,
\* OwnedTxn::commit_confirmed; crates/database/db/src/txn/owner/mod.rs
\* Owners::finish_unless; crates/database/db/src/txn/owner/table.rs Table::runs,
\* Table::send_request;
\* crates/engine/query/src/txn/context.rs AcpRegistrationProjection::owed_creator.
EXTENDS Naturals

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

\* The document's possible owners: its creator on this Node, somebody else,
\* or nobody (unregistered or archived).
Owners == {"creator", "other", "none"}

\* Every state variable, with what it stands for.
VARIABLES
  owedBy,     \* who owes the registration: "none", "txn" (this transaction) or "earlier"
  record,     \* version of the owed record; a settle bumps it
  provider,   \* the provider's current owner of the document
  rev,        \* the provider's revision; every finalized change bumps it
  reachable,  \* TRUE while the provider answers
  phase,      \* "open", "plain" (split look only), "confirming", "committed", "conflict", "refused"
  sawOwed,    \* TRUE once a lookup answered from the owed record
  seen,       \* the record version that lookup read
  kept,       \* the provider answers the transaction kept
  snapshot,   \* the answers the commit's look returned to the edge
  joined,     \* TRUE once a request ran after the commit began
  startRev,   \* the provider revision known when the commit began
  confirmed,  \* the owner the confirmation round read, "unread" before it
  confirmRev, \* the revision the confirmation read at
  heldSince,  \* every owner the provider held at some revision since the commit began
  atCommit,   \* the provider's owner at the instant the commit landed
  changes,    \* provider changes left to explore (keeps the model finite)
  pending     \* TRUE while a request passed the caller's early check and is not yet on the owner

\* The tuple of all variables, for stuttering steps.
vars == <<owedBy, record, provider, rev, reachable, phase, sawOwed, seen, kept,
          snapshot, joined, startRev, confirmed, confirmRev, heldSince, atCommit,
          changes, pending>>

\* The type of every variable.
TypeOK ==
  \* The registration is owed by nobody, this transaction or an earlier one.
  /\ owedBy \in {"none", "txn", "earlier"}
  \* At most two settles bump the record.
  /\ record \in 0..2
  \* The provider names one of the owners.
  /\ provider \in Owners
  \* At most two changes bump the revision.
  /\ rev \in 0..2
  \* The provider is up or down.
  /\ reachable \in BOOLEAN
  \* The transaction is in one of its phases.
  /\ phase \in {"open", "plain", "confirming", "committed", "conflict", "refused"}
  \* A lookup used the owed record or not.
  /\ sawOwed \in BOOLEAN
  \* The record version that lookup read.
  /\ seen \in 0..2
  \* The answers kept are owners.
  /\ kept \subseteq Owners
  \* The answers the look returned are owners.
  /\ snapshot \subseteq Owners
  \* A request joined after the commit began or not.
  /\ joined \in BOOLEAN
  \* The revision the commit began at.
  /\ startRev \in 0..2
  \* The confirmation read an owner or nothing yet.
  /\ confirmed \in Owners \cup {"unread"}
  \* The revision the confirmation read at.
  /\ confirmRev \in 0..2
  \* The owners held since the commit began.
  /\ heldSince \subseteq Owners
  \* The owner at the commit's instant, once it landed.
  /\ atCommit \in Owners \cup {"unread"}
  \* Changes left to explore.
  /\ changes \in 0..2
  \* A request is on its way to the owner or not.
  /\ pending \in BOOLEAN

\* The document is registered on the provider already, owed by an earlier
\* committed transaction, or created in this one.
Init ==
  \* Registered before: nothing owed, the provider names an owner.
  /\ \/ owedBy = "none" /\ provider \in {"creator", "other"}
     \* Owed by an earlier transaction: the provider has it or not (contested).
     \/ owedBy = "earlier" /\ provider \in {"none", "other"}
     \* Created in this transaction: likewise.
     \/ owedBy = "txn" /\ provider \in {"none", "other"}
  \* The record has not been settled.
  /\ record = 0
  \* The provider starts at its first revision.
  /\ rev = 0
  \* The provider may be up or down.
  /\ reachable \in BOOLEAN
  \* The transaction is open.
  /\ phase = "open"
  \* No lookup has answered.
  /\ sawOwed = FALSE
  \* No record version read yet.
  /\ seen = 0
  \* Nothing kept.
  /\ kept = {}
  \* No look yet.
  /\ snapshot = {}
  \* Nobody joined.
  /\ joined = FALSE
  \* No commit began.
  /\ startRev = 0
  \* Nothing confirmed.
  /\ confirmed = "unread"
  \* No confirmation revision.
  /\ confirmRev = 0
  \* Nothing held since a commit that has not begun.
  /\ heldSince = {}
  \* Nothing landed.
  /\ atCommit = "unread"
  \* Two changes to explore.
  /\ changes = 2
  \* No request on its way.
  /\ pending = FALSE

\* While a commit is under way every owner the provider takes is noted.
Note(owner) == IF phase \in {"plain", "confirming"} THEN heldSince \cup {owner} ELSE heldSince

\* What happens after the transaction ended decides nothing in it.
Ended == phase \in {"committed", "conflict", "refused"}

\* A transfer or an archive finalized on the provider, by anybody, at any time.
RemoteChange ==
  \* Only while the transaction has not ended.
  /\ ~Ended
  \* Within the bound on explored changes.
  /\ changes > 0
  \* The provider moves to another owner, noted if a commit is under way.
  /\ \E next \in Owners \ {provider}:
       /\ provider' = next
       /\ heldSince' = Note(next)
  \* A new revision.
  /\ rev' = rev + 1
  \* One change fewer to explore.
  /\ changes' = changes - 1
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, reachable, phase, sawOwed, seen, kept, snapshot,
                 joined, startRev, confirmed, confirmRev, atCommit, pending>>

\* The registration an earlier transaction owes settles. A provider that holds
\* the document already keeps its owner (contested); the record is cleared
\* either way, which bumps its version. The transaction's own record cannot
\* settle before it commits.
Settle ==
  \* Only while the transaction has not ended.
  /\ ~Ended
  \* Only an earlier transaction's record settles now.
  /\ owedBy = "earlier"
  \* Within the bound on explored changes.
  /\ changes > 0
  \* Nothing is owed any more.
  /\ owedBy' = "none"
  \* The record changes: clearing it is a write a validated read sees.
  /\ record' = record + 1
  \* An unregistered document becomes the creator's; a held one stays.
  /\ provider' = IF provider = "none" THEN "creator" ELSE provider
  \* Noted if a commit is under way.
  /\ heldSince' = Note(provider')
  \* A new revision.
  /\ rev' = rev + 1
  \* One change fewer to explore.
  /\ changes' = changes - 1
  \* Nothing else changes.
  /\ UNCHANGED <<reachable, phase, sawOwed, seen, kept, snapshot, joined, startRev,
                 confirmed, confirmRev, atCommit, pending>>

\* The provider goes away or comes back.
Flap ==
  \* Reachability flips.
  /\ reachable' = ~reachable
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, phase, sawOwed, seen, kept, snapshot,
                 joined, startRev, confirmed, confirmRev, heldSince, atCommit, changes,
                 pending>>

\* Whether a request reaches the transaction now: while it is open; with the
\* split-look bug, between its empty look and its commit; with the bugs that
\* let requests in after the commit began, during the round too.
Admitted ==
  \* An open transaction takes requests.
  \/ phase = "open"
  \* The split look leaves a gap a request can take.
  \/ phase = "plain"
  \* Only a bug lets a request into the round.
  \/ phase = "confirming" /\ Bug \in {"no_seal", "no_seal_snapshot"}

\* A request passes the caller's early check, which only turns a stranger
\* away without a hop: it is not yet on the transaction's owner.
EarlyCheck ==
  \* The transaction takes requests now.
  /\ Admitted
  \* One request on its way at a time is enough to show the race.
  /\ ~pending
  \* It is on its way.
  /\ pending' = TRUE
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, phase, sawOwed, seen, kept,
                 snapshot, joined, startRev, confirmed, confirmRev, heldSince, atCommit,
                 changes>>

\* The request reaches the owner, whose one lookup that picks the mailbox
\* decides admission again: a commit that began since refuses it. With the
\* stale-admission bug the early check is taken as the decision.
\* An admitted lookup: an owed record names its creator; otherwise the
\* provider's owner, read at the edge and kept.
Dispatch ==
  \* A request is on its way.
  /\ pending
  \* It is no longer on its way, whatever happens.
  /\ pending' = FALSE
  \* Admitted now (or by the bug), it runs; otherwise it finds no transaction.
  /\ IF Admitted \/ Bug = "stale_admission"
       THEN /\ IF owedBy # "none" /\ Bug # "owed_ignored"
                 \* Owed: the creator, from the record.
                 THEN /\ sawOwed' = TRUE
                      /\ seen' = record
                      /\ UNCHANGED kept
                 \* Otherwise the provider, if it answers, kept.
                 ELSE /\ reachable
                      /\ kept' = kept \cup {provider}
                      /\ UNCHANGED <<sawOwed, seen>>
            \* A request after the commit began joined it.
            /\ joined' = (joined \/ phase # "open")
       ELSE UNCHANGED <<sawOwed, seen, kept, joined>>
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, phase, snapshot, startRev,
                 confirmed, confirmRev, heldSince, atCommit, changes>>

\* The validated read of the owed record: it conflicts when the record
\* changed since the lookup read it.
RecordChanged == sawOwed /\ record # seen /\ Bug # "settle_unchecked"

\* The commit's look, on the transaction's core, in the same step as the
\* commit when nothing was kept. With the split-look bug, an empty look only
\* moves to "plain", and the commit is a later step a lookup can precede.
Look ==
  \* Only an open transaction begins a commit.
  /\ phase = "open"
  \* The floor of any confirmation is the head known now.
  /\ startRev' = rev
  \* The owners held since now start with the current one.
  /\ heldSince' = {provider}
  \* The edge gets what was kept.
  /\ snapshot' = kept
  \* Nothing kept: commit now (or, split, later); something kept: the round.
  /\ IF kept = {}
       THEN IF Bug = "split_look"
              THEN /\ phase' = "plain"
                   /\ UNCHANGED atCommit
              ELSE /\ phase' = IF RecordChanged THEN "conflict" ELSE "committed"
                   /\ atCommit' = provider
       ELSE /\ phase' = "confirming"
            /\ UNCHANGED atCommit
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, sawOwed, seen, kept, joined,
                 confirmed, confirmRev, changes, pending>>

\* The split-look bug's later commit: it commits without looking again.
PlainCommit ==
  \* Only after an empty look under the bug.
  /\ phase = "plain"
  \* The validated read decides.
  /\ phase' = IF RecordChanged THEN "conflict" ELSE "committed"
  \* The owner at the instant it lands.
  /\ atCommit' = provider
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, sawOwed, seen, kept, snapshot,
                 joined, startRev, confirmed, confirmRev, heldSince, changes, pending>>

\* The edge reads the answers again, outside the transaction, in one round.
\* Unreachable: refused, nothing commits.
Confirm ==
  \* Only in the round, and once.
  /\ phase = "confirming"
  \* Nothing was read yet.
  /\ confirmed = "unread"
  \* The no-confirm bug commits on what was kept, unread.
  /\ IF Bug = "no_confirm"
       THEN /\ phase' = IF RecordChanged THEN "conflict" ELSE "committed"
            /\ atCommit' = provider
            /\ UNCHANGED <<confirmed, confirmRev>>
       \* Up, or the bug that ignores an unreachable provider: read it now
       \* (the bug takes a kept answer as confirmed).
       ELSE IF reachable \/ Bug = "unreachable_commits"
              THEN /\ confirmed' = IF reachable THEN provider ELSE CHOOSE a \in snapshot: TRUE
                   /\ confirmRev' = rev
                   /\ UNCHANGED <<phase, atCommit>>
              \* Down: refused; the transaction stays for another commit.
              ELSE /\ phase' = "refused"
                   /\ UNCHANGED <<confirmed, confirmRev, atCommit>>
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, sawOwed, seen, kept, snapshot,
                 joined, startRev, heldSince, changes, pending>>

\* The answers the commit compares: every one kept, or, with the bug, only
\* those the look returned.
Compared == IF Bug = "no_seal_snapshot" THEN snapshot ELSE kept

\* The commit, where the transaction lives, after the round.
Commit ==
  \* Only in the round.
  /\ phase = "confirming"
  \* Only once the round read.
  /\ confirmed # "unread"
  \* A compared answer that is not the confirmed owner conflicts, as does a
  \* changed owed record.
  /\ phase' = IF RecordChanged \/ (\E a \in Compared: a # confirmed)
                THEN "conflict" ELSE "committed"
  \* The owner at the instant it lands.
  /\ atCommit' = provider
  \* Nothing else changes.
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, sawOwed, seen, kept, snapshot,
                 joined, startRev, confirmed, confirmRev, heldSince, changes, pending>>

\* Every step the system can take.
Next == RemoteChange \/ Settle \/ Flap \/ EarlyCheck \/ Dispatch \/ Look \/ PlainCommit \/ Confirm \/ Commit

\* From the initial state, any sequence of steps.
Spec == Init /\ [][Next]_vars

\* A committed provider answer was held by the provider at some revision since
\* the commit began.
INV_NoStaleOwner ==
  (phase = "committed" /\ kept # {}) => kept \subseteq heldSince

\* Every answer the transaction kept, one kept after the look included, was
\* confirmed as itself at a revision no older than the commit's start.
INV_NoUnconfirmedAnswer ==
  (phase = "committed" /\ kept # {}) =>
    /\ \A a \in kept: a = confirmed
    /\ confirmRev >= startRev

\* No request joined the transaction after its commit began.
INV_NothingJoinsCommit == ~joined

\* While a registration is owed, every lookup named its creator and none
\* asked the provider.
INV_OwedNamesCreator ==
  (owedBy # "none" /\ record = 0) => kept = {}

\* An owed creator commits only while the record it was read from is unchanged.
INV_OwedRecordHeld ==
  (phase = "committed" /\ sawOwed) => record = seen

\* The window, as a property the model must break: every kept answer is the
\* provider's owner at the instant the local commit lands.
WindowClosed ==
  (phase = "committed" /\ kept # {}) => \A a \in kept: a = atCommit
=============================================================================
