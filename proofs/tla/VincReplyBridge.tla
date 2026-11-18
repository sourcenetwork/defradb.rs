---- MODULE VincReplyBridge ----
\* The story, in plain words.
\*
\* A peer opens request streams to a node over one vinc connection. vinc's
\* streams cannot leave the thread that owns the endpoint, but the engine that
\* answers a request runs anywhere. So the endpoint keeps each request's send
\* stream in a table, under an id, and hands the engine a token carrying only
\* that id. The engine later does one of two things with the token:
\*   * it replies: the reply travels to the endpoint as a message, and the
\*     endpoint takes the stream out of the table and writes the reply on it;
\*   * it drops the token unspent: the token's drop sends a "drop" message,
\*     and the endpoint takes the stream out and resets it. That message is a
\*     try-send, so when the command queue is full it is lost.
\* Meanwhile the connection may end at any time, which ends every stream on
\* it; the endpoint then purges that connection's entries. And every entry
\* has a deadline: a sweep resets and removes the ones past it, which is what
\* catches a drop whose message was lost. The table holds at most Cap
\* entries; a request that finds it full is refused at once.
\*
\* A tiny example. A stream arrives and is held. The engine's handler fails
\* and drops its token; the queue is full, so the drop message is lost. The
\* deadline passes, the sweep resets the stream, and the table is empty again.
\*
\* What must never happen:
\*   1. a stream gets a reply and is also reset, or gets two replies
\*      - INV_OneOutcome;
\*   2. the table holds more than Cap entries - INV_Bounded;
\*   3. a reply is written on a stream whose connection ended
\*      - INV_NoWriteAfterEnd;
\*   4. an entry stays in the table forever - LIVE_NoLeak;
\*   5. a dropped token's stream, still open, is never reset - LIVE_DropResets.
\*
\* Correspondence with the Rust code (crates/p2p/src/vinc/):
\*
\*   | Model             | Rust                                                      |
\*   |-------------------|-----------------------------------------------------------|
\*   | Arrive            | Node::token in serve.rs: ReplyTable::issue, or a reset    |
\*   |                   | with RESET_BUSY when the table is full                    |
\*   | table, deadline   | ReplyTable in reply_table.rs, REPLY_DEADLINE              |
\*   | tok[s] = "live"   | the VincResponseToken (token.rs) the engine holds         |
\*   | Reply             | VincTransport::reply (transport.rs): spend the token and  |
\*   |                   | send Command::Reply, which waits for room                 |
\*   | Drop              | Drop for VincResponseToken: try_send Command::DropReply,  |
\*   |                   | lost when the queue is full                               |
\*   | Process           | Node::handle (endpoint.rs), Command::Reply and            |
\*   |                   | Command::DropReply: ReplyTable::take, then write_reply    |
\*   |                   | or SendStream::reset                                      |
\*   | ConnEnd, Purge    | Node::serve_connection ends, Node::disconnected calls     |
\*   |                   | ReplyTable::purge                                         |
\*   | Sweep, Tick       | Node::sweep_loop: ReplyTable::sweep every SWEEP_INTERVAL  |
\*   | TakeOnce          | take removes the entry it returns (RED: hands the stream  |
\*   |                   | out and keeps the entry)                                  |
\*   | DropReset         | the token's drop sends DropReply (RED: it sends nothing)  |
\*   | SweepGuard        | the sweep task runs (RED: no sweep)                       |
\*   | CapGuard          | issue refuses past capacity (RED: no bound)               |
\*
\* Gaps, explained:
\*   * One connection: entries of different connections never interact, so
\*     more connections add nothing but states.
\*   * A message's order in the queue is left free (a set): the endpoint
\*     handles each command on its own, and no rule depends on their order.
\*   * Lossy says whether a drop message may be lost, which happens when the
\*     queue is full; the queue's size is not modeled.
\*   * DeadlinePasses says whether the clock reaches the deadlines in this
\*     run. With it FALSE no deadline ever passes, so the sweep never fires:
\*     that is the run where drop and reply alone must keep the table clean.
\*   * The reply write itself (and its own 30 s write deadline) is one step.

EXTENDS Naturals, FiniteSets
\* Naturals gives numbers for the clock; FiniteSets gives Cardinality.

CONSTANTS N, Cap, D, Lossy, DeadlinePasses, TakeOnce, DropReset, SweepGuard, CapGuard
\* N streams arrive; the table holds Cap; an entry's deadline is D ticks after
\* it arrives. Lossy and DeadlinePasses describe the world. The four other
\* switches are guards in the code: TRUE as written, and each RED turns one
\* off.

ASSUME /\ N \in 1..3
       \* a few streams;
       /\ Cap \in 1..3
       \* a small table;
       /\ D \in 1..2
       \* a short deadline;
       /\ \A b \in {Lossy, DeadlinePasses, TakeOnce, DropReset, SweepGuard, CapGuard} : b \in BOOLEAN
       \* and every switch is on or off.

Streams == 1..N
\* The streams, by number.

