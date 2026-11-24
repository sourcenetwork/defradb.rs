---- MODULE TakeoverReport ----
\* The story, in plain words.
\*
\* A DefraDB node keeps its access rules on a Vera deployment. When another
\* device takes that deployment over, a new term is certified, and some
\* submissions the node had seen confirmed are no longer there. A provider
\* call notices the new term and checks it: it sends each lost submission
\* again. Each replay has one of three outcomes: the deployment holds it
\* again, the node owes it again through its outbox, or the node cannot
\* restore it and says why. Then the term is checked, and what is owed is
\* handed to the outbox. If the outbox cannot take it yet, a later call tries
\* the delivery again.
\*
\* The embedder does not watch any of this. It is told on the node's bus:
\*   * once per term, by the call that checks it, after every replay is
\*     classified: the term, the cut of that term, and every replay in the
\*     place it holds, the owed ones included;
\*   * that the node failed closed, with the reason, when a check cannot
\*     vouch for what the term voided or cannot owe what it left. The node
\*     then refuses every decision, so the embedder must not take an empty
\*     report for a clean one.
\*
\* What must never happen:
\*   * a checked term with no report, or two reports;
\*   * a report sent while replays are still to be classified, or one that
\*     leaves a replay out;
\*   * a report that names one term and another term's cut;
\*   * an unrestorable replay with no reason;
\*   * a node that failed closed and never said so;
\*   * a report that says the node closed while its journal does not (the
\*     write of the close failed): the next call closes again and would
\*     report the term a second time.
\*
\* Code this models:
\*   crates/engine/vera/src/vera_rs/provider/terms.rs  Follower::check_with
\*       (Begin, Finish, Deliver, DeliveryFails, LateRefuse),
\*       Follower::certificate (the term and its cut, read together),
\*       Follower::owed_effects (Refuse), Follower::publish, takeover_report,
\*       Journal::close (the recorded flag of Finish and LateRefuse)
\*   crates/engine/vera/src/vera_rs/follow/mod.rs      first_check (Begin),
\*       reconcile (Replay, Interrupt, Close)
\*   crates/engine/vera/src/vera_rs/follow/lost.rs     lost_effects (Replay)
\*   crates/engine/vera/src/vera_rs/follow/report.rs   TakeoverReport,
\*       Applied, Unrestorable
\*   crates/foundation/events/src/event.rs             AcpTakeoverReconciledData
\*
\* Which replay taints which, and what a restart keeps, is
\* FollowDeployment.tla. That each list keeps every entry in order is
\* proofs/lean/VeraTakeoverReport.lean.

EXTENDS Naturals, Sequences

CONSTANTS
    Submissions,      \* the submissions a takeover makes the node send again
    MaxTerm,          \* how many terms the deployment certifies
    MaxFaults,        \* how many replays or deliveries may be cut short
    Publish,          \* TRUE: the provider publishes the report
    AfterAll,         \* TRUE: it waits until every replay is classified
    Once,             \* TRUE: only the call that checks a term reports it
    KeepOwed,         \* TRUE: the report keeps the replays owed again
    CutOfTerm,        \* TRUE: the cut is read with the term it belongs to
    NameUnrestorable, \* TRUE: an unrestorable replay says why
    NameClosed,       \* TRUE: a check that failed closed says so
    Recorded          \* TRUE: an event says closed only once the journal does

VARIABLES
    certified,   \* the newest term the light client holds a certificate of
    handled,     \* the term last checked, as the journal records it
    phase,       \* what the provider call is doing now
    checking,    \* the term the running call read at its start
    first,       \* whether the running call is the one that checks its term
    replayed,    \* the submissions the running check has sent again
    outcome,     \* the outcome of each replayed submission
    reason,      \* why an unrestorable submission could not be restored
    undelivered, \* whether changes owed again still wait for the outbox
    closedNode,  \* whether the node failed closed
    events,      \* the events published on the bus, in order
    faults       \* how many replays or deliveries were cut short so far

vars == <<certified, handled, phase, checking, first, replayed, outcome,
          reason, undelivered, closedNode, events, faults>>

\* The three places a replayed submission can hold in a report.
Kinds == {"applied", "owed", "unrestorable"}

\* The reason an unrestorable replay carries when it carries one.
Why == "lost policy creation"

\* One published event: the term, its cut (named by the term it belongs to),
\* the three lists, and whether the check failed closed.
Event == [term : 0..MaxTerm,                \* the term the event names
          cut : 0..MaxTerm,                 \* the term whose cut it carries
          applied : SUBSET Submissions,     \* replays held again
          owed : SUBSET Submissions,        \* replays owed again
          unrestorable : SUBSET Submissions, \* replays that cannot be restored
          closed : BOOLEAN]                 \* whether it says the node closed

