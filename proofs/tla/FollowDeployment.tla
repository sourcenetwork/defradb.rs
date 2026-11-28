---- MODULE FollowDeployment ----
\* The story, in plain words.
\*
\* A DefraDB node keeps its access rules on a Vera deployment. It sends each
\* rule change (a grant, a revocation) to the deployment, and the deployment
\* confirms it in a finalized block. The node's own copy of a change it sent
\* is "owed" until the deployment confirms it; an owed revocation already
\* denies on this node, an owed grant does not grant yet.
\*
\* The deployment runs on several devices of one account, one active at a
\* time. When another device takes over without a handover, the blocks the
\* replaced device finalized above the takeover's cut are void: the new
\* active device never had them. Changes this node saw confirmed in those
\* blocks are gone from the deployment, although they verified. The height
\* the node follows goes DOWN: the new term goes on from the cut.
\*
\* The node notices the new term through its light client. Until it has
\* checked what the new term dropped, it decides nothing and sends nothing.
\* It keeps a bounded journal of the submissions it saw confirmed. It looks
\* up which of them the deployment still holds; the missing ones are always
\* the newest (a void is a cut through one chain). It sends each missing
\* request again, exactly as signed, in its order. The deployment executes it
\* again or refuses it (its caller's delegation may have expired meanwhile).
\* A refused one, and every later one on the same object, becomes owed
\* again, so it is sent with fresh authority after it and never overtakes
\* it. If the journal no longer holds every submission the term may have
\* voided, the node cannot know what was lost: it stops deciding ("fails
\* closed") and says so. How it then rebuilds from its own records and
\* reopens is FollowDeploymentRecovery.tla; here a closed node stays closed.
\*
\* A decision is cached under the revision it was read at. The cache key
\* carries the term the node last checked, so nothing cached before a term
\* change is served after it.
\*
\* The check is not one step. The node sends the missing requests again one
\* by one, and it can stop (crash, or lose the connection) part way. A
\* refused replay is still a request the deployment holds, so asking again
\* would no longer show it missing: before the first replay the node writes
\* down where its replay starts, and a check run again replays from there.
\* When the replay is done, what it left to owe again is recorded with the
\* checked term, and handed to the outbox in one more step; until it is
\* handed over, the node still decides nothing.
\*
\* What must never happen:
\*   * a decision served from a revision a term voided;
\*   * access wider than the node intends: once its newest change on the
\*     object is a revocation, no decision allows, also when that revocation
\*     was confirmed in a void block;
\*   * a checked term after which a change the node committed is neither held
\*     by the deployment nor owed, unless the node failed closed.
\* What must happen: every change owed again is settled, unless the node
\* failed closed.
\*
\* Code this models (defradb.rs, and the light client at the backbone rev
\* Cargo.lock pins):
\*   crates/engine/vera/src/vera_rs/follow/mod.rs         Follow, Follow::admit, Follow::still,
\*       Follow::epoch, reconcile, scan, uncovered
\*   crates/engine/vera/src/vera_rs/follow/journal.rs     Journal::record, Journal::floor,
\*       Journal::begin_replay, Journal::handled, Journal::replaying,
\*       MAX_CONFIRMATIONS, MAX_CONFIRMATION_BYTES
\*   crates/engine/vera/src/vera_rs/follow/lost.rs        lost_effects, tainted_by
\*   crates/engine/vera/src/vera_rs/provider/terms.rs     Follower::check_with (run on the task of a
\*       provider call; hands what is owed again to the outbox before it decides again)
\*   crates/engine/vera/src/vera_rs/provider.rs           decision_state (the term in the key),
\*       admitted, admitted_read (admitted before each read, and at the same checked
\*       term after it); provider/submission.rs (admitted again once the worker is held)
\*   crates/engine/acp/src/external/voided.rs             ExternalDocumentAcp::owe_again
\*   acp-light-client:src/term.rs                         certify, adopt (the head is dropped)
\*
\* Correspondence table (model -> Rust):
\*   chain, TakeOver      the deployment's chain of requests, each executed or refused;
\*                        a takeover keeps SubSeq(1, cut)
\*   sent, InSequence     the worker's signed requests; a replay sends the same bytes again,
\*                        so it keeps its request number (submission identifier); the chain
\*                        executes the worker's requests in sequence only
\*   Abort                a replay that cannot be confirmed ends at its deadline (ProviderChain)
\*   term, handled        the light client's certified term (HeaderChain::term_certificate);
\*                        the term the journal records as checked (Journal::handled_term)
\*   Gate, Open           Follow::admit: refuse while handled < certified term, while owed
\*                        changes are not handed over, and once closed
\*   journal, Keep        follow/journal.rs: the retained confirmations, MAX_CONFIRMATIONS
\*   evicted              Floor::Pruned, the newest pruned entry
\*   CheckWindow          uncovered: a floor the deployment does not hold fails closed
\*   failClosed           Journal::close and Journal::close_recoverable: the journal is
\*                        closed (Follow::admit refuses)
\*   Begin, Step, Finish  reconcile: scan (or resume at the mark), Journal::begin_replay,
\*                        one replay per Step, Journal::handled; the missing suffix is
\*                        proved in VeraConfirmationJournal.lean
\*   mark, Mark           Journal::replaying, written before the first replay
\*   cursor, lost, Crash  the replay's progress in memory, lost when the node stops
\*   pending, Deliver     Journal::undelivered and Follower::check_with handing it to the
\*                        outbox (owe_again); GateDelivery: Follow::admit refuses while
\*                        it is not empty, published with the term (Standing)
\*   KeepBatch            owe_again keeps every change of a tuple handed back together
\*   Restage, Taint       lost.rs: refused entries, and later entries of their object
\*   owed, Confirm        the outbox (#2954) and its settlement, sent by send_tx_with_sequence
\*                        or by the drainer's steps (owed_request.rs locked_worker), both
\*                        admitted only while Open; owed again first: in Rust a
\*                        change whose tuple already owes a later one is not owed again, so
\*                        the later one still decides the tuple (owe_again)
\*   cache, CacheByTerm   the decision memo (decisions.rs) keyed by decision_state, which
\*                        carries Follow::epoch
\*   Decide               VeraDocumentACP::check_doc_access through ExternalDocumentAcp
\*                        (an owed revocation denies)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    Keep,         \* MAX_CONFIRMATIONS: how many confirmations the journal retains
    MaxTakeovers, \* how many takeovers happen in a run (keeps the model finite)
    MaxCrashes,   \* how many times the node stops part way through a check
    Gate,         \* TRUE: nothing is decided or sent while a new term is unchecked
    GateDelivery, \* TRUE: nor while what a term voided is not handed to the outbox
    CacheByTerm,  \* TRUE: the cache key carries the checked term
    Restage,      \* TRUE: a refused replay is owed again
    Taint,        \* TRUE: entries after a refused one on its object are owed again too
    CheckWindow,  \* TRUE: a term that may have voided pruned entries fails closed
    Mark,         \* TRUE: where a replay starts is written down before it begins
    KeepBatch     \* TRUE: every change of the object handed back together is owed

\* The node's changes on one object, in order: it grants, then revokes.
Ops == <<"grant", "revoke">>

N == Len(Ops)   \* how many changes the node makes

VARIABLES
    staged,      \* how many of Ops the node committed so far
    owed,        \* changes waiting to be confirmed, in order (indices into Ops)
    pending,     \* changes a checked term left to owe again, not yet handed over
    sent,        \* every signed request so far: request number -> the change it makes
    chain,       \* the deployment's chain: requests, each executed (ok) or refused
    journal,     \* the requests the node saw confirmed and retains, oldest first
    evicted,     \* the newest request pruned from the journal, 0 if none
    term,        \* the deployment's term, as the light client certified it
    handled,     \* the term the node last checked
    failClosed,  \* TRUE once the node could not tell what a term voided
    mark,        \* where an unfinished replay starts (a request number), 0 if none; durable
    cursor,      \* the journal position the replay sends next, 0 when no replay runs
    checking,    \* the term the running replay checks (the one certified when it began)
    lost,        \* the requests this replay found lost so far, in order; in memory
    tainted,     \* TRUE once this replay found a lost change on the object; in memory
    crashes,     \* how many times the node stopped part way
    cache,       \* cached decisions: key, the deployment's answer, the chain it was read from
    takeovers,   \* how many takeovers happened
    servedVoid,  \* TRUE once a decision was served from a void revision
    servedWide   \* TRUE once a decision allowed while the node's intent revokes

vars == <<staged, owed, pending, sent, chain, journal, evicted, term, handled,
          failClosed, mark, cursor, checking, lost, tainted, crashes, cache, takeovers,
          servedVoid, servedWide>>

\* The most requests a run signs: each change once, and once more per takeover.
MaxSent == N * (MaxTakeovers + 1)

\* The set of the elements of a sequence.
Range(s) == {s[i] : i \in DOMAIN s}

\* Whether p is a prefix of s: the chain p was read from is still part of s.
IsPrefix(p, s) == Len(p) <= Len(s) /\ SubSeq(s, 1, Len(p)) = p

\* The requests the chain holds, executed or refused.
Held == {chain[i].r : i \in DOMAIN chain}

\* What the deployment says: the newest request it executed grants.
DeployAllows(c) ==
    LET done == SelectSeq(c, LAMBDA e : e.ok)          \* the executed requests
    IN Len(done) > 0 /\ Ops[sent[done[Len(done)].r]] = "grant"

\* An owed revocation denies on this node until it is settled.
OwedRevoke == \E i \in Range(owed) : Ops[i] = "revoke"

\* The node's intent: its newest committed change revokes.
IntentRevokes == staged > 0 /\ Ops[staged] = "revoke"

\* The decision cache key: the revision's height, with the checked term.
Key == <<IF CacheByTerm THEN handled ELSE 0, Len(chain)>>

\* The journal position of request r (0 if it is not there).
PosOf(r) == IF \E k \in DOMAIN journal : journal[k] = r
            THEN CHOOSE k \in DOMAIN journal : journal[k] = r ELSE 0

\* Whether the chain can take request r now: the worker's requests are
\* executed in sequence, so every earlier one must be held (executed or
\* refused) before it.
InSequence(r) == \A q \in 1..(r - 1) : q \in Held

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ staged \in 0..N                              \* a count of committed changes
    /\ owed \in Seq(1..N) /\ pending \in Seq(1..N)  \* change indices
    /\ sent \in Seq(1..N)                           \* each request makes one change
    /\ journal \in Seq(1..MaxSent)                  \* request numbers
    /\ evicted \in 0..MaxSent /\ mark \in 0..MaxSent  \* a request number or none
    /\ cursor \in Nat /\ lost \in Seq(1..MaxSent)   \* the replay's progress
    /\ checking \in Nat                             \* a term
    /\ tainted \in BOOLEAN /\ failClosed \in BOOLEAN  \* flags
    /\ term \in Nat /\ handled \in Nat              \* terms
    /\ takeovers \in 0..MaxTakeovers /\ crashes \in 0..MaxCrashes  \* counts
    /\ servedVoid \in BOOLEAN /\ servedWide \in BOOLEAN  \* history flags

\* At the start: nothing committed, one term, an empty chain and journal.
Init ==
    /\ staged = 0 /\ owed = <<>> /\ pending = <<>>  \* nothing committed or owed
    /\ sent = <<>> /\ chain = <<>> /\ journal = <<>>  \* nothing signed, held or journaled
    /\ evicted = 0 /\ mark = 0                      \* nothing pruned, no replay begun
    /\ term = 0 /\ handled = 0                      \* the first term, checked
    /\ failClosed = FALSE                           \* the node decides
    /\ cursor = 0 /\ lost = <<>> /\ tainted = FALSE /\ crashes = 0  \* no replay runs
    /\ checking = 0                                 \* no term being checked
    /\ cache = {} /\ takeovers = 0                  \* nothing cached, no takeover
    /\ servedVoid = FALSE /\ servedWide = FALSE     \* nothing served yet

\* Whether the node may decide and send now.
Open ==
    /\ ~failClosed                                              \* not closed
    /\ cursor = 0                                               \* no replay runs
    /\ (~Gate \/ (handled = term /\ (pending = <<>> \/ ~GateDelivery)))  \* the term is settled

\* The node commits its next change; until confirmed it is owed.
Stage ==
    /\ staged < N                               \* a change is left to make
    /\ staged' = staged + 1                     \* it is committed
    /\ owed' = Append(owed, staged + 1)         \* and owed, behind what is owed
    /\ UNCHANGED <<pending, sent, chain, journal, evicted, term, handled, failClosed,
                   mark, cursor, checking, lost, tainted, crashes, cache, takeovers,
                   servedVoid, servedWide>>     \* nothing else moves

\* The oldest owed change is signed as a new request, sent and confirmed. The
\* journal retains the request; when full it prunes its oldest entry and
\* remembers it.
Confirm ==
    /\ Open                                     \* the node may send
    /\ owed # <<>>                              \* something is owed
    /\ Len(sent) < MaxSent                      \* the run still signs (keeps it finite)
    /\ InSequence(Len(sent) + 1)                \* the chain takes the worker's next request
    /\ LET id == Len(sent) + 1                  \* the new request's number
       IN /\ sent' = Append(sent, Head(owed))   \* it makes the oldest owed change
          /\ chain' = Append(chain, [r |-> id, ok |-> TRUE])  \* the chain executes it
          /\ IF Len(journal) = Keep             \* the journal is full
             THEN /\ journal' = Append(Tail(journal), id)   \* prune the oldest, retain the new
                  /\ evicted' = Head(journal)               \* remember what was pruned
             ELSE /\ journal' = Append(journal, id)         \* retain the new
                  /\ evicted' = evicted                     \* nothing pruned
    /\ owed' = Tail(owed)                       \* it is no longer owed
    /\ UNCHANGED <<staged, pending, term, handled, failClosed, mark, cursor, checking, lost,
                   tainted, crashes, cache, takeovers, servedVoid, servedWide>>  \* nothing else moves

\* Another device takes over without a handover: the chain keeps only what
\* lies at or below the cut, so the followed height goes down. The term rises.
TakeOver ==
    /\ takeovers < MaxTakeovers                 \* the run still has a takeover
    /\ chain # <<>>                             \* something can be voided
    /\ \E cut \in 0..(Len(chain) - 1) :         \* the new device's newest block
        chain' = SubSeq(chain, 1, cut)          \* blocks above it are void
    /\ term' = term + 1                         \* the new term
    /\ takeovers' = takeovers + 1               \* one more takeover
    /\ UNCHANGED <<staged, owed, pending, sent, journal, evicted, handled, failClosed,
                   mark, cursor, checking, lost, tainted, crashes, cache, servedVoid,
                   servedWide>>                 \* the node has not looked yet

\* The node begins to check the new term. With a replay mark it resumes
\* there. Otherwise it scans: the journal's requests the chain no longer
\* holds are the newest ones. Nothing missing: the term is checked. All of
\* them missing and the newest pruned one not held: the node fails closed.
\* Else it writes the mark (with Mark) and starts replaying at the first
\* missing request.
Begin ==
    /\ handled < term /\ ~failClosed /\ cursor = 0   \* a term is unchecked, no replay runs
    /\ LET missing == {k \in DOMAIN journal : journal[k] \notin Held}  \* positions not held
           covered == evicted = 0 \/ evicted \in Held \/ ~CheckWindow   \* nothing pruned is void
           marked == IF Mark /\ mark # 0 THEN PosOf(mark) ELSE 0     \* an unfinished replay's start
           resume == marked # 0 /\ checking = term                   \* this term's, cut short
           \* the first missing position, or with an earlier term's replay
           \* unfinished, the earlier of that and its start
           starts == missing \cup (IF marked # 0 THEN {marked} ELSE {})
           from == IF resume THEN marked
                   ELSE IF starts = {} THEN 0
                   ELSE CHOOSE k \in starts : \A j \in starts : k <= j
       IN IF ~resume /\ missing = DOMAIN journal /\ ~covered
          THEN /\ failClosed' = TRUE                       \* what was lost is unknown
               /\ UNCHANGED <<handled, mark, cursor, checking, lost, tainted>>
          ELSE IF from = 0
          THEN /\ handled' = term                          \* nothing to send again
               /\ mark' = 0
               /\ UNCHANGED <<failClosed, cursor, checking, lost, tainted>>
          ELSE /\ mark' = IF Mark THEN journal[from] ELSE 0  \* written before any replay
               /\ cursor' = from                           \* the replay starts here
               /\ lost' = <<>>                             \* nothing found lost yet
               /\ tainted' = (pending # <<>>)              \* what still waits taints the object
               /\ checking' = term                        \* the term this replay checks
               /\ UNCHANGED <<handled, failClosed>>
    /\ UNCHANGED <<staged, owed, pending, sent, chain, journal, evicted, term, crashes,
                   cache, takeovers, servedVoid, servedWide>>  \* nothing else moves

\* The next journaled request is sent again, exactly as signed. A request
\* the chain already holds answers with what it came to; any other is
\* executed or refused, once every earlier one is held. A refused one, and
\* with Taint every later one on the object, is lost.
Step ==
    /\ cursor # 0 /\ cursor <= Len(journal)     \* a replay runs and has a request left
    /\ LET r == journal[cursor]                 \* the request sent again
           prior == {i \in DOMAIN chain : chain[i].r = r}  \* where the chain holds it
       IN /\ (prior # {} \/ InSequence(r))     \* answered, or takeable now
          /\ \E ok \in BOOLEAN :                \* executed again, or refused
            /\ IF prior # {}                    \* held already: its answer stands
               THEN /\ ok = chain[CHOOSE i \in prior : TRUE].ok
                    /\ UNCHANGED chain
               ELSE chain' = Append(chain, [r |-> r, ok |-> ok])  \* the chain takes it
            /\ lost' = IF ~ok \/ (Taint /\ tainted) THEN Append(lost, r) ELSE lost  \* lost?
            /\ tainted' = (tainted \/ ~ok)      \* a refusal taints what follows
    /\ cursor' = cursor + 1                     \* on to the next
    /\ UNCHANGED <<staged, owed, pending, sent, journal, evicted, term, handled,
                   failClosed, mark, checking, crashes, cache, takeovers, servedVoid,
                   servedWide>>                 \* nothing else moves

\* The replay is done: what it found lost is recorded with the checked term
\* and the mark is cleared. Lost requests stay in the journal: they hold
\* their place in the worker's sequence, and a later takeover that voids
\* them has them sent again before anything after them.
Finish ==
    /\ cursor # 0 /\ cursor > Len(journal)      \* every request was sent again
    /\ pending' = pending \o (IF Restage THEN [k \in DOMAIN lost |-> sent[lost[k]]] ELSE <<>>)
    /\ handled' = checking /\ mark' = 0         \* the term it began for is checked
    /\ cursor' = 0 /\ lost' = <<>> /\ tainted' = FALSE  \* the replay ends
    /\ UNCHANGED <<staged, owed, sent, chain, journal, evicted, term, failClosed,
                   checking, crashes, cache, takeovers, servedVoid, servedWide>>  \* nothing else moves

\* A replay whose next request the chain cannot take (a later takeover
\* voided an earlier one) waits for its receipt until its deadline and
\* ends with an error, as a stop does: the check is run again.
Abort ==
    /\ cursor # 0 /\ cursor <= Len(journal)     \* a replay runs and has a request left
    /\ ~(journal[cursor] \in Held \/ InSequence(journal[cursor]))  \* it cannot go on
    /\ cursor' = 0 /\ lost' = <<>> /\ tainted' = FALSE  \* its progress is lost
    /\ UNCHANGED <<staged, owed, pending, sent, chain, journal, evicted, term, handled,
                   failClosed, mark, checking, crashes, cache, takeovers, servedVoid,
                   servedWide>>                 \* nothing else moves

\* The node stops part way through a replay: what it held in memory is
\* gone; the chain, the journal and the mark stay.
Crash ==
    /\ cursor # 0 /\ crashes < MaxCrashes       \* a replay runs; the run may still crash
    /\ cursor' = 0 /\ lost' = <<>> /\ tainted' = FALSE  \* its progress is lost
    /\ crashes' = crashes + 1                   \* one more stop
    /\ UNCHANGED <<staged, owed, pending, sent, chain, journal, evicted, term, handled,
                   failClosed, mark, checking, cache, takeovers, servedVoid,
                   servedWide>>                 \* nothing else moves

\* What a checked term left to owe again is handed to the outbox, before
\* whatever is owed already. With KeepBatch every change of the object handed
\* back together is owed; without, only the first.
Deliver ==
    /\ pending # <<>> /\ handled = term /\ cursor = 0  \* something waits, the term is checked
    /\ owed' = (IF KeepBatch THEN pending ELSE <<Head(pending)>>) \o owed  \* owed again first
    /\ pending' = <<>>                          \* handed over
    /\ UNCHANGED <<staged, sent, chain, journal, evicted, term, handled, failClosed,
                   mark, cursor, checking, lost, tainted, crashes, cache, takeovers, servedVoid,
                   servedWide>>                 \* nothing else moves

\* The node decides. A cached answer under the current key is reused;
\* otherwise the deployment is asked at its head and the answer cached. An
\* owed revocation denies whatever the deployment says.
Decide ==
    /\ Open                                                   \* the node may decide
    /\ IF \E e \in cache : e.k = Key                          \* a cached answer under this key
       THEN \E e \in cache :
              /\ e.k = Key                                    \* that answer
              /\ servedVoid' = (servedVoid \/ ~IsPrefix(e.basis, chain))  \* read from a void revision?
              /\ servedWide' = (servedWide \/ (e.v /\ ~OwedRevoke /\ IntentRevokes))  \* wider than intended?
              /\ UNCHANGED cache                              \* the cache stays
       ELSE /\ cache' = cache \cup {[k |-> Key, v |-> DeployAllows(chain), basis |-> chain]}  \* read and cache
            /\ servedVoid' = servedVoid                       \* read at the live head
            /\ servedWide' = (servedWide \/ (DeployAllows(chain) /\ ~OwedRevoke /\ IntentRevokes))  \* wider?
    /\ UNCHANGED <<staged, owed, pending, sent, chain, journal, evicted, term, handled,
                   failClosed, mark, cursor, checking, lost, tainted, crashes, takeovers>>  \* nothing else moves

\* A node that failed closed does nothing more; the run stutters.
Stopped ==
    /\ failClosed          \* the node refuses everything
    /\ UNCHANGED vars      \* so nothing moves

\* One of these happens at each step.
Next == Stage \/ Confirm \/ TakeOver \/ Begin \/ Step \/ Finish \/ Abort \/ Crash \/ Deliver
        \/ Decide \/ Stopped

\* The node keeps checking terms, handing over and sending what is owed.
Spec == Init /\ [][Next]_vars
        /\ WF_vars(Begin) /\ WF_vars(Step) /\ WF_vars(Finish) /\ WF_vars(Abort)
        /\ WF_vars(Deliver)
        /\ WF_vars(Confirm)

\* No decision is served from a revision a term voided.
INV_NoVoidDecision == ~servedVoid

\* Access is never wider than intended: once the node's newest change
\* revokes, nothing allows, also when the revocation was confirmed in a void block.
INV_RevocationDenies == ~servedWide

\* After a checked term every change the node committed is executed on the
\* chain, owed, or waiting to be owed again, unless the node failed closed:
\* nothing a term voided is silently taken as kept.
INV_NothingLost ==
    (handled = term /\ ~failClosed /\ cursor = 0) =>
        \A i \in 1..staged :
            \/ \E k \in DOMAIN chain : chain[k].ok /\ sent[chain[k].r] = i
            \/ i \in Range(owed) \/ i \in Range(pending)

\* Every change owed again is settled, unless the node failed closed.
LIVE_ResubmittedSettles ==
    <>[](failClosed \/ (owed = <<>> /\ pending = <<>> /\ handled = term))
====