VARIABLES st, tok, table, deadline, chan, conn, now, replied, wasReset
\* st: each stream as its peer sees it. tok: the engine's token for it.
\* table: the streams the endpoint holds. deadline: when each held entry is
\* due. chan: the messages in the command queue. conn: the connection.
\* now: the clock. replied, wasReset: history, what each stream was sent.

vars == <<st, tok, table, deadline, chan, conn, now, replied, wasReset>>
\* Every variable, for the stuttering steps.

Msgs == [kind : {"reply", "drop"}, s : Streams]
\* A command: a reply for a stream, or a drop of its token.

MaxTime == N * D + N
\* No run of the clock goes past this: each stream adds one deadline.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ st \in [Streams -> {"idle", "held", "replied", "reset", "ended", "refused"}]
  \* a stream has not arrived, waits for its reply, got it, was reset, ended
  \* with its connection, or was refused busy;
  /\ tok \in [Streams -> {"none", "live", "spent", "dropped"}]
  \* the engine has no token, holds one, spent it on a reply, or dropped it;
  /\ table \subseteq Streams
  \* the endpoint holds some of the streams;
  /\ deadline \in [Streams -> 0..MaxTime]
  \* each stream's deadline is a time on the clock;
  /\ chan \subseteq Msgs
  \* the queue holds some commands;
  /\ conn \in {"up", "down"}
  \* the connection is up or has ended;
  /\ now \in 0..MaxTime
  \* the clock is within its bound;
  /\ replied \in [Streams -> BOOLEAN]
  \* whether a reply was written on each stream;
  /\ wasReset \in [Streams -> BOOLEAN]
  \* and whether each was reset.

Init ==
  \* Nothing has arrived, the queue is empty, the connection is up.
  /\ st = [s \in Streams |-> "idle"]
  \* no stream has arrived;
  /\ tok = [s \in Streams |-> "none"]
  \* no token exists;
  /\ table = {}
  \* the table is empty;
  /\ deadline = [s \in Streams |-> 0]
  \* no deadline is set;
  /\ chan = {}
  \* the queue is empty;
  /\ conn = "up"
  \* the connection is up;
  /\ now = 0
  \* the clock starts at zero;
  /\ replied = [s \in Streams |-> FALSE]
  \* nothing was written;
  /\ wasReset = [s \in Streams |-> FALSE]
  \* and nothing was reset.

Arrive(s) ==
  \* A request arrives on stream s while the connection is up.
  /\ conn = "up"
  \* only a live connection carries a request;
  /\ st[s] = "idle"
  \* stream s has not arrived yet;
  /\ IF Cardinality(table) < Cap \/ ~CapGuard
     \* there is room (RED: room is never checked):
     THEN /\ table' = table \cup {s}
          \* the endpoint holds the stream,
          /\ st' = [st EXCEPT ![s] = "held"]
          \* which now waits for its reply,
          /\ tok' = [tok EXCEPT ![s] = "live"]
          \* the engine gets a token for it,
          /\ deadline' = [deadline EXCEPT ![s] = now + D]
          \* and its entry is due D ticks from now;
          /\ UNCHANGED wasReset
          \* nothing is reset;
     ELSE /\ st' = [st EXCEPT ![s] = "refused"]
          \* else the stream is refused busy at once,
          /\ wasReset' = [wasReset EXCEPT ![s] = TRUE]
          \* which is a reset,
          /\ UNCHANGED <<table, tok, deadline>>
          \* and nothing is held.
  /\ UNCHANGED <<chan, conn, now, replied>>
  \* The rest stays.

Reply(s) ==
  \* The engine replies on its live token: the reply joins the queue.
  /\ tok[s] = "live"
  \* the engine holds the token;
  /\ tok' = [tok EXCEPT ![s] = "spent"]
  \* spending it, so it can reply only once;
  /\ chan' = chan \cup {[kind |-> "reply", s |-> s]}
  \* and the reply is queued (the send waits for room, so it is never lost);
  /\ UNCHANGED <<st, table, deadline, conn, now, replied, wasReset>>
  \* The rest stays.

Drop(s) ==
  \* The engine drops its live token without replying.
  /\ tok[s] = "live"
  \* the engine holds the token;
  /\ tok' = [tok EXCEPT ![s] = "dropped"]
  \* it is gone;
  /\ \/ /\ DropReset
        \* its drop sends a drop message (RED: sends nothing),
        /\ chan' = chan \cup {[kind |-> "drop", s |-> s]}
        \* which joins the queue;
     \/ /\ ~DropReset \/ Lossy
        \* or nothing arrives: no message is sent, or the queue was full
        \* and the try-send lost it;
        /\ UNCHANGED chan
        \* and the queue is as it was.
  /\ UNCHANGED <<st, table, deadline, conn, now, replied, wasReset>>
  \* The rest stays.