\* The submissions of `rep` whose outcome in `out` is `kind`.
Of(rep, out, kind) == {i \in rep : out[i] = kind}

\* The event built for term `term` from the replays `rep` and their outcomes
\* `out`. `newest` is the newest certified term at that moment: a provider
\* that reads the cut late carries that term's cut instead.
Report(term, newest, rep, out, closed) ==
    [term |-> term,                                    \* the term checked
     cut |-> IF CutOfTerm THEN term ELSE newest,       \* the cut it is paired with
     applied |-> Of(rep, out, "applied"),              \* held again
     owed |-> IF KeepOwed THEN Of(rep, out, "owed") ELSE {}, \* owed again
     unrestorable |-> Of(rep, out, "unrestorable"),    \* not restorable
     closed |-> closed]                                \* failed closed or not

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ certified \in 0..MaxTerm                    \* a term the model allows
    /\ handled \in 0..MaxTerm                      \* a term the model allows
    /\ phase \in {"idle", "replay", "deliver"}     \* one of the three stages
    /\ checking \in 0..MaxTerm                     \* a term, or none
    /\ first \in BOOLEAN                           \* a yes or a no
    /\ replayed \subseteq Submissions              \* submissions sent again
    /\ outcome \in [Submissions -> Kinds \cup {"none"}] \* one outcome each
    /\ reason \in [Submissions -> {"", Why}]       \* its reason, when it has one
    /\ undelivered \in BOOLEAN                     \* a yes or a no
    /\ closedNode \in BOOLEAN                      \* a yes or a no
    /\ events \in Seq(Event)                       \* a list of events
    /\ faults \in 0..MaxFaults                     \* a count within its bound

\* At the start: no term is certified, nothing is replayed or published.
Init ==
    /\ certified = 0                               \* no takeover yet
    /\ handled = 0                                 \* so nothing is checked
    /\ phase = "idle"                              \* no call is running
    /\ checking = 0                                \* and none read a term
    /\ first = FALSE                               \* nor checks one
    /\ replayed = {}                               \* no replay has run
    /\ outcome = [i \in Submissions |-> "none"]    \* so none has an outcome
    /\ reason = [i \in Submissions |-> ""]         \* or a reason
    /\ undelivered = FALSE                         \* nothing is owed
    /\ closedNode = FALSE                          \* the node is open
    /\ events = <<>>                               \* the bus has seen nothing
    /\ faults = 0                                  \* nothing was cut short

\* Another device takes over: the light client holds a newer certificate. It
\* can happen at any time, also while a check is running.
Certify ==
    /\ certified < MaxTerm                         \* the model allows another
    /\ certified' = certified + 1                  \* the newest term goes up
    /\ UNCHANGED <<handled, phase, checking, first, replayed, outcome,
                   reason, undelivered, closedNode, events, faults>>

\* A provider call finds work: a term not checked yet, or changes still to
\* deliver. It reads the term and its cut once. If the journal has not
\* handled the term, this call is the one that checks it and starts the
\* replays afresh; otherwise it only delivers. A provider that reports on
\* every call (Once off) publishes again here, with nothing in its lists.
Begin ==
    /\ phase = "idle"                              \* no other call is running
    /\ ~closedNode                                 \* a closed node checks nothing
    /\ certified > handled \/ undelivered          \* there is something to do
    /\ checking' = certified                       \* the term is read once
    /\ first' = (certified > handled)              \* is it this call's to check
    /\ phase' = IF certified > handled THEN "replay" ELSE "deliver" \* what is next
    /\ replayed' = IF certified > handled THEN {} ELSE replayed \* replays start over
    /\ outcome' = IF certified > handled           \* and so do their outcomes
                  THEN [i \in Submissions |-> "none"] ELSE outcome
    /\ reason' = IF certified > handled            \* and their reasons
                 THEN [i \in Submissions |-> ""] ELSE reason
    /\ events' = IF certified <= handled /\ ~Once /\ Publish \* the planted repeat
                 THEN Append(events, Report(certified, certified, {}, outcome, FALSE))
                 ELSE events
    /\ UNCHANGED <<certified, handled, undelivered, closedNode, faults>>

