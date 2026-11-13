---- MODULE HeldPubsubRequest ----
\* The story. A node asks its peers for the newest heads of some documents
\* (a doc-sync request) by publishing one message on the gossip topic
\* "doc-sync". The peers are already connected, but gossip learns that a peer
\* reads the topic only when that peer's "I subscribe" note arrives, a moment
\* after the connection. While no peer is known on the topic, gossip refuses
\* the message ("InsufficientPeers").
\*
\* Who acts: the requester (it publishes, waits, and reads replies) and each
\* peer (it subscribes, at a time nobody controls, and answers every copy of
\* the request it receives).
\*
\* What must never happen: the request ends while a peer that subscribed in
\* time never received it, the caller sees the same peer's reply twice, or
\* the request is sent again after its deadline has passed.
\*
\* How the code avoids it: the request is held until its deadline. It is
\* published again each time a peer subscribes (a wake-up from a Notify
\* that is armed before the first publish), a refusal before any peer exists
\* only means "not sent yet", and only the first reply from each peer is kept.
\* Once the deadline has passed nothing is sent again, and a publish still in
\* progress is given up. This is what Go does with pubsub-rpc's
\* WithRepublishing.
\*
\* Rust code modelled (crates/p2p/src/sync/coordinator/held_request.rs):
\*   phase, watching, woken, sent  ->  HeldRequest::next / HeldRequest::publish
\*     (publish_due, next_join, sent, refused)
\*   Deadline, Expire              ->  HeldRequest::next's Wake::Done and its
\*     deadline check on Wake::Joined, HeldRequest::publish_by_deadline,
\*     HeldRequest::finish
\*   Subscribe                     ->  PubsubServices::peer_subscribed
\*     (pubsub_services.rs), called from handle_peer_subscribed
\*     (event_handler/mod.rs)
\*   returned / DedupeReplies      ->  HeldRequest::answered
\*   Callers: SyncCoordinator::pubsub_sync_documents and
\*   pubsub_sync_branchable_collection (pubsub_client.rs). A caller that
\*   expects no reply sends once through HeldRequest::send and is not held;
\*   that path is covered by held_request_tests.rs, not by this model.
\*
\* The deadline can pass at any moment, even before any peer subscribes (the
\* path doc_sync_no_peer_ever_receives_fails_at_the_deadline tests). A peer
\* that subscribes just before it may get no copy in time, as in Go, so
\* Delivered is promised only for a deadline that comes once every
\* subscription and every reply is over (the "quiet" deadline below).

\* Numbers, and Cardinality for counting peers.
EXTENDS Naturals, FiniteSets

\* The peers connected to the requester.
CONSTANT Peers
\* TRUE: a refused publish keeps the request open (the fix). FALSE: it fails
\* at once (main before the fix).
CONSTANT HoldOnRefusal
\* TRUE: a peer subscribing wakes the request and it publishes again.
CONSTANT ResendOnJoin
\* TRUE: the wake-up is armed before the first publish, so a subscription
\* landing just after gossip's refusal still wakes the request.
CONSTANT WatchBeforePublish
\* TRUE: only the first reply from each peer is handed to the caller.
CONSTANT DedupeReplies
\* TRUE: after the deadline nothing is sent again, and a due resend or a
\* publish in progress ends the request instead.
CONSTANT StopAtDeadline

\* Where the requester is: about to publish, arming its wake-up, waiting, or done.
VARIABLE phase
\* Whether a wake-up (a Notified future) is armed.
VARIABLE watching
\* Whether the armed wake-up has fired and not been handled yet.
VARIABLE woken
\* The peers gossip knows read the topic.
VARIABLE subscribed
\* got[p]: how many copies of the request peer p has received.
VARIABLE got
\* handled[p]: how many of those copies peer p has answered.
VARIABLE handled
\* replies[p]: p's replies waiting in the requester's response channel.
VARIABLE replies
\* returned[p]: how many of p's replies the caller was handed.
VARIABLE returned
\* Whether any publish was accepted by gossip.
VARIABLE sent
\* How the request ended: still open, ok, or refused (no peer ever got it).
VARIABLE outcome
\* Whether the deadline has passed.
VARIABLE expired
\* Whether a publish was ever attempted (the first one is always allowed).
VARIABLE attempted
\* Whether a publish was attempted again after the deadline.
VARIABLE late
\* Whether the deadline came once everything was over: every peer
\* subscribed, every copy answered, every reply read, no wake-up pending.
VARIABLE quiet

\* Every variable, for stuttering steps.
vars == <<phase, watching, woken, subscribed, got, handled, replies, returned, sent, outcome,
          expired, attempted, late, quiet>>

\* The most copies a peer can get: the first publish plus one per subscription.
MaxCopies == 1 + Cardinality(Peers)

