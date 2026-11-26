---- MODULE PincJoinConfirm ----
\* The story, in plain words.
\*
\* A node runs pinc and names a hub. Its engine subscribes to a topic. On
\* gossipsub a subscription is in the peers' view once the call returns. A
\* pinc join returns before the hub knows of it, so a message published at
\* once could miss the new member. So a subscribe waits until the hub
\* confirms the join (pinc's Joined event), and fails after twelve seconds
\* with none. The coordinator retries a failed subscribe later.
\*
\* Who acts:
\*   * the engine: subscribes (each call here is one attempt, a retry is a
\*     second call) and unsubscribes;
\*   * a publish, which holds the topic joined for a while (the fanout hold),
\*     and the end of that hold;
\*   * pinc on the node: joins and leaves, tells the hub, takes the hub's
\*     lists of subscribers, and queues a Joined event the first time a list
\*     names the node (an event can be lost when the queue is full);
\*   * the hub: takes the node's subscribe and unsubscribe in order, and
\*     answers a subscribe with a list that names the node;
\*   * the node's event reader: takes Joined events off the queue in order.
\*
\* How the node decides: it remembers which joined topics a hub confirmed
\* (the confirmed set). A subscribe answers at once when its topic is in the
\* set, and waits otherwise. A Joined event of a topic still joined puts it
\* in the set and wakes every waiting subscribe; one of a topic left since
\* does nothing. Leaving a topic
\* takes it out of the set. A wait that runs out looks at the set once more;
\* if the topic is still not there, the subscribe fails and takes its topic
\* back (and leaves it, unless a publish holds it), so the retry joins afresh.
\*
\* What must never happen:
\*   1. a subscribe answers "subscribed" while the hub does not hold the
\*      node's current subscription - INV_OkOnlyWhenHeld;
\*   2. a subscribe fails although the node knows the hub confirmed it
\*      - INV_FailOnlyUnconfirmed;
\*   3. a subscribe waits for a confirmation the node already took
\*      - INV_NoMissedWake;
\*   4. the engine's subscription is left for good with no confirmation to
\*      come: no subscribe waits, pinc took the hub's list, and its Joined
\*      event is gone - INV_NoStuckSubscription;
\*   5. pinc is joined to a topic neither the engine nor a hold has, or the
\*      other way round - INV_JoinedMatches.
\*
\* What the GREEN configurations assume of pinc (both are pinc #483's
\* "for this subscription and no earlier one"; Red_UntaggedList and
\* Red_Undrained show what goes wrong without them):
\*   * HubTagsLists: a list counts only for the subscription it answers. pinc
\*     today takes any list that names the node, also one the hub sent before
\*     it saw a quick leave and join;
\*   * RejoinAfterDrain: a topic is joined again only once the reader took
\*     every Joined event already queued, so a queued confirmation of the old
\*     join is never read as one of the new join.
\*
\* Correspondence with the Rust code (crates/database/pinc/src/topics.rs):
\*
\*   | Model            | Rust                                                     |
\*   |------------------|----------------------------------------------------------|
\*   | Start            | Node::subscribe: into `subscribed`, then ep.join         |
\*   | Decide           | Node::subscribe's match: wait (answer_when_confirmed)    |
\*   |                  | unless `confirmed` holds the topic                       |
\*   | ConfirmCheck     | the `confirmed` check (RED: old code, wait only when the |
\*   |                  | join was new to pinc)                                    |
\*   | ReadJoined       | Node::topic_events, Event::Joined: Node::joined          |
\*   | StillJoined      | Node::joined's `subscribed` or `fanout` return (RED: off)|
\*   | Finish           | the task answer_when_confirmed spawns, woken in time     |
\*   | Timeout          | the same task when JOIN_WAIT runs out                    |
\*   | Refuse           | answer_when_confirmed when the endpoint has no room for  |
\*   |                  | the task: Node::abandon_join, then a busy error          |
\*   | Recheck          | its second look at `confirmed` (RED: off)                |
\*   | Rollback         | Node::abandon_join (RED: the subscription stays)         |
\*   | Leave            | Node::leave: out of `confirmed`, then ep.leave           |
\*   | ClearOnLeave     | Node::leave's `confirmed` removal (RED: off)             |
\*   | Unsubscribe      | Node::unsubscribe: out of `subscribed`, the hold ends,   |
\*   |                  | then Node::leave                                         |
\*   | Hold, Expire     | Node::hold_fanout, Node::expire_fanout                   |
\*   | VJoin, vconf     | pinc's Endpoint::join and its topic's `confirmed` flag   |
\*   |                  | (pinc-broadcast Topics::subscribe, on_subs)              |
\*   | Hub, Recv        | the hub's SUB and UNSUB handling, and its SUBS lists     |
\*
\* Gaps, explained:
\*   * One topic: topics never interact here.
\*   * Time is not modeled: a wait may run out at any step, which covers
\*     every length of JOIN_WAIT. JOIN_WAIT (12 s) is under the coordinator's
\*     deadline (COLLECTION_SUBSCRIPTION_TIMEOUT, 15 s in
\*     crates/database/p2p/src/sync/coordinator/subscriptions/mod.rs), so the wait's
\*     own failure is what the coordinator sees; that ordering is a constant,
\*     not a behaviour.
\*   * One hub. A hub that never lists the node (a topic past the hub's 256
\*     listed subscribers, pinc #483) is a hub that never sends a list: every
\*     subscribe then fails, which no invariant here forbids.
\*   * The endpoint has hubs: with none (`has_hubs` false) a subscribe
\*     answers at once and nothing here applies.

EXTENDS Naturals, Sequences

CONSTANTS Calls, MaxUnsub, MaxHold, MaxLost,
          ConfirmCheck, StillJoined, Recheck, Rollback, ClearOnLeave,
          HubTagsLists, RejoinAfterDrain
\* Calls: the subscribe attempts. MaxUnsub, MaxHold, MaxLost: how many
\* unsubscribes, fanout holds and lost events may happen, to keep the states
\* few. The five guards are TRUE as the code is written; each RED turns one
\* off. The last two are what pinc is assumed to do (see the story).

ASSUME /\ MaxUnsub \in Nat /\ MaxHold \in Nat /\ MaxLost \in Nat
       \* the budgets are counts;
       /\ \A b \in {ConfirmCheck, StillJoined, Recheck, Rollback, ClearOnLeave,
                    HubTagsLists, RejoinAfterDrain} : b \in BOOLEAN
       \* and every switch is on or off.

VARIABLES subscribed, fanout, vjoined, vconf, inst, hubHolds, up, down,
          queue, confirmed, pc, vnew, woken, unsubs, holds, lost, badOk, badFail
\* subscribed: the engine holds the topic (`subscribed`). fanout: a publish
\* holds it. vjoined: pinc is joined to it. vconf: pinc's own flag, a list
\* named the node since it joined. inst: which join this is, counting from 1.
\* hubHolds: the join the hub holds, 0 for none. up: subscribes and
\* unsubscribes on their way to the hub. down: the hub's lists on their way
\* to the node, each naming the join it answers. queue: the Joined events
\* pinc queued, each naming its join. confirmed: the node's `confirmed` set
\* holds the topic. pc: where each call is. vnew: whether the call's join was
\* new to pinc. woken: whether Node::joined woke the call. unsubs, holds,
\* lost: the budgets used. badOk, badFail: a wrong answer was given.

vars == <<subscribed, fanout, vjoined, vconf, inst, hubHolds, up, down,
          queue, confirmed, pc, vnew, woken, unsubs, holds, lost, badOk, badFail>>
\* Every variable, for the stuttering steps.

Pending == {"joined", "waiting"}
\* A call is pending once it joined and until it answers.

HoldsCurrent == vjoined /\ hubHolds = inst
\* The hub holds the node's current subscription: the node is joined and the
\* hub took this very join last.

CanJoin == ~RejoinAfterDrain \/ vjoined \/ queue = <<>>
\* A join new to pinc happens only once the reader took every queued event,
\* when that is assumed.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ subscribed \in BOOLEAN /\ fanout \in BOOLEAN
  \* the engine's and the hold's flags;
  /\ vjoined \in BOOLEAN /\ vconf \in BOOLEAN /\ confirmed \in BOOLEAN
  \* pinc's two flags and the node's set;
  /\ inst \in Nat /\ hubHolds \in Nat
  \* join numbers;
  /\ up \in Seq([k : Nat, sub : BOOLEAN])
  \* subscribes (sub) and unsubscribes on the way up, with their join;
  /\ down \in Seq(Nat) /\ queue \in Seq(Nat)
  \* lists and events, each naming a join;
  /\ pc \in [Calls -> {"idle", "joined", "waiting", "ok", "failed"}]
  \* where each call is;
  /\ vnew \in [Calls -> BOOLEAN] /\ woken \in [Calls -> BOOLEAN]
  \* each call's two flags;
  /\ unsubs \in Nat /\ holds \in Nat /\ lost \in Nat
  \* the budgets used;
  /\ badOk \in BOOLEAN /\ badFail \in BOOLEAN
  \* and the two wrong-answer flags.

Init ==
  \* Nothing joined, nothing on its way, every call yet to run.
  /\ subscribed = FALSE /\ fanout = FALSE
  \* the engine has no topic and no publish holds one;
  /\ vjoined = FALSE /\ vconf = FALSE /\ confirmed = FALSE
  \* pinc is not joined and nothing is confirmed;
  /\ inst = 0 /\ hubHolds = 0
  \* no join yet, and the hub holds none;
  /\ up = <<>> /\ down = <<>> /\ queue = <<>>
  \* nothing is on its way;
  /\ pc = [c \in Calls |-> "idle"]
  \* no call ran;
  /\ vnew = [c \in Calls |-> FALSE] /\ woken = [c \in Calls |-> FALSE]
  \* no call joined or was woken;
  /\ unsubs = 0 /\ holds = 0 /\ lost = 0
  \* no budget is used;
  /\ badOk = FALSE /\ badFail = FALSE
  \* and no wrong answer was given.

\* ---- pinc's join and the node's leave, as pieces of the steps below ----

VJoinVars == <<vjoined, vconf, inst, up>>
\* What pinc's join changes.

VJoin ==
  \* pinc joins: a join that is new to it counts as the next join, clears its
  \* flag and sends a subscribe to the hub; a second join changes nothing.
  IF vjoined
  THEN UNCHANGED VJoinVars
  \* already joined: nothing;
  ELSE /\ vjoined' = TRUE /\ vconf' = FALSE
       \* now joined, no list named it yet;
       /\ inst' = inst + 1
       \* the next join;
       /\ up' = Append(up, [k |-> inst + 1, sub |-> TRUE])
       \* and its subscribe goes up.

Leave ==
  \* Node::leave: the topic leaves the confirmed set (RED: it stays), then
  \* pinc leaves, clears its flag and sends an unsubscribe, if it was joined.
  /\ confirmed' = (confirmed /\ ~ClearOnLeave)
  \* the node forgets the confirmation;
  /\ IF vjoined
     THEN /\ vjoined' = FALSE /\ vconf' = FALSE
          \* pinc leaves;
          /\ up' = Append(up, [k |-> inst, sub |-> FALSE])
          \* and its unsubscribe goes up;
     ELSE UNCHANGED <<vjoined, vconf, up>>
  \* not joined: pinc does nothing;
  /\ UNCHANGED inst
  \* a leave is no new join.

\* ---- the engine's calls ----

Start(c) ==
  \* Call c subscribes: the topic goes into `subscribed`, then pinc joins.
  /\ pc[c] = "idle" /\ CanJoin
  \* c has not run, and a new join may happen now;
  /\ subscribed' = TRUE
  \* the engine has the topic;
  /\ VJoin
  \* pinc joins;
  /\ vnew' = [vnew EXCEPT ![c] = ~vjoined]
  \* c remembers whether its join was new to pinc;
  /\ pc' = [pc EXCEPT ![c] = "joined"]
  \* c goes on to decide;
  /\ UNCHANGED <<fanout, hubHolds, down, queue, confirmed, woken, unsubs, holds,
                 lost, badOk, badFail>>
  \* The rest stays.

Decide(c) ==
  \* c answers at once when the topic is confirmed, and waits otherwise
  \* (RED: it waits only when its join was new to pinc, as the old code).
  /\ pc[c] = "joined"
  \* c joined;
  /\ LET wait == IF ConfirmCheck THEN ~confirmed ELSE vnew[c]
     \* whether c waits;
     IN IF wait
        THEN /\ pc' = [pc EXCEPT ![c] = "waiting"]
             \* c waits;
             /\ woken' = [woken EXCEPT ![c] = FALSE]
             \* not woken yet;
             /\ UNCHANGED badOk
             \* nothing answered;
        ELSE /\ pc' = [pc EXCEPT ![c] = "ok"]
             \* c answers "subscribed";
             /\ badOk' = (badOk \/ ~HoldsCurrent)
             \* wrong if the hub does not hold it;
             /\ UNCHANGED woken
             \* nothing to wake;
  /\ UNCHANGED <<subscribed, fanout, vjoined, vconf, inst, hubHolds, up, down,
                 queue, confirmed, vnew, unsubs, holds, lost, badFail>>
  \* The rest stays.

Finish(c) ==
  \* A woken c answers "subscribed". Its answer was settled when it was
  \* woken, so ReadJoined judges it then: an unsubscribe in between comes
  \* after the subscribe, as the engine sent them.
  /\ pc[c] = "waiting" /\ woken[c]
  \* c waits and was woken;
  /\ pc' = [pc EXCEPT ![c] = "ok"]
  \* it answers;
  /\ UNCHANGED <<subscribed, fanout, vjoined, vconf, inst, hubHolds, up, down,
                 queue, confirmed, vnew, woken, unsubs, holds, lost, badOk, badFail>>
  \* The rest stays.

Timeout(c) ==
  \* c's wait runs out, woken or not (the timer can win the race). It looks
  \* at the confirmed set once more (RED: it does not): confirmed, it
  \* answers; else it fails and takes the topic back, leaving it unless a
  \* publish holds it (RED: the subscription stays).
  /\ pc[c] = "waiting"
  \* c waits;
  /\ IF Recheck /\ confirmed
     THEN /\ pc' = [pc EXCEPT ![c] = "ok"]
          \* it answers "subscribed";
          /\ badOk' = (badOk \/ ~HoldsCurrent)
          \* wrong if the hub does not hold it;
          /\ UNCHANGED <<subscribed, vjoined, vconf, up, confirmed, badFail>>
          \* nothing is taken back;
     ELSE /\ pc' = [pc EXCEPT ![c] = "failed"]
          \* it fails;
          /\ badFail' = (badFail \/ confirmed)
          \* wrong if the node knew the hub confirmed it;
          /\ IF Rollback
             THEN /\ subscribed' = FALSE
                  \* the engine's topic is taken back;
                  /\ IF fanout
                     THEN UNCHANGED <<vjoined, vconf, up, confirmed>>
                     \* a held topic stays joined;
                     ELSE Leave
                     \* else the node leaves it;
             ELSE UNCHANGED <<subscribed, vjoined, vconf, up, confirmed>>
          \* RED: everything stays;
          /\ UNCHANGED badOk
          \* no "subscribed" was answered;
  /\ UNCHANGED <<fanout, inst, hubHolds, down, queue, vnew, woken, unsubs, holds, lost>>
  \* The rest stays.

Refuse(c) ==
  \* c would wait, but the endpoint has no room for the waiting task: c
  \* fails at once and takes the topic back, as a wait that ran out does
  \* (RED: the subscription stays). Nothing could confirm it in between.
  /\ pc[c] = "joined"
  \* c joined;
  /\ IF ConfirmCheck THEN ~confirmed ELSE vnew[c]
  \* and would wait;
  /\ pc' = [pc EXCEPT ![c] = "failed"]
  \* it fails;
  /\ badFail' = (badFail \/ confirmed)
  \* wrong if the node knew the hub confirmed it;
  /\ IF Rollback
     THEN /\ subscribed' = FALSE
          \* the engine's topic is taken back;
          /\ IF fanout
             THEN UNCHANGED <<vjoined, vconf, up, confirmed>>
             \* a held topic stays joined;
             ELSE Leave
             \* else the node leaves it;
     ELSE UNCHANGED <<subscribed, vjoined, vconf, up, confirmed>>
  \* RED: everything stays;
  /\ UNCHANGED <<fanout, inst, hubHolds, down, queue, vnew, woken, unsubs, holds,
                 lost, badOk>>
  \* The rest stays.

Unsubscribe ==
  \* The engine unsubscribes: out of `subscribed`, the hold ends, and the
  \* node leaves.
  /\ subscribed /\ unsubs < MaxUnsub
  \* the engine has the topic, and the budget allows;
  /\ subscribed' = FALSE /\ fanout' = FALSE
  \* the engine drops it and the hold ends;
  /\ Leave
  \* the node leaves;
  /\ unsubs' = unsubs + 1
  \* one more unsubscribe;
  /\ UNCHANGED <<inst, hubHolds, down, queue, pc, vnew, woken, holds, lost, badOk, badFail>>
  \* The rest stays.

\* ---- the fanout hold ----

Hold ==
  \* A publish holds the topic: a new hold joins it, unless the engine has
  \* it already.
  /\ ~fanout /\ holds < MaxHold /\ CanJoin
  \* no hold yet, the budget allows, and a join may happen;
  /\ fanout' = TRUE
  \* the topic is held;
  /\ IF subscribed THEN UNCHANGED VJoinVars ELSE VJoin
  \* joined, unless the engine joined it;
  /\ holds' = holds + 1
  \* one more hold;
  /\ UNCHANGED <<subscribed, hubHolds, down, queue, confirmed, pc, vnew, woken,
                 unsubs, lost, badOk, badFail>>
  \* The rest stays.

Expire ==
  \* The hold ends, and the node leaves unless the engine has the topic.
  /\ fanout
  \* a hold lasts;
  /\ fanout' = FALSE
  \* now it ends;
  /\ IF subscribed
     THEN UNCHANGED <<vjoined, vconf, up, confirmed, inst>>
     \* the engine keeps it joined;
     ELSE Leave
     \* else the node leaves;
  /\ UNCHANGED <<subscribed, hubHolds, down, queue, pc, vnew, woken, unsubs,
                 holds, lost, badOk, badFail>>
  \* The rest stays.

\* ---- the hub, pinc's list handling, and the reader ----

Hub ==
  \* The hub takes the next subscribe or unsubscribe. A subscribe it holds,
  \* and answers with a list naming the node; an unsubscribe it drops.
  /\ up # <<>>
  \* something is on its way up;
  /\ LET m == Head(up)
     \* the next one;
     IN IF m.sub
        THEN /\ hubHolds' = m.k
             \* the hub holds this join;
             /\ down' = Append(down, m.k)
             \* and its list goes down;
        ELSE /\ hubHolds' = 0
             \* the hub holds none;
             /\ UNCHANGED down
             \* and sends nothing;
  /\ up' = Tail(up)
  \* it is taken;
  /\ UNCHANGED <<subscribed, fanout, vjoined, vconf, inst, queue, confirmed, pc,
                 vnew, woken, unsubs, holds, lost, badOk, badFail>>
  \* The rest stays.

Accepts(k) == vjoined /\ ~vconf /\ (~HubTagsLists \/ k = inst)
\* pinc takes a list naming the node while joined and not yet told, and,
\* when lists are tagged, only the list of its current join.

Recv(keep) ==
  \* pinc takes the next list. One it accepts sets its flag and queues a
  \* Joined event; keep FALSE is a full queue that loses the event.
  /\ down # <<>>
  \* a list is on its way;
  /\ keep \/ lost < MaxLost
  \* a loss only within the budget;
  /\ LET k == Head(down)
     \* the join the list answers;
     IN IF Accepts(k)
        THEN /\ vconf' = TRUE
             \* pinc is told;
             /\ queue' = IF keep THEN Append(queue, k) ELSE queue
             \* and queues the event, or loses it;
             /\ lost' = IF keep THEN lost ELSE lost + 1
             \* counting a loss;
        ELSE UNCHANGED <<vconf, queue, lost>>
        \* else the list changes nothing;
  /\ down' = Tail(down)
  \* it is taken;
  /\ UNCHANGED <<subscribed, fanout, vjoined, inst, hubHolds, up, confirmed, pc,
                 vnew, woken, unsubs, holds, badOk, badFail>>
  \* The rest stays.

ReadJoined ==
  \* The reader takes the next Joined event. If the engine or a hold still
  \* has the topic (RED: whatever), Node::joined puts it in the confirmed set
  \* and wakes every waiting call; else the event changes nothing.
  /\ queue # <<>>
  \* an event is queued;
  /\ queue' = Tail(queue)
  \* it is taken;
  /\ IF subscribed \/ fanout \/ ~StillJoined
     THEN /\ confirmed' = TRUE
          \* the topic is confirmed;
          /\ woken' = [c \in Calls |-> woken[c] \/ pc[c] = "waiting"]
          \* every waiting call is woken;
          /\ badOk' = (badOk \/ ((\E c \in Calls : pc[c] = "waiting") /\ ~HoldsCurrent))
          \* and a woken call answers "subscribed": wrong if the hub does
          \* not hold the node's subscription now;
     ELSE UNCHANGED <<confirmed, woken, badOk>>
     \* a topic left meanwhile: nothing;
  /\ UNCHANGED <<subscribed, fanout, vjoined, vconf, inst, hubHolds, up, down, pc,
                 vnew, unsubs, holds, lost, badFail>>
  \* The rest stays.

Next ==
  \* Any call, the engine, the hold, the hub, pinc or the reader acts.
  \/ \E c \in Calls : Start(c) \/ Decide(c) \/ Finish(c) \/ Timeout(c) \/ Refuse(c)
  \/ Unsubscribe \/ Hold \/ Expire
  \/ Hub \/ Recv(TRUE) \/ Recv(FALSE) \/ ReadJoined

Spec == Init /\ [][Next]_vars
\* Start empty and take any step.

INV_OkOnlyWhenHeld ==
  \* No subscribe answers "subscribed" while the hub does not hold the
  \* node's current subscription.
  ~badOk

INV_FailOnlyUnconfirmed ==
  \* No subscribe fails while the node knows the hub confirmed it.
  ~badFail

INV_NoMissedWake ==
  \* A call waits, unwoken, only while the topic is not confirmed: no call
  \* waits for an event that was already read.
  \A c \in Calls : (pc[c] = "waiting" /\ ~woken[c]) => ~confirmed

INV_NoStuckSubscription ==
  \* An engine subscription that is not confirmed, with no call pending, can
  \* still be confirmed: pinc was not told yet, or its event is queued.
  (subscribed /\ ~confirmed /\ \A c \in Calls : pc[c] \notin Pending)
    => (~vconf \/ queue # <<>>)

INV_JoinedMatches ==
  \* pinc is joined exactly while the engine or a hold has the topic.
  vjoined = (subscribed \/ fanout)

====
