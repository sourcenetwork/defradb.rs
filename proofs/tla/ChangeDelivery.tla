---------------------------- MODULE ChangeDelivery ----------------------------
\* The story. Local writers and a P2P merger commit updates to one
\* collection. Each committed update gets one row in the collection's update
\* journal (`_documentUpdates`): rows are numbered 1, 2, 3, ... in commit
\* order, and a once key stops a block merged again from adding a second
\* row. After a commit the node notifies subscribers, through one of two
\* channels:
\*   - "closing": a bounded queue of notices, each naming its row. When the
\*     queue is full the notice is dropped, the drop is counted, and the
\*     channel closes, which wakes the reader (the GraphQL SSE stream).
\*   - "coalescing": one wake bit, set only after the state it announces is
\*     visible; a set bit absorbs later wakes (document change
\*     notifications).
\* A subscriber follows the contract's pattern: subscribe first, then read
\* the journal from its cursor to the head; read again on every wake and
\* every loss signal; apply a notice directly only when it names the next
\* row. Its cursor is saved together with what it observed. The subscriber
\* can crash: its subscription and every notice in flight are lost, its
\* cursor and what it observed are kept, and it starts again.
\* What must never happen: an update observed twice, or out of commit
\* order, or a drop that nobody counts. What must happen: every committed
\* update is observed in the end.
\* Out of scope: order across collections (each collection has its own
\* journal and events carry no global sequence), access control, paging
\* (a read here takes every row up to the head in one step), and the raw
\* bus's payload consumers beyond what Red_CounterNoWake shows.
\* Rust owners: crates/database/db/src/event/updates.rs (record, read,
\* once_key), crates/database/db/src/merge/merge_handler/composite_persist.rs
\* (persist_merged_document), crates/foundation/events/src/channel_bus.rs
\* (ChannelBus::publish_raw, Subscriber::record_overflow),
\* crates/foundation/events/src/document_changes.rs (ChangePublisher::publish)
\* and crates/engine/query/src/runner/arrivals.rs (execute_arrivals_query).
EXTENDS Integers, FiniteSets, Sequences

\* Updates are the update blocks; Merged are the ones a peer delivers; Cap
\* is the closing channel's queue size; MaxCrashes bounds the crashes;
\* Channel picks the notification channel; Mode picks the planted bug.
CONSTANTS Updates, Merged, Cap, MaxCrashes, Channel, Mode

\* Merged updates are updates.
ASSUME Merged \subseteq Updates
\* The queue holds at least one notice.
ASSUME Cap \in Nat \ {0}
\* The subscriber crashes a bounded number of times.
ASSUME MaxCrashes \in Nat
\* The two channels the node offers.
ASSUME Channel \in {"closing", "coalescing"}
\* "safe" is the design; every other mode breaks exactly one guard.
ASSUME Mode \in {"safe", "silent_loss", "read_before_subscribe",
                 "journal_skips_merge", "remerge_appends", "counter_no_wake",
                 "wake_before_state", "cursor_not_durable"}

\* Writer state. stage: "new", "announced" (only Red_WakeBeforeState wakes
\* before it commits) or "committed". cidx: the commit's place in commit
\* order. remerged: the block was merged a second time. log: the journal's
\* rows, in commit order. toNotify: rows whose notice is not sent yet.
\* commits: commits so far.
\* Channel state. subscribed: the subscriber has a live subscription.
\* inbox: queued notices (row numbers). closed: the closing channel closed.
\* counter: drops counted and not yet read. overflowing: notices dropped
\* whose count is not recorded yet. wake: the coalescing wake bit.
\* dropped, recorded: every drop and every count so far, never reset.
\* Subscriber state. rphase: "down", "reading" (read, not yet subscribed:
\* only Red_ReadBeforeSubscribe) or "up". needRead: a wake or loss asks
\* for a read. cursor: the last row observed. obs: what it observed, in
\* order. crashes: crashes so far.
VARIABLES stage, cidx, remerged, log, toNotify, commits,
          subscribed, inbox, closed, counter, overflowing, wake, dropped,
          recorded,
          rphase, needRead, cursor, obs, crashes

