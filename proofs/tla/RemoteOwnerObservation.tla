------------------------ MODULE RemoteOwnerObservation ------------------------
\* The story, in plain words.
\*
\* An embedder holds a transaction on a Node and asks who owns a document,
\* then decides something in the same transaction and commits. The Node's
\* document ACP is a remote provider (vera.rs): it cannot join the
\* transaction, so its answers do not live in the transaction's view and
\* nothing in the storage engine sees them change.
\*
\* Which answer the Node gives:
\*   - a registration this transaction, or an earlier committed one, still
\*     owes the provider names its creator as the owner. That record is read
\*     from the transaction's own view as a validated point read. It is the
\*     same rule reads and verified writes apply to an owed registration: the
\*     creator reads and updates its document while the registration is owed.
\*     An owed creator is the Node's own claim, not the provider's word;
\*   - otherwise the provider's owner, read at the edge (outside the
\*     transaction, as a prepared input) and recorded in the transaction.
\*
\* What happens at commit:
\*   - a record the answer was read from changes before the commit (an
\*     earlier transaction's registration settles): the validated read
\*     conflicts and nothing commits. A run again takes the provider's answer;
\*   - the provider's owner changes (a transfer or an archive finalized on the
\*     provider): nothing local changes, so the commit confirms. When the
\*     transaction recorded provider answers, the edge reads every one of them
\*     again from the provider, outside the transaction, at or after the head
\*     known when the commit began, in one round. The commit then runs and
\*     fails with a conflict ("owner changed") when any answer differs. It
\*     commits only when every one is the same;
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
\*   - a commit on a provider answer without the confirmation round;
\*   - an owed registration is ignored: the answer comes from the provider
\*     while a registration is owed, reporting no owner (or another one) for a
\*     document its creator can already read;
\*   - a commit on an owed creator after the record it was read from changed;
\*   - a commit while the provider could not confirm.
\*
\* Rust owners: crates/engine/query/src/runner/owners.rs owner_job,
\* remote_owner; crates/engine/query/src/txn/observed_owners.rs ObservedOwners;
\* crates/engine/query/src/runner/owner_confirmation.rs commit_confirming;
\* crates/database/db/src/txn/registry/owned.rs OwnedTxn::commit_confirmed;
\* crates/engine/query/src/txn/context.rs AcpRegistrationProjection::owed_creator.
EXTENDS Naturals

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

\* The creator of the document on this Node, and somebody else.
Owners == {"creator", "other", "none"}

VARIABLES
  owedBy,     \* who owes the registration: "none", "txn" (this transaction) or "earlier"
  record,     \* version of the owed record; a settle bumps it
  provider,   \* the provider's current owner of the document
  rev,        \* the provider's revision; every finalized change bumps it
  reachable,  \* TRUE while the provider answers
  phase,      \* "open", "observed", "confirming", "confirmed", "committed", "conflict", "refused"
  source,     \* where the answer came from: "none", "owed" or "provider"
  answer,     \* the owner the observation gave
  seen,       \* the record version the observation read
  startRev,   \* the provider revision known when the commit began
  confirmed,  \* the owner the confirmation round read, "unread" before it
  confirmRev, \* the revision the confirmation read at
  heldSince,  \* every owner the provider held at some revision since the commit began
  atCommit,   \* the provider's owner at the instant the commit landed
  changes     \* provider changes left to explore (keeps the model finite)

vars == <<owedBy, record, provider, rev, reachable, phase, source, answer, seen,
          startRev, confirmed, confirmRev, heldSince, atCommit, changes>>

TypeOK ==
  /\ owedBy \in {"none", "txn", "earlier"}
  /\ record \in 0..2
  /\ provider \in Owners
  /\ rev \in 0..3
  /\ reachable \in BOOLEAN
  /\ phase \in {"open", "observed", "confirming", "confirmed", "committed", "conflict", "refused"}
  /\ source \in {"none", "owed", "provider"}
  /\ answer \in Owners \cup {"unasked"}
  /\ seen \in 0..2
  /\ startRev \in 0..3
  /\ confirmed \in Owners \cup {"unread"}
  /\ confirmRev \in 0..3
  /\ heldSince \subseteq Owners
  /\ atCommit \in Owners \cup {"unread"}
  /\ changes \in 0..2

\* The document is either already registered on the provider, owed by an
\* earlier committed transaction, or created in this transaction.
Init ==
  /\ \/ owedBy = "none" /\ provider \in {"creator", "other"}
     \/ owedBy = "earlier" /\ provider \in {"none", "other"}
     \/ owedBy = "txn" /\ provider \in {"none", "other"}
  /\ record = 0
  /\ rev = 0
  /\ reachable \in BOOLEAN
  /\ phase = "open"
  /\ source = "none"
  /\ answer = "unasked"
  /\ seen = 0
  /\ startRev = 0
  /\ confirmed = "unread"
  /\ confirmRev = 0
  /\ heldSince = {}
  /\ atCommit = "unread"
  /\ changes = 2

\* While the commit is under way every owner the provider takes is noted.
Note(owner) == IF phase \in {"confirming", "confirmed"} THEN heldSince \cup {owner} ELSE heldSince