\* Every variable holds a value of the expected kind.
TypeOK ==
    \* The requester is in one of its four places.
    /\ phase \in {"Publish", "Arm", "Wait", "Done"}
    \* Flags are booleans.
    /\ watching \in BOOLEAN /\ woken \in BOOLEAN /\ sent \in BOOLEAN
    \* So are the deadline's flags.
    /\ expired \in BOOLEAN /\ attempted \in BOOLEAN /\ late \in BOOLEAN /\ quiet \in BOOLEAN
    \* Subscribers are some of the peers.
    /\ subscribed \subseteq Peers
    \* Counters stay within the copies a peer can receive.
    /\ got \in [Peers -> 0..MaxCopies]
    /\ handled \in [Peers -> 0..MaxCopies]
    /\ replies \in [Peers -> 0..MaxCopies]
    /\ returned \in [Peers -> 0..MaxCopies]
    \* The outcome is one of three.
    /\ outcome \in {"open", "ok", "refused"}

\* Start: about to publish, nobody subscribed yet, nothing sent.
Init ==
    \* The first thing the request does is publish.
    /\ phase = "Publish"
    \* The fix arms the wake-up when the request is built, before publishing.
    /\ watching = WatchBeforePublish
    \* Nothing has woken it.
    /\ woken = FALSE
    \* Gossip knows no subscriber yet: the race window.
    /\ subscribed = {}
    \* No peer has a copy, an answer, or a returned reply.
    /\ got = [p \in Peers |-> 0]
    /\ handled = [p \in Peers |-> 0]
    /\ replies = [p \in Peers |-> 0]
    /\ returned = [p \in Peers |-> 0]
    \* Nothing was accepted, and the request is open.
    /\ sent = FALSE
    /\ outcome = "open"
    \* The deadline is ahead and no publish has been tried.
    /\ expired = FALSE
    /\ attempted = FALSE
    /\ late = FALSE
    /\ quiet = FALSE

\* Where the requester goes after publishing: wait if its wake-up is armed,
\* otherwise arm it first.
AfterPublish == IF watching THEN "Wait" ELSE "Arm"

\* The requester publishes (HeldRequest::publish). Gossip sends a copy to
\* every peer it knows on the topic, or refuses when it knows none.
Publish ==
    \* Only when a publish is due.
    /\ phase = "Publish"
    \* With the fix, only the first publish may still go out after the deadline
    \* (it is polled before the deadline in publish_by_deadline).
    /\ StopAtDeadline => (~expired \/ ~attempted)
    \* This attempt is late if the deadline had passed and it is a resend.
    /\ late' = (late \/ (expired /\ attempted))
    \* From now on any further publish is a resend.
    /\ attempted' = TRUE
    /\ IF subscribed = {}
       \* Refused: no peer on the topic.
       THEN IF HoldOnRefusal
            \* The fix: not sent yet, keep waiting.
            THEN /\ phase' = AfterPublish
                 /\ UNCHANGED <<got, sent, outcome>>
            \* Before the fix: the sync fails with InsufficientPeers.
            ELSE /\ phase' = "Done"
                 /\ outcome' = "refused"
                 /\ UNCHANGED <<got, sent>>
       \* Accepted: each known subscriber gets one more copy.
       ELSE /\ got' = [p \in Peers |-> IF p \in subscribed THEN got[p] + 1 ELSE got[p]]
            /\ sent' = TRUE
            /\ phase' = AfterPublish
            /\ UNCHANGED outcome
    \* Publishing touches nothing else.
    /\ UNCHANGED <<watching, woken, subscribed, handled, replies, returned, expired, quiet>>

\* Only in the late-arming bug: the wake-up is armed after the publish, so
\* a subscription between the two is missed.
Arm ==
    \* Only right after a publish without an armed wake-up.
    /\ phase = "Arm"
    \* Arm it and start waiting.
    /\ watching' = TRUE
    /\ phase' = "Wait"
    \* Nothing else changes.
    /\ UNCHANGED <<woken, subscribed, got, handled, replies, returned, sent, outcome,
                   expired, attempted, late, quiet>>

\* Peer p subscribes to the topic (gossip learns it, then the coordinator's
\* handle_peer_subscribed calls PubsubServices::peer_subscribed).
Subscribe(p) ==
    \* Each peer subscribes once, while the request is open.
    /\ p \notin subscribed
    /\ phase # "Done"
    \* Gossip now knows p reads the topic.
    /\ subscribed' = subscribed \cup {p}
    \* notify_waiters wakes an armed wake-up; with none armed it is lost.
    /\ woken' = (woken \/ watching)
    \* Nothing else changes.
    /\ UNCHANGED <<phase, watching, got, handled, replies, returned, sent, outcome,
                   expired, attempted, late, quiet>>

