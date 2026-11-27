---- MODULE DocSyncCompletion ----
\* The story, in plain words.
\*
\* A node asks its connected peers for some documents (an explicit document
\* sync: sync_documents in crates/surfaces/p2p-adapter/src/doc_sync/sync.rs,
\* which iroh's IrohP2PAdapter::sync_documents and libp2p's fallback path call).
\* Each peer that holds a head of an asked document the node lacks sends it,
\* and the node merges it. Every merge is announced on the node's event bus as
\* a merge report (MergeComplete) naming the document. The sync listens to
\* those reports and stops waiting once its completion rule says it has
\* everything, or when one of its timeouts fires.
\*
\* Reports are not one per document. The merge handler reports a merge when it
\* commits (crates/database/db/src/merge/merge_handler/composite.rs), and the
\* replication loop reports the same merge again, and reports a head it already
\* held as merged when a second peer sends it
\* (publish_replication_result in crates/surfaces/p2p-adapter/src/replication_events.rs).
\* Reports for documents the sync did not ask for arrive on the same bus.
\* A composite can also be terminally refused, as when a remote update changes
\* an immutable field. A refusal that did not apply anything is not a merge
\* report: before treating a report as completion, the merge handler checks
\* that the terminal skip means the node already holds the block
\* (MergeOutcome::skip_reason_is_applied in the composite merge handler).
\* Reject below models the old false report, and RejectSpec gives a RED run
\* showing why INV_CompleteMeansEveryDocMerged must keep that report out.
\*
\* The sync returns in one of two ways. It returns early, before any
\* timeout, when its completion rule says it has everything ("complete").
\* Or a timeout ends it ("timedout"): its idle timeout (3 s without a report)
\* in each of its rounds, or the caller's overall deadline. Either way the
\* caller is told Ok, except that a transport whose send confirms a reply
\* (iroh) says "timeout" when a peer never answered and nothing asked for
\* merged. So Ok does not mean every document merged: after a timeout it
\* means only that the sync stopped waiting. The fetches the replies start
\* are best effort and a failed one is not driven again (the "Reply-driven
\* DAG fetch" family); the caller owns completion and reads back what it
\* needs. That is the contract before and after the fix, and the model
\* states it as it is.
\*
\* A peer may also answer with heads of an asked document that the node
\* already holds and has merged: nothing is fetched or merged for it, so no
\* merge would ever report it. The reply handler reports such a document
\* itself, once every head the peer advertised for it is merged here and the
\* local document-head lookup confirms each head belongs to that document
\* (handle_doc_sync_reply in
\* crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs sends
\* SyncEvent::HeadsAlreadyMerged, which the replication loop turns into the
\* same "already merged" report). A head paired with another document's ID is
\* ignored, so a peer cannot complete one document with another's head.
\*
\* What must never happen:
\*   1. the sync returns early while a document it asked for has not
\*      merged - INV_CompleteMeansEveryDocMerged;
\*   2. the sync keeps waiting after it has read a report for every document
\*      it asked for - INV_DoneOnceEveryDocReported;
\*   3. the sync keeps waiting after every peer's answer for every asked
\*      document was handled and every report was read, when each asked
\*      document was in some peer's answer - INV_NoWaitOnceEveryAnswerHandled.
\* What may happen, shown by a witness configuration: a timeout ends the sync
\* with documents missing and the caller is told Ok (INV_OkMeansEveryDocMerged
\* is not promised).
\*
\* The sync sends its requests in up to three rounds; a round that ends on
\* its idle timeout sends them again. Here the rounds are one wait: sending
\* again only lets more reports arrive, which the model allows at any time.
\*
\* What the sync does not promise, shown by a witness configuration: when two
\* peers hold different heads of one document, the sync may complete after the
\* first merges and before the second arrives (INV_EveryPeersHeadMerged).
\*
\* The Rust code each guard models:
\*   CountReports  FALSE: the sync keeps the set of asked documents it read a
\*                 report for (`merged`) and is complete when that set holds
\*                 every asked document. TRUE: the code before the fix, which
\*                 counted reports and was complete at peers x documents;
\*   FilterAsked   TRUE: a report for a document not asked for is ignored
\*                 (`doc_set.contains(&data.doc_id)`);
\*   ReportHeld    TRUE: a reply whose every head of a document is already
\*                 merged here reports the document (HeadsAlreadyMerged).
\*                 FALSE: the code before, which skipped such heads and
\*                 reported nothing, so the sync waited out its timeouts.
\*   Reject        models a terminal refusal that is reported without merging;
\*                 RejectSpec includes it, while the fixed Spec does not.

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Docs,          \* every document a report can name, asked for or not
  Peers,         \* the connected peers the request went to
  MaxReports,    \* how many reports the run may publish, to keep it finite
  CountReports,  \* TRUE: the code before the fix, counting reports
  FilterAsked,   \* TRUE: reports for documents not asked for are ignored
  HeldDocs,      \* the documents a peer may answer for with heads the node already merged
  ReportHeld     \* TRUE: a document already merged here is reported when a peer's answer names it

VARIABLES
  asked,      \* the documents the sync asked for (fixed for the run)
  holds,      \* holds[p]: the documents peer p has a head of that the node lacks (fixed)
  current,    \* current[p]: the documents peer p answers for with heads the node has all merged (fixed)
  answered,   \* the pairs <<p, d>>: peer p's answer for d, all heads already merged here, was handled
  arrived,    \* the pairs <<p, d>>: peer p's head of document d reached the node and merged
  merged,     \* the documents that really merged on the node
  queue,      \* reports published and not read by the sync yet, oldest first
  published,  \* how many reports were published so far
  seen,       \* the sync's set of documents it read a report for and kept
  count,      \* the sync's count of reports it read and kept (the code before)
  state,      \* "waiting", "complete" (its rule said done, before any timeout) or "timedout"
  result      \* what the caller is told: "none" yet, "ok" or "error" (the timeout error)

vars == <<asked, holds, current, answered, arrived, merged, queue, published, seen, count, state, result>>  \* every variable

\* The sync's completion rule: the fix compares the set of documents it read
\* reports for with the set it asked for; the code before compared a count of
\* reports with the number of peers times the number of documents.
Done ==
  IF CountReports
  THEN count >= Cardinality(Peers) * Cardinality(asked)  \* the code before
  ELSE Cardinality(seen) >= Cardinality(asked)            \* the fix: merged.len() >= doc_set.len()

Init ==                                   \* at the start:
  /\ asked \in SUBSET Docs                \* any set of documents is asked for, the empty one too
  /\ holds \in [Peers -> SUBSET Docs]     \* each peer holds some heads the node lacks
  /\ current \in [Peers -> SUBSET HeldDocs]  \* and answers for some documents with heads the node merged
  /\ \A p \in Peers : holds[p] \cap current[p] = {}  \* a document with a head still to send is not "all merged"
  /\ answered = {}                        \* no such answer handled yet
  /\ arrived = {}                         \* nothing has arrived
  /\ merged = UNION {current[p] : p \in Peers}  \* only the documents already merged here have merged
  /\ queue = <<>>                         \* no report yet
  /\ published = 0                        \* none published
  /\ seen = {}                            \* the sync has read nothing
  /\ count = 0                            \* nor counted anything
  /\ state = "waiting"                    \* the sync waits
  /\ result = "none"                      \* and has told the caller nothing

\* Peer p's head of document d arrives and merges; the merge handler reports it.
Merge(p, d) ==
  /\ d \in holds[p]                       \* the peer holds a head the node lacks
  /\ <<p, d>> \notin arrived              \* and has not sent it yet
  /\ published < MaxReports               \* within the run's bound
  /\ arrived' = arrived \cup {<<p, d>>}   \* it arrived
  /\ merged' = merged \cup {d}            \* the document merged
  /\ queue' = Append(queue, d)            \* its report is published
  /\ published' = published + 1           \* one more report
  /\ UNCHANGED <<asked, holds, current, answered, seen, count, state, result>>  \* the sync has not read it yet

\* Peer p's answer names document d, and every head it names is merged here
\* already: nothing is fetched or merged. With the fix the reply handler
\* reports d; the code before reported nothing.
AnswerHeld(p, d) ==
  /\ d \in current[p]                     \* every head p names for d is merged here
  /\ <<p, d>> \notin answered             \* and this answer was not handled yet
  /\ published < MaxReports               \* within the run's bound
  /\ answered' = answered \cup {<<p, d>>} \* the answer is handled
  /\ queue' = IF ReportHeld THEN Append(queue, d) ELSE queue  \* MUTATION report_held: no report
  /\ published' = IF ReportHeld THEN published + 1 ELSE published  \* one more report, if any
  /\ UNCHANGED <<asked, holds, current, arrived, merged, seen, count, state, result>>  \* nothing merged

\* A merge already reported is reported again: the replication loop's own
\* report of it, or a second peer's copy of a head the node already holds.
Echo(d) ==
  /\ d \in merged                         \* only a document that merged
  /\ published < MaxReports               \* within the run's bound
  /\ queue' = Append(queue, d)            \* another report of it
  /\ published' = published + 1           \* one more report
  /\ UNCHANGED <<asked, holds, current, answered, arrived, merged, seen, count, state, result>>  \* nothing else happened

\* Peer p's head of document d is terminally refused, and the old merge handler
\* reports it anyway. Nothing merged, but the report can still complete the sync.
Reject(p, d) ==
  /\ d \in holds[p]                       \* the peer holds a head the node lacks
  /\ <<p, d>> \notin arrived              \* that head has not merged
  /\ published < MaxReports               \* within the run's bound
  /\ queue' = Append(queue, d)            \* the false report is published
  /\ published' = published + 1           \* one more report
  /\ UNCHANGED <<asked, holds, current, answered, arrived, merged, seen, count, state, result>>  \* nothing merged

\* The sync reads the oldest report while its rule does not say done.
Read ==
  /\ state = "waiting"                    \* still waiting
  /\ ~Done                                \* the loop runs only until the rule holds
  /\ queue # <<>>                         \* a report is there
  /\ LET d == Head(queue)                 \* the document it names
         keep == ~FilterAsked \/ d \in asked  \* MUTATION filter_asked: every report is kept
     IN /\ seen' = IF keep THEN seen \cup {d} ELSE seen  \* the fix: the document joins the set
        /\ count' = IF keep THEN count + 1 ELSE count     \* the code before: one more report counted
  /\ queue' = Tail(queue)                 \* read
  /\ UNCHANGED <<asked, holds, current, answered, arrived, merged, published, state, result>>  \* the rest stays

\* The rule says done: the sync stops waiting and returns.
Finish ==
  /\ state = "waiting"                    \* still waiting
  /\ Done                                 \* the rule holds (at once when nothing was asked)
  /\ state' = "complete"                  \* complete
  /\ result' = "ok"                       \* the caller is told Ok
  /\ UNCHANGED <<asked, holds, current, answered, arrived, merged, queue, published, seen, count>>  \* the rest stays

\* The idle timeout of its last round or the overall timeout fires, at any moment.
Timeout ==
  /\ state = "waiting"                    \* still waiting
  /\ state' = "timedout"                  \* it stops waiting
  /\ result' \in IF seen \cap asked = {}  \* nothing asked for merged: on iroh, with a peer
                 THEN {"ok", "error"}     \* that never answered, the timeout error; else Ok
                 ELSE {"ok"}              \* something merged: Ok, with or without the rest
  /\ UNCHANGED <<asked, holds, current, answered, arrived, merged, queue, published, seen, count>>  \* the rest stays

Next ==                                   \* any of these, in any order:
  \/ \E p \in Peers, d \in Docs : Merge(p, d)  \* a head arrives
  \/ \E p \in Peers, d \in Docs : AnswerHeld(p, d)  \* an answer names only heads merged here
  \/ \E d \in Docs : Echo(d)              \* a merge is reported again
  \/ Read                                 \* the sync reads a report
  \/ Finish                               \* the sync completes
  \/ Timeout                              \* the sync times out

Spec == Init /\ [][Next]_vars             \* every behaviour

\* The old merge handler's behaviour, used only by the RED refusal report run.
RejectSpec == Init /\ [][Next \/ (\E p \in Peers, d \in Docs : Reject(p, d))]_vars

TypeOK ==                                 \* every variable holds what it should
  /\ asked \subseteq Docs
  /\ holds \in [Peers -> SUBSET Docs]
  /\ current \in [Peers -> SUBSET Docs]
  /\ answered \subseteq Peers \X Docs
  /\ arrived \subseteq Peers \X Docs
  /\ merged \subseteq Docs
  /\ queue \in Seq(Docs)
  /\ published \in 0..MaxReports
  /\ seen \subseteq Docs
  /\ count \in 0..MaxReports
  /\ state \in {"waiting", "complete", "timedout"}
  /\ result \in {"none", "ok", "error"}

\* A sync that returns early, before any timeout, has every document it asked for merged.
INV_CompleteMeansEveryDocMerged ==
  state = "complete" => asked \subseteq merged

\* A sync that has read a report for every document it asked for does not wait for more.
INV_DoneOnceEveryDocReported ==
  (state = "waiting" /\ asked \subseteq seen) => Done

\* A sync that is still waiting after every peer's answer for every asked
\* document was handled, and after reading every report, is done, as long as
\* each asked document was in some peer's answer. (A document no peer answers
\* for is left to the timeouts.)
INV_NoWaitOnceEveryAnswerHandled ==
  LET answeredFor == UNION {holds[p] \cup current[p] : p \in Peers}  \* the asked documents some peer answers for
      allHandled ==                                                     \* every such answer was handled
        /\ \A p \in Peers : \A d \in asked \cap holds[p] : <<p, d>> \in arrived
        /\ \A p \in Peers : \A d \in asked \cap current[p] : <<p, d>> \in answered
  IN (state = "waiting" /\ queue = <<>> /\ asked \subseteq answeredFor /\ allHandled) => Done

\* Not promised, checked only by the witness: a complete sync has every peer's head of every asked document.
INV_EveryPeersHeadMerged ==
  state = "complete" =>
    \A p \in Peers : \A d \in asked \cap holds[p] : <<p, d>> \in arrived
\* Not promised, checked only by a witness: a sync that tells its caller Ok has every asked document merged.
INV_OkMeansEveryDocMerged ==
  result = "ok" => asked \subseteq merged
====