\* One lost submission is sent again and classified. A provider that does not
\* wait for the rest (AfterAll off) publishes what it has at once.
Replay(i, kind) ==
    /\ phase = "replay"                            \* a check is replaying
    /\ i \in Submissions \ replayed                \* a submission not yet sent again
    /\ kind \in Kinds                              \* the outcome the replay found
    /\ replayed' = replayed \cup {i}               \* it is replayed
    /\ outcome' = [outcome EXCEPT ![i] = kind]     \* its outcome is recorded
    /\ reason' = [reason EXCEPT ![i] =             \* an unrestorable one says why
                     IF kind = "unrestorable" /\ NameUnrestorable THEN Why ELSE ""]
    /\ events' = IF ~AfterAll /\ Publish           \* the planted early report
                 THEN Append(events, Report(checking, certified, replayed', outcome', FALSE))
                 ELSE events
    /\ UNCHANGED <<certified, handled, phase, checking, first, undelivered,
                   closedNode, faults>>

\* The deployment cannot be reached part way: the call ends, the term stays
\* unchecked, and nothing is published. The next call replays again.
Interrupt ==
    /\ phase = "replay"                            \* a check is replaying
    /\ faults < MaxFaults                          \* the model allows a fault
    /\ faults' = faults + 1                        \* it is counted
    /\ phase' = "idle"                             \* the call is over
    /\ checking' = 0                               \* and holds no term
    /\ UNCHANGED <<certified, handled, first, replayed, outcome, reason,
                   undelivered, closedNode, events>>

\* The check cannot vouch for what the term voided (a void older than the
\* journal, or an archive refused when sent again): the node fails closed
\* for good and publishes what it had learnt, marked closed.
Close ==
    /\ phase = "replay"                            \* a check is replaying
    /\ closedNode' = TRUE                          \* the node fails closed
    /\ phase' = "idle"                             \* the call is over
    /\ checking' = 0                               \* and holds no term
    /\ events' = IF Publish                        \* the embedder is told
                 THEN Append(events, Report(checking, certified, replayed, outcome, NameClosed))
                 ELSE events
    /\ UNCHANGED <<certified, handled, first, replayed, outcome, reason,
                   undelivered, faults>>

\* Every replay is classified: the term is checked, and the report is
\* published once. If something is owed that the node cannot owe (a shared
\* provider, a change with no actor), it fails closed instead and the report
\* says so. Closing is a journal write that can fail (`recorded` FALSE, a
\* fault): the node is then not closed, the term stays checked with its owed
\* work waiting, and the report goes out as a checked term's. A provider that
\* does not wait for the write (Recorded off) says closed anyway.
Finish(refuse, recorded) ==
    /\ phase = "replay"                            \* a check is replaying
    /\ replayed = Submissions                      \* and every replay has run
    /\ refuse => Of(replayed, outcome, "owed") # {} \* only owed work can be refused
    /\ ~recorded => refuse /\ faults < MaxFaults   \* only a close write can fail
    /\ faults' = IF recorded THEN faults ELSE faults + 1 \* a failed write is counted
    /\ handled' = checking                         \* the term is checked
    /\ undelivered' = (Of(replayed, outcome, "owed") # {}) \* owed work waits
    /\ closedNode' = (refuse /\ recorded)          \* refused and written: closed
    /\ phase' = IF refuse THEN "idle" ELSE "deliver" \* else delivery is next
    /\ checking' = IF refuse THEN 0 ELSE checking  \* a refusing call holds no term
    /\ events' = IF Publish                        \* the one report of the term
                 THEN Append(events, Report(checking, certified, replayed, outcome,
                                            refuse /\ (recorded \/ ~Recorded) /\ NameClosed))
                 ELSE events
    /\ UNCHANGED <<certified, first, replayed, outcome, reason>>

\* The outbox takes what is owed (or nothing was owed): the call is done.
Deliver ==
    /\ phase = "deliver"                           \* a call is delivering
    /\ undelivered' = FALSE                        \* nothing waits any more
    /\ phase' = "idle"                             \* the call is over
    /\ checking' = 0                               \* and holds no term
    /\ UNCHANGED <<certified, handled, first, replayed, outcome, reason,
                   closedNode, events, faults>>

\* The outbox is not there yet, or refuses: the call ends with the changes
\* still waiting, and a later call delivers. Nothing is published.
DeliveryFails ==
    /\ phase = "deliver"                           \* a call is delivering
    /\ undelivered                                 \* something is owed
    /\ faults < MaxFaults                          \* the model allows a fault
    /\ faults' = faults + 1                        \* it is counted
    /\ phase' = "idle"                             \* the call is over
    /\ checking' = 0                               \* and holds no term
    /\ UNCHANGED <<certified, handled, first, replayed, outcome, reason,
                   undelivered, closedNode, events>>