\* Peer p answers one copy of the request it received.
Answer(p) ==
    \* p has a copy it has not answered, and the request is open.
    /\ handled[p] < got[p]
    /\ phase # "Done"
    \* One more copy answered, one more reply in the response channel.
    /\ handled' = [handled EXCEPT ![p] = @ + 1]
    /\ replies' = [replies EXCEPT ![p] = @ + 1]
    \* Nothing else changes.
    /\ UNCHANGED <<phase, watching, woken, subscribed, got, returned, sent, outcome,
                   expired, attempted, late, quiet>>

\* The requester reads one of p's replies (the Wake::Reply branch).
TakeReply(p) ==
    \* Only while waiting, with a reply from p queued.
    /\ phase = "Wait"
    /\ replies[p] > 0
    \* The reply leaves the channel.
    /\ replies' = [replies EXCEPT ![p] = @ - 1]
    \* With dedupe, a peer already answered is skipped; otherwise it is
    \* handed to the caller again.
    /\ returned' = IF DedupeReplies /\ returned[p] >= 1
                   THEN returned
                   ELSE [returned EXCEPT ![p] = @ + 1]
    \* Nothing else changes.
    /\ UNCHANGED <<phase, watching, woken, subscribed, got, handled, sent, outcome,
                   expired, attempted, late, quiet>>

\* The wake-up fired (the Wake::Joined branch): re-arm it, then publish again.
Joined ==
    \* Only while waiting, after a peer subscribed.
    /\ phase = "Wait"
    /\ woken
    \* The fired wake-up is handled and a fresh one armed before publishing,
    \* so a later subscription is never missed.
    /\ woken' = FALSE
    /\ watching' = TRUE
    \* The fix publishes again; without the resend it just keeps waiting.
    /\ phase' = IF ResendOnJoin THEN "Publish" ELSE "Wait"
    \* Nothing else changes.
    /\ UNCHANGED <<subscribed, got, handled, replies, returned, sent, outcome,
                   expired, attempted, late, quiet>>

\* Everything is over: every peer subscribed, no wake-up pending, every copy
\* answered and every reply read, and the requester is waiting.
AllOver ==
    /\ phase = "Wait"
    /\ subscribed = Peers
    /\ ~woken
    /\ \A p \in Peers : handled[p] = got[p] /\ replies[p] = 0

\* The deadline passes, at any moment while the request is open.
Deadline ==
    \* Only once, and only while the request is open.
    /\ ~expired
    /\ phase # "Done"
    \* From now on the deadline is behind us.
    /\ expired' = TRUE
    \* Remember whether it came once everything was over.
    /\ quiet' = AllOver
    \* Nothing else changes.
    /\ UNCHANGED <<phase, watching, woken, subscribed, got, handled, replies, returned, sent,
                   outcome, attempted, late>>

\* The request ends after its deadline (HeldRequest::finish): from waiting
\* (Wake::Done), or, with the fix, from a resend that is due or a publish in
\* progress (the Wake::Joined deadline check and publish_by_deadline).
Expire ==
    \* Only once the deadline has passed.
    /\ expired
    \* Without the fix only a waiting request ends; with it a due publish too.
    /\ \/ phase = "Wait"
       \/ StopAtDeadline /\ phase = "Publish"
    \* The request ends: refused only if gossip never accepted it.
    /\ phase' = "Done"
    /\ outcome' = IF sent THEN "ok" ELSE "refused"
    \* Nothing else changes.
    /\ UNCHANGED <<watching, woken, subscribed, got, handled, replies, returned, sent,
                   expired, attempted, late, quiet>>

\* A finished request stays finished (keeps TLC from reporting a deadlock).
Finished == phase = "Done" /\ UNCHANGED vars

\* One step: any of the above.
Next ==
    \/ Publish
    \/ Arm
    \/ Joined
    \/ Deadline
    \/ Expire
    \/ \E p \in Peers : Subscribe(p) \/ Answer(p) \/ TakeReply(p)
    \/ Finished

\* The whole behaviour: start in Init, then take steps.
Spec == Init /\ [][Next]_vars

\* A request that ended before its deadline, or whose deadline came once
\* every subscription and reply was over, was received by every peer.
Delivered == (phase = "Done" /\ (quiet \/ ~expired)) => \A p \in Peers : got[p] >= 1

\* The caller is handed at most one reply per peer.
OneReplyEach == \A p \in Peers : returned[p] <= 1

\* A request is reported refused only when no peer ever received it.
RefusedOnlyUnsent == outcome = "refused" => \A p \in Peers : got[p] = 0

\* Once the deadline has passed, the request is never sent again.
NoResendAfterDeadline == ~late
====