\* Writer variables.
writer == <<stage, cidx, remerged, log, toNotify, commits>>
\* Channel variables.
channel == <<subscribed, inbox, closed, counter, overflowing, wake, dropped,
             recorded>>
\* Subscriber variables.
reader == <<rphase, needRead, cursor, obs, crashes>>
\* Every variable.
vars == <<writer, channel, reader>>

\* The updates the subscriber observed.
ObsSet == {obs[i] : i \in 1..Len(obs)}
\* The rows from just after the cursor up to the head.
Unread == SubSeq(log, cursor + 1, Len(log))

\* Nothing is committed, nobody is subscribed and nothing is observed.
Init == /\ stage = [u \in Updates |-> "new"]
        /\ cidx = [u \in Updates |-> 0]
        /\ remerged = [u \in Updates |-> FALSE]
        /\ log = <<>>
        /\ toNotify = {}
        /\ commits = 0
        /\ subscribed = FALSE
        /\ inbox = <<>>
        /\ closed = FALSE
        /\ counter = 0
        /\ overflowing = 0
        /\ wake = FALSE
        /\ dropped = 0
        /\ recorded = 0
        /\ rphase = "down"
        /\ needRead = FALSE
        /\ cursor = 0
        /\ obs = <<>>
        /\ crashes = 0

\* updates::record: the update gets its row in its own commit, unless
\* Red_JournalSkipsMerge leaves merged updates out.
HasRow(u) == ~(u \in Merged /\ Mode = "journal_skips_merge")

\* A local write or a merge commits u. Its row, when it has one, is
\* appended in commit order and its notice is queued to be sent.
Commit(u) ==
    \* Red_WakeBeforeState commits only what it already announced.
    /\ stage[u] = IF Mode = "wake_before_state" THEN "announced" ELSE "new"
    /\ stage' = [stage EXCEPT ![u] = "committed"]
    \* The commit takes the next place in commit order.
    /\ cidx' = [cidx EXCEPT ![u] = commits + 1]
    /\ commits' = commits + 1
    \* The row, at the next position.
    /\ log' = IF HasRow(u) THEN Append(log, u) ELSE log
    \* The notice is sent after the commit; Red_WakeBeforeState sent it before.
    /\ toNotify' = IF HasRow(u) /\ Mode # "wake_before_state"
                      THEN toNotify \cup {Len(log) + 1} ELSE toNotify
    /\ UNCHANGED <<remerged, channel, reader>>

\* Red_WakeBeforeState: the coalescing wake is sent before the update is
\* visible, so a reader it wakes reads the journal without it.
Announce(u) ==
    /\ Mode = "wake_before_state"
    /\ stage[u] = "new"
    /\ stage' = [stage EXCEPT ![u] = "announced"]
    \* The wake bit is set for a live subscription.
    /\ wake' = (wake \/ subscribed)
    /\ UNCHANGED <<cidx, remerged, log, toNotify, commits, subscribed, inbox,
                   closed, counter, overflowing, dropped, recorded, reader>>

\* A merged block arrives again, through another handler. The once key
\* makes its append a no-op; Red_ReMergeAppends drops the once key, so a
\* second row (and notice) appears.
ReMerge(u) ==
    /\ u \in Merged
    /\ stage[u] = "committed"
    /\ ~remerged[u]
    /\ remerged' = [remerged EXCEPT ![u] = TRUE]
    \* The once key: nothing appended unless the RED removed it.
    /\ IF Mode = "remerge_appends"
          THEN /\ log' = Append(log, u)
               /\ toNotify' = toNotify \cup {Len(log) + 1}
          ELSE UNCHANGED <<log, toNotify>>
    /\ UNCHANGED <<stage, cidx, commits, channel, reader>>