Process(m) ==
  \* The endpoint handles one queued command.
  /\ m \in chan
  \* the command is in the queue;
  /\ chan' = chan \ {m}
  \* it leaves the queue;
  /\ IF m.s \in table
     \* The stream is still held:
     THEN /\ table' = IF TakeOnce \/ m.kind = "drop" THEN table \ {m.s} ELSE table
          \* taking it removes the entry (RED: a reply keeps it);
          /\ IF m.kind = "reply"
             THEN /\ replied' = [replied EXCEPT ![m.s] = conn = "up" \/ replied[m.s]]
                  \* a reply is written only while the connection is up,
                  /\ st' = [st EXCEPT ![m.s] = IF conn = "up" THEN "replied" ELSE st[m.s]]
                  \* and then the stream has its reply;
                  /\ UNCHANGED wasReset
                  \* nothing is reset;
             ELSE /\ wasReset' = [wasReset EXCEPT ![m.s] = conn = "up" \/ wasReset[m.s]]
                  \* a drop resets a stream still open,
                  /\ st' = [st EXCEPT ![m.s] = IF st[m.s] = "held" THEN "reset" ELSE st[m.s]]
                  \* and then the stream is reset;
                  /\ UNCHANGED replied
                  \* nothing is written.
     \* The stream is gone (purged or swept): the reply errs and nothing is
     \* written or reset.
     ELSE UNCHANGED <<table, st, replied, wasReset>>
  /\ UNCHANGED <<tok, deadline, conn, now>>
  \* The rest stays.

ConnEnd ==
  \* The connection ends, before any reply: every open stream ends with it.
  /\ conn = "up"
  \* the connection was up;
  /\ conn' = "down"
  \* now it has ended;
  /\ st' = [s \in Streams |-> IF st[s] = "held" THEN "ended" ELSE st[s]]
  \* and each stream still waiting has ended.
  /\ UNCHANGED <<tok, table, deadline, chan, now, replied, wasReset>>
  \* The rest stays.

Purge ==
  \* The endpoint sees the connection end and drops every entry of it.
  /\ conn = "down"
  \* the connection has ended;
  /\ table # {}
  \* and some entry is still held;
  /\ table' = {}
  \* each one is dropped, with nothing written (its stream is gone).
  /\ UNCHANGED <<st, tok, deadline, chan, conn, now, replied, wasReset>>
  \* The rest stays.

Due == {s \in table : deadline[s] <= now}
\* The held entries whose deadline has passed.

Sweep ==
  \* The sweep resets and removes every entry past its deadline.
  /\ SweepGuard
  \* the sweep task runs (RED: there is none);
  /\ Due # {}
  \* and some entry is due;
  /\ table' = table \ Due
  \* those entries leave the table;
  /\ wasReset' = [s \in Streams |-> wasReset[s] \/ (s \in Due /\ conn = "up")]
  \* each is reset while its connection is up (RED TakeOnce: even one that
  \* already got its reply);
  /\ st' = [s \in Streams |-> IF s \in Due /\ st[s] = "held" THEN "reset" ELSE st[s]]
  \* and a stream that waited is now reset.
  /\ UNCHANGED <<tok, deadline, chan, conn, now, replied>>
  \* The rest stays.

Tick ==
  \* Time passes, toward the deadlines still ahead.
  /\ DeadlinePasses
  \* in this run the clock reaches the deadlines;
  /\ \E s \in table : deadline[s] > now
  \* some held entry is not due yet;
  /\ now < MaxTime
  \* the clock is within its bound;
  /\ now' = now + 1
  \* and it moves on one tick.
  /\ UNCHANGED <<st, tok, table, deadline, chan, conn, replied, wasReset>>
  \* The rest stays.

Next ==
  \* One step of someone: an arrival, the engine, the endpoint, the
  \* connection, the sweep or the clock.
  \/ \E s \in Streams : Arrive(s) \/ Reply(s) \/ Drop(s)
  \* a request arrives, or the engine replies or drops its token;
  \/ \E m \in chan : Process(m)
  \* the endpoint handles a command;
  \/ ConnEnd \/ Purge \/ Sweep \/ Tick
  \* the connection ends, its entries are purged, the sweep runs, time passes.

Fairness ==
  \* What is sure to happen eventually, once it can.
  /\ WF_vars(\E s \in Streams : Reply(s) \/ Drop(s))
  \* the engine settles every token it holds;
  /\ WF_vars(\E m \in chan : Process(m))
  \* the endpoint handles every command;
  /\ WF_vars(Purge)
  \* it purges an ended connection;
  /\ WF_vars(Sweep)
  \* the sweep runs while something is due;
  /\ WF_vars(Tick)
  \* and time passes.

Spec == Init /\ [][Next]_vars /\ Fairness
\* Start empty, take steps, and make the progress above.

INV_OneOutcome ==
  \* A stream gets one answer at most: never both a reply and a reset.
  \A s \in Streams : ~(replied[s] /\ wasReset[s])

INV_Bounded ==
  \* The table never holds more than Cap entries.
  Cardinality(table) <= Cap

INV_NoWriteAfterEnd ==
  \* A stream whose connection ended before its reply never got one.
  \A s \in Streams : st[s] = "ended" => ~replied[s]

LIVE_NoLeak ==
  \* Eventually the table is empty for good: no entry is held forever.
  <>[](table = {})

LIVE_DropResets ==
  \* A dropped token's stream that is still open is reset in the end.
  \A s \in Streams : (tok[s] = "dropped" /\ st[s] = "held") ~> (st[s] # "held")

====
