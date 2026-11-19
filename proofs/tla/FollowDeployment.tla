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
\* voided, the node cannot know what was lost: it stops deciding for good
\* ("fails closed") and says so.
\*
\* A decision is cached under the revision it was read at. The cache key
\* carries the term the node last checked, so nothing cached before a term
\* change is served after it.
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
\* Code this models (defradb.rs, light client at backbone rev a853cc6):
\*   crates/engine/vera/src/vera_rs/follow/mod.rs         Follow, Follow::admit, Follow::epoch,
\*       reconcile, uncovered
\*   crates/engine/vera/src/vera_rs/follow/journal.rs     Journal::record, Journal::floor,
\*       MAX_CONFIRMATIONS, MAX_CONFIRMATION_BYTES
\*   crates/engine/vera/src/vera_rs/follow/lost.rs        lost_effects, tainted_by
\*   crates/engine/vera/src/vera_rs/provider/terms.rs     Follower::check (hands what is owed
\*       again to the outbox before the provider decides again)
\*   crates/engine/vera/src/vera_rs/provider.rs           decision_state (the term in the key),
\*       admitted_read (admitted before and after each read)
\*   crates/engine/acp/src/external/voided.rs             ExternalDocumentAcp::owe_again
\*   acp-light-client src/term.rs                         certify, adopt (the head is dropped)
\*
\* Correspondence table (model -> Rust):
\*   history, TakeOver    the deployment's chain; a takeover keeps SubSeq(1, cut)
\*   sent                 the worker's signed requests; a replay sends the same bytes again,
\*                        so it keeps its request number (submission identifier)
\*   term, handled        the light client's certified term (HeaderChain::term_certificate);
\*                        the term the journal records as checked (Journal::handled_term)
\*   Gate, Open           Follow::admit: refuse while handled < certified term, while owed
\*                        changes are not handed over, and once closed
\*   journal, Keep        follow/journal.rs: the retained confirmations, MAX_CONFIRMATIONS
\*   evicted              Floor::Pruned, the newest pruned entry
\*   CheckWindow          uncovered: a floor the deployment does not hold fails closed
\*   Reconcile            reconcile: replay the missing suffix in order, owe again the lost
\*                        (the missing suffix is proved in VeraConfirmationJournal.lean)
\*   Restage, Taint       lost.rs: refused entries, and later entries of their object
\*   owed, Confirm        the outbox (#2954) and its settlement; owed again first: in Rust a
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
    Gate,         \* TRUE: nothing is decided or sent while a new term is unchecked
    CacheByTerm,  \* TRUE: the cache key carries the checked term
    Restage,      \* TRUE: a refused replay is owed again
    Taint,        \* TRUE: entries after a refused one on its object are owed again too
    CheckWindow   \* TRUE: a term that may have voided pruned entries fails closed

\* The node's changes on one object, in order: it grants, then revokes.
Ops == <<"grant", "revoke">>

N == Len(Ops)   \* how many changes the node makes

VARIABLES
    staged,      \* how many of Ops the node committed so far
    owed,        \* changes waiting to be confirmed, in order (indices into Ops)
    sent,        \* every signed request so far: request number -> the change it makes
    history,     \* the deployment's chain: the requests it executed, in order
    journal,     \* the requests the node saw confirmed and retains, oldest first
    evicted,     \* the newest request pruned from the journal, 0 if none
    term,        \* the deployment's term, as the light client certified it
    handled,     \* the term the node last checked
    failClosed,  \* TRUE once the node could not tell what a term voided
    cache,       \* cached decisions: key, the deployment's answer, the chain it was read from
    takeovers,   \* how many takeovers happened
    servedVoid,  \* TRUE once a decision was served from a void revision
    servedWide   \* TRUE once a decision allowed while the node's intent revokes

vars == <<staged, owed, sent, history, journal, evicted, term, handled, failClosed,
          cache, takeovers, servedVoid, servedWide>>

\* The most requests a run signs: each change once, and once more per takeover.
MaxSent == N * (MaxTakeovers + 1)

\* The set of the elements of a sequence.
Range(s) == {s[i] : i \in DOMAIN s}

\* Whether p is a prefix of s: the chain p was read from is still part of s.
IsPrefix(p, s) == Len(p) <= Len(s) /\ SubSeq(s, 1, Len(p)) = p

\* What the deployment says: the newest request it executed grants.
DeployAllows(h) == Len(h) > 0 /\ Ops[sent[h[Len(h)]]] = "grant"

\* An owed revocation denies on this node until it is settled.
OwedRevoke == \E i \in Range(owed) : Ops[i] = "revoke"

\* The node's intent: its newest committed change revokes.
IntentRevokes == staged > 0 /\ Ops[staged] = "revoke"

\* The decision cache key: the revision's height, with the checked term.
Key == <<IF CacheByTerm THEN handled ELSE 0, Len(history)>>

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ staged \in 0..N                              \* a count of committed changes
    /\ owed \in Seq(1..N)                           \* change indices
    /\ sent \in Seq(1..N)                           \* each request makes one change
    /\ history \in Seq(1..MaxSent)                  \* request numbers
    /\ journal \in Seq(1..MaxSent)                  \* request numbers
    /\ evicted \in 0..MaxSent                       \* a request number or none
    /\ term \in Nat /\ handled \in Nat              \* terms
    /\ failClosed \in BOOLEAN                       \* a flag
    /\ takeovers \in 0..MaxTakeovers                \* a count
    /\ servedVoid \in BOOLEAN /\ servedWide \in BOOLEAN  \* history flags

\* At the start: nothing committed, one term, an empty chain and journal.
Init ==
    /\ staged = 0          \* no change yet
    /\ owed = <<>>         \* nothing owed
    /\ sent = <<>>         \* nothing signed
    /\ history = <<>>      \* the chain holds nothing
    /\ journal = <<>>      \* nothing confirmed
    /\ evicted = 0         \* nothing pruned
    /\ term = 0            \* the first term
    /\ handled = 0         \* and it is checked
    /\ failClosed = FALSE  \* the node decides
    /\ cache = {}          \* nothing cached
    /\ takeovers = 0       \* no takeover yet
    /\ servedVoid = FALSE  \* nothing served yet
    /\ servedWide = FALSE  \* nothing served yet

\* Whether the node may decide and send now.
Open == ~failClosed /\ (handled = term \/ ~Gate)

\* The node commits its next change; until confirmed it is owed.
Stage ==
    /\ staged < N                               \* a change is left to make
    /\ staged' = staged + 1                     \* it is committed
    /\ owed' = Append(owed, staged + 1)         \* and owed, behind what is owed
    /\ UNCHANGED <<sent, history, journal, evicted, term, handled, failClosed,
                   cache, takeovers, servedVoid, servedWide>>  \* nothing else moves

\* The oldest owed change is signed as a new request, sent and confirmed. The
\* journal retains the request; when full it prunes its oldest entry and
\* remembers it.
Confirm ==
    /\ Open                                     \* the node may send
    /\ owed # <<>>                              \* something is owed
    /\ LET id == Len(sent) + 1                  \* the new request's number
       IN /\ sent' = Append(sent, Head(owed))   \* it makes the oldest owed change
          /\ history' = Append(history, id)     \* the chain executes it
          /\ IF Len(journal) = Keep             \* the journal is full
             THEN /\ journal' = Append(Tail(journal), id)   \* prune the oldest, retain the new
                  /\ evicted' = Head(journal)               \* remember what was pruned
             ELSE /\ journal' = Append(journal, id)         \* retain the new
                  /\ evicted' = evicted                     \* nothing pruned
    /\ owed' = Tail(owed)                       \* it is no longer owed
    /\ UNCHANGED <<staged, term, handled, failClosed, cache, takeovers,
                   servedVoid, servedWide>>     \* nothing else moves

\* Another device takes over without a handover: the chain keeps only what
\* lies at or below the cut, so the followed height goes down. The term rises.
TakeOver ==
    /\ takeovers < MaxTakeovers                 \* the run still has a takeover
    /\ history # <<>>                           \* something can be voided
    /\ \E cut \in 0..(Len(history) - 1) :       \* the new device's newest block
        history' = SubSeq(history, 1, cut)      \* blocks above it are void
    /\ term' = term + 1                         \* the new term
    /\ takeovers' = takeovers + 1               \* one more takeover
    /\ UNCHANGED <<staged, owed, sent, journal, evicted, handled, failClosed, cache,
                   servedVoid, servedWide>>     \* the node has not looked yet

\* The node checks the new term. The journal's requests the chain no longer
\* holds are the newest ones. If a pruned request may be among the void ones
\* (the newest pruned one is not held), the node fails closed. Otherwise it
\* sends each missing request again, exactly as signed and in order; some are
\* refused. A refused one, and with Taint every later one on the object, is
\* owed again before whatever is owed already.
Reconcile ==
    /\ handled < term                           \* a term is unchecked
    /\ ~failClosed                              \* the node still decides
    /\ LET missing == SelectSeq(journal, LAMBDA r : r \notin Range(history))  \* the void suffix
           covered == evicted = 0 \/ evicted \in Range(history) \/ ~CheckWindow   \* nothing pruned is void
       IN IF covered
          THEN \E refused \in SUBSET Range(missing) :      \* the replays the deployment refuses
                LET first == {k \in DOMAIN missing : missing[k] \in refused}  \* positions refused
                    \* owed again: refused, or (with Taint) after a refused one on the object
                    lost(k) == missing[k] \in refused \/ (Taint /\ \E j \in first : j < k)
                    lostReqs == {missing[k] : k \in {k \in DOMAIN missing : lost(k)}}  \* their requests
                    applied == SelectSeq(missing, LAMBDA r : r \notin refused)  \* executed again
                    owedAgain == [k \in 1..Len(SelectSeq(missing, LAMBDA r : r \in lostReqs)) |->
                                    sent[SelectSeq(missing, LAMBDA r : r \in lostReqs)[k]]]  \* their changes, in order
                IN /\ history' = history \o applied                  \* replays the chain holds
                   /\ owed' = (IF Restage THEN owedAgain ELSE <<>>) \o owed  \* owed again first
                   /\ journal' = SelectSeq(journal, LAMBDA r : r \notin lostReqs)  \* journaled again once settled
                   /\ handled' = term                                \* the term is checked
                   /\ UNCHANGED failClosed                           \* the node decides
          ELSE /\ failClosed' = TRUE                               \* what was lost is unknown
               /\ UNCHANGED <<history, owed, journal, handled>>    \* nothing is sent
    /\ UNCHANGED <<staged, sent, evicted, term, cache, takeovers, servedVoid, servedWide>>  \* nothing else moves

\* The node decides. A cached answer under the current key is reused;
\* otherwise the deployment is asked at its head and the answer cached. An
\* owed revocation denies whatever the deployment says.
Decide ==
    /\ Open                                                   \* the node may decide
    /\ IF \E e \in cache : e.k = Key                          \* a cached answer under this key
       THEN \E e \in cache :
              /\ e.k = Key                                    \* that answer
              /\ servedVoid' = (servedVoid \/ ~IsPrefix(e.basis, history))  \* read from a void revision?
              /\ servedWide' = (servedWide \/ (e.v /\ ~OwedRevoke /\ IntentRevokes))  \* wider than intended?
              /\ UNCHANGED cache                              \* the cache stays
       ELSE /\ cache' = cache \cup {[k |-> Key, v |-> DeployAllows(history), basis |-> history]}  \* read and cache
            /\ servedVoid' = servedVoid                       \* read at the live head
            /\ servedWide' = (servedWide \/ (DeployAllows(history) /\ ~OwedRevoke /\ IntentRevokes))  \* wider?
    /\ UNCHANGED <<staged, owed, sent, history, journal, evicted, term, handled, failClosed, takeovers>>  \* nothing else moves

\* A node that failed closed does nothing more; the run stutters.
Stopped ==
    /\ failClosed          \* the node refuses everything
    /\ UNCHANGED vars      \* so nothing moves

Next == Stage \/ Confirm \/ TakeOver \/ Reconcile \/ Decide \/ Stopped  \* one of these happens at each step

\* The node keeps checking terms and sending what is owed.
Spec == Init /\ [][Next]_vars /\ WF_vars(Reconcile) /\ WF_vars(Confirm)

\* No decision is served from a revision a term voided.
INV_NoVoidDecision == ~servedVoid

\* Access is never wider than intended: once the node's newest change
\* revokes, nothing allows, also when the revocation was confirmed in a void block.
INV_RevocationDenies == ~servedWide

\* After a checked term every change the node committed is held by the chain
\* or owed again, unless the node failed closed: nothing a term voided is
\* silently taken as kept.
INV_NothingLost ==
    (handled = term /\ ~failClosed) =>
        \A i \in 1..staged : (\E r \in Range(history) : sent[r] = i) \/ i \in Range(owed)

\* Every change owed again is settled, unless the node failed closed.
LIVE_ResubmittedSettles == <>[](failClosed \/ (owed = <<>> /\ handled = term))
====