\* A later call finds it cannot owe what the term left (the provider came to
\* serve several nodes since, or a close write failed before): the node fails
\* closed, and says so in one more event that names the changes still owed.
\* If the close write fails again (`recorded` FALSE, a fault) nothing changes
\* and nothing is published; a provider that does not wait for the write
\* (Recorded off) says closed anyway.
LateRefuse(recorded) ==
    /\ phase = "deliver"                           \* a call is delivering
    /\ ~first                                      \* one that did not check the term
    /\ undelivered                                 \* something is owed
    /\ ~recorded => faults < MaxFaults             \* the model allows a fault
    /\ faults' = IF recorded THEN faults ELSE faults + 1 \* a failed write is counted
    /\ closedNode' = recorded                      \* written: the node fails closed
    /\ phase' = "idle"                             \* the call is over
    /\ checking' = 0                               \* and holds no term
    /\ events' = IF Publish /\ (recorded \/ ~Recorded) \* the embedder is told
                 THEN Append(events, Report(checking, certified,
                                            Of(replayed, outcome, "owed"), outcome, NameClosed))
                 ELSE events
    /\ UNCHANGED <<certified, handled, first, replayed, outcome, reason,
                   undelivered>>

\* Whether nothing is left to do: the node closed, or every term the model
\* certifies is checked and delivered.
Settled ==
    /\ phase = "idle"                              \* no call is running
    /\ \/ closedNode                               \* the node is closed for good
       \/ /\ certified = MaxTerm                   \* or no term is to come
          /\ handled = certified                   \* the newest is checked
          /\ ~undelivered                          \* and nothing waits

\* Once settled, nothing else happens.
Stop ==
    /\ Settled                                     \* nothing is left to do
    /\ UNCHANGED vars                              \* so nothing changes

\* Every step the system can take.
Next ==
    \/ Certify                                     \* a takeover is certified
    \/ Begin                                       \* a call starts
    \/ \E i \in Submissions : \E kind \in Kinds : Replay(i, kind) \* a replay
    \/ Interrupt                                   \* a replay is cut short
    \/ Close                                       \* a check fails closed
    \/ \E refuse, recorded \in BOOLEAN : Finish(refuse, recorded) \* a check completes
    \/ Deliver                                     \* owed work is handed over
    \/ DeliveryFails                               \* or is not, yet
    \/ \E recorded \in BOOLEAN : LateRefuse(recorded) \* or cannot be owed
    \/ Stop                                        \* or nothing is left

\* The system starts at Init, takes Next steps, and does not stop while a
\* step that changes something is possible.
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Every replay holds one of the three places, and an unrestorable one
\* carries its reason.
INV_Classified ==
    \A i \in replayed :                            \* for each replay so far
        /\ outcome[i] \in Kinds                    \* it is in one of the lists
        /\ outcome[i] = "unrestorable" => reason[i] # "" \* with its reason

\* A checked term has its report on the bus.
INV_Published ==
    handled > 0 =>                                 \* once a term is checked
        \E k \in DOMAIN events : events[k].term = handled \* an event names it

\* No term is reported twice: a second event for a term can only be the one
\* that says the node failed closed afterwards.
INV_PublishedOnce ==
    \A j, k \in DOMAIN events :                    \* for any two events
        (events[j].term = events[k].term /\ events[j].closed = events[k].closed)
            => j = k                               \* they are the same event

\* A report of a checked term leaves no replay out: every submission is in
\* one of its three lists.
INV_Complete ==
    \A k \in DOMAIN events :                       \* for each event
        ~events[k].closed =>                       \* that reports a checked term
            events[k].applied \cup events[k].owed \cup events[k].unrestorable
                = Submissions                      \* every replay is named

\* An event carries the cut of the term it names.
INV_CutOfTerm ==
    \A k \in DOMAIN events : events[k].cut = events[k].term \* same term for both

\* A node that failed closed told the embedder so.
INV_ClosedNamed ==
    closedNode => \E k \in DOMAIN events : events[k].closed \* an event says closed

\* An event that says the node closed is backed by the journal: the node is
\* closed, so no later call closes again and reports the term once more.
INV_ClosedRecorded ==
    \A k \in DOMAIN events : events[k].closed => closedNode \* the close is written

\* In the end every certified term is checked and delivered, or the node is
\* closed; with the invariants above, the embedder was told either way.
LIVE_Reported == <>[]Settled

====