\* What happens after the transaction ended decides nothing in it.
Ended == phase \in {"committed", "conflict", "refused"}

\* A transfer or an archive finalized on the provider, by anybody, at any time.
RemoteChange ==
  /\ ~Ended
  /\ changes > 0
  /\ \E next \in Owners \ {provider}:
       /\ provider' = next
       /\ heldSince' = Note(next)
  /\ rev' = rev + 1
  /\ changes' = changes - 1
  /\ UNCHANGED <<owedBy, record, reachable, phase, source, answer, seen, startRev,
                 confirmed, confirmRev, atCommit>>

\* The registration an earlier transaction owes settles. A provider that holds
\* the document already keeps its owner (contested); the record is cleared
\* either way, which bumps its version. The transaction's own record cannot
\* settle before it commits.
Settle ==
  /\ ~Ended
  /\ owedBy = "earlier"
  /\ changes > 0
  /\ owedBy' = "none"
  /\ record' = record + 1
  /\ provider' = IF provider = "none" THEN "creator" ELSE provider
  /\ heldSince' = Note(provider')
  /\ rev' = rev + 1
  /\ changes' = changes - 1
  /\ UNCHANGED <<reachable, phase, source, answer, seen, startRev, confirmed,
                 confirmRev, atCommit>>

\* The provider goes away or comes back.
Flap ==
  /\ reachable' = ~reachable
  /\ UNCHANGED <<owedBy, record, provider, rev, phase, source, answer, seen, startRev,
                 confirmed, confirmRev, heldSince, atCommit, changes>>

\* The observation: an owed record names its creator; otherwise the provider's
\* owner, read at the edge.
Observe ==
  /\ phase = "open"
  /\ IF owedBy # "none" /\ Bug # "owed_ignored"
       THEN /\ source' = "owed" /\ answer' = "creator"
       ELSE /\ reachable
            /\ source' = "provider" /\ answer' = provider
  /\ seen' = record
  /\ phase' = "observed"
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, startRev, confirmed,
                 confirmRev, heldSince, atCommit, changes>>

\* The commit begins: the head the Node knows now is the floor of the
\* confirmation.
BeginCommit ==
  /\ phase = "observed"
  /\ phase' = "confirming"
  /\ startRev' = rev
  /\ heldSince' = {provider}
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, source, answer, seen,
                 confirmed, confirmRev, atCommit, changes>>

\* The edge reads every recorded provider answer again, outside the
\* transaction. Unreachable: refused, nothing commits. A transaction that
\* recorded no provider answer reads nothing here: its look and its commit are
\* one step where it lives (commit_unless_told_as), so this action only passes
\* it on to Commit.
Confirm ==
  /\ phase = "confirming"
  /\ IF source = "provider" /\ Bug # "no_confirm"
       THEN IF reachable \/ Bug = "unreachable_commits"
              THEN /\ confirmed' = IF reachable THEN provider ELSE answer
                   /\ confirmRev' = rev
                   /\ phase' = "confirmed"
              ELSE /\ phase' = "refused"
                   /\ UNCHANGED <<confirmed, confirmRev>>
       ELSE /\ phase' = "confirmed"
            /\ UNCHANGED <<confirmed, confirmRev>>
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, source, answer, seen,
                 startRev, heldSince, atCommit, changes>>

\* The commit runs where the transaction lives: the validated read of the
\* owed record and the comparison with the confirmation.
Commit ==
  /\ phase = "confirmed"
  /\ LET recordChanged == source = "owed" /\ record # seen /\ Bug # "settle_unchecked"
         ownerChanged == source = "provider" /\ confirmed # "unread" /\ confirmed # answer
     IN phase' = IF recordChanged \/ ownerChanged THEN "conflict" ELSE "committed"
  /\ atCommit' = provider
  /\ UNCHANGED <<owedBy, record, provider, rev, reachable, source, answer, seen,
                 startRev, confirmed, confirmRev, heldSince, changes>>

Next == RemoteChange \/ Settle \/ Flap \/ Observe \/ BeginCommit \/ Confirm \/ Commit

Spec == Init /\ [][Next]_vars

\* A provider answer commits only if the provider held it at some revision
\* since the commit began.
INV_NoStaleOwner ==
  (phase = "committed" /\ source = "provider") => answer \in heldSince

\* A provider answer commits only on a confirmation equal to it, read at a
\* revision no older than the commit's start.
INV_ConfirmedAtCommit ==
  (phase = "committed" /\ source = "provider") =>
    /\ confirmed = answer
    /\ confirmRev >= startRev

\* While a registration is owed the observation names its creator.
INV_OwedNamesCreator ==
  (phase # "open" /\ seen = record /\ owedBy # "none") =>
    (source = "owed" /\ answer = "creator")

\* An owed creator commits only while the record it was read from is unchanged.
INV_OwedRecordHeld ==
  (phase = "committed" /\ source = "owed") => record = seen

\* The window, as a property the model must break: the answer is the
\* provider's owner at the instant the local commit lands.
WindowClosed ==
  (phase = "committed" /\ source = "provider") => answer = atCommit
=============================================================================