\* The node sends row p's notice. Nobody subscribed, or the channel closed:
\* nobody hears it, and the next subscription's read covers it.
\* Coalescing: the wake bit is set. Closing: the notice is queued, or, when
\* the queue is full (try_send's Full), dropped; the drop is counted in a
\* later step (record_overflow's fetch_add). Red_SilentLoss drops it with
\* nothing left to count it.
Notify(p) ==
    /\ p \in toNotify
    /\ toNotify' = toNotify \ {p}
    \* Exactly one of the four cases below applies.
    /\ \/ /\ (~subscribed \/ closed)
          /\ UNCHANGED <<inbox, wake, overflowing, dropped>>
       \/ /\ subscribed /\ ~closed /\ Channel = "coalescing"
          /\ wake' = TRUE
          /\ UNCHANGED <<inbox, overflowing, dropped>>
       \/ /\ subscribed /\ ~closed /\ Channel = "closing" /\ Len(inbox) < Cap
          /\ inbox' = Append(inbox, p)
          /\ UNCHANGED <<wake, overflowing, dropped>>
       \/ /\ subscribed /\ ~closed /\ Channel = "closing" /\ Len(inbox) >= Cap
          /\ dropped' = dropped + 1
          \* The count is owed, unless the RED loses it.
          /\ overflowing' = IF Mode = "silent_loss" THEN overflowing ELSE overflowing + 1
          /\ UNCHANGED <<inbox, wake>>
    /\ UNCHANGED <<stage, cidx, remerged, log, commits, subscribed, closed,
                   counter, recorded, reader>>

\* Subscriber::record_overflow: the drop is counted and the channel closes,
\* which wakes a reader waiting on it. Red_CounterNoWake counts and does
\* not close, as Node::subscribe's subscription does.
RecordOverflow ==
    /\ overflowing > 0
    /\ overflowing' = overflowing - 1
    /\ recorded' = recorded + 1
    \* The count lands only on a subscription still alive.
    /\ counter' = IF subscribed THEN counter + 1 ELSE counter
    /\ closed' = IF subscribed /\ Mode # "counter_no_wake" THEN TRUE ELSE closed
    /\ UNCHANGED <<writer, subscribed, inbox, wake, dropped, reader>>

\* The subscriber starts: subscribe, then read. Red_ReadBeforeSubscribe
\* reads first and subscribes in a later step.
Start ==
    /\ rphase = "down"
    /\ IF Mode = "read_before_subscribe"
          THEN /\ rphase' = "reading"
               /\ obs' = obs \o Unread
               /\ cursor' = Len(log)
               /\ UNCHANGED <<subscribed, needRead>>
          ELSE /\ rphase' = "up"
               /\ subscribed' = TRUE
               /\ needRead' = TRUE
               /\ UNCHANGED <<obs, cursor>>
    /\ UNCHANGED <<writer, inbox, closed, counter, overflowing, wake, dropped,
                   recorded, crashes>>

\* Red_ReadBeforeSubscribe's late subscription, after its read.
SubscribeLate ==
    /\ rphase = "reading"
    /\ rphase' = "up"
    /\ subscribed' = TRUE
    /\ UNCHANGED <<writer, inbox, closed, counter, overflowing, wake, dropped,
                   recorded, needRead, cursor, obs, crashes>>

\* The subscriber takes the next notice. The next row is applied with its
\* cursor in one step; an older row is skipped; a gap asks for a read.
\* Then it checks the drop counter (check_and_reset_dropped): a loss asks
\* for a read.
Recv ==
    /\ rphase = "up"
    /\ inbox # <<>>
    /\ LET p == Head(inbox)
           next == p = cursor + 1 /\ p <= Len(log)
       IN /\ inbox' = Tail(inbox)
          /\ obs' = IF next THEN Append(obs, log[p]) ELSE obs
          /\ cursor' = IF next THEN p ELSE cursor
          /\ needRead' = (needRead \/ p > cursor + 1 \/ counter > 0)
    /\ counter' = 0
    /\ UNCHANGED <<writer, subscribed, closed, overflowing, wake, dropped,
                   recorded, rphase, crashes>>

\* The coalescing wake: it is taken and asks for a read.
Wake ==
    /\ rphase = "up"
    /\ wake
    /\ wake' = FALSE
    /\ needRead' = TRUE
    /\ UNCHANGED <<writer, subscribed, inbox, closed, counter, overflowing,
                   dropped, recorded, rphase, cursor, obs, crashes>>

\* A closed, drained channel: subscribe again, then read.
Resubscribe ==
    /\ rphase = "up"
    /\ closed
    /\ inbox = <<>>
    /\ closed' = FALSE
    /\ counter' = 0
    /\ needRead' = TRUE
    /\ UNCHANGED <<writer, subscribed, inbox, overflowing, wake, dropped,
                   recorded, rphase, cursor, obs, crashes>>

\* `_documentUpdates` from the cursor: every row up to the head, observed
\* with the cursor in one step.
Read ==
    /\ rphase = "up"
    /\ needRead
    /\ needRead' = FALSE
    /\ obs' = obs \o Unread
    /\ cursor' = Len(log)
    /\ UNCHANGED <<writer, channel, rphase, crashes>>

\* The subscriber crashes: its subscription and every notice in flight are
\* gone, what it observed is kept, and so is its cursor, saved with it.
\* Red_CursorNotDurable forgets the cursor.
Crash ==
    /\ crashes < MaxCrashes
    /\ rphase # "down"
    /\ crashes' = crashes + 1
    /\ rphase' = "down"
    /\ needRead' = FALSE
    /\ cursor' = IF Mode = "cursor_not_durable" THEN 0 ELSE cursor
    \* The subscription and what it held are lost.
    /\ subscribed' = FALSE
    /\ inbox' = <<>>
    /\ closed' = FALSE
    /\ counter' = 0
    /\ wake' = FALSE
    /\ UNCHANGED <<writer, overflowing, dropped, recorded, obs>>

\* One step of a writer, the node, or the subscriber.
Next == \/ \E u \in Updates : Commit(u) \/ Announce(u) \/ ReMerge(u)
        \/ \E p \in toNotify : Notify(p)
        \/ RecordOverflow
        \/ Start \/ SubscribeLate \/ Recv \/ Wake \/ Resubscribe \/ Read
        \/ Crash

\* The node and the subscriber keep going; an announced update commits.
\* Writers need not write and the subscriber need not crash.
Fairness == /\ WF_vars(\E p \in toNotify : Notify(p))
            /\ WF_vars(RecordOverflow)
            /\ WF_vars(\E u \in Updates : stage[u] = "announced" /\ Commit(u))
            /\ WF_vars(Start) /\ WF_vars(SubscribeLate)
            /\ WF_vars(Recv) /\ WF_vars(Wake) /\ WF_vars(Resubscribe)
            /\ WF_vars(Read)

\* Behaviours start in Init, take Next steps, and are fair.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every variable stays in its range.
TypeOK == /\ stage \in [Updates -> {"new", "announced", "committed"}]
          /\ log \in Seq(Updates)
          /\ inbox \in Seq(Nat)
          /\ obs \in Seq(Updates)
          /\ cursor \in 0..Len(log)
          /\ rphase \in {"down", "reading", "up"}

\* No update is observed twice, and only committed updates are observed.
INV_ExactlyOnce == /\ \A i, j \in 1..Len(obs) : i # j => obs[i] # obs[j]
                   /\ \A u \in ObsSet : stage[u] = "committed"

\* Updates are observed in commit order.
INV_CommitOrder == \A i, j \in 1..Len(obs) : i < j => cidx[obs[i]] < cidx[obs[j]]

\* Every drop is counted, or its count is on its way.
INV_LossCounted == dropped = recorded + overflowing

\* Every committed update is observed in the end.
EventuallyObserved == \A u \in Updates : [](stage[u] = "committed" => <>(u \in ObsSet))

=============================================================================
