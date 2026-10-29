---- MODULE PgWire ----
\* The story, in plain words.
\*
\* A Postgres client opens a TCP connection to the node's pg-compat port. One
\* task serves that connection from its first byte to its end. It goes
\* through three phases:
\*   1. the opening: the task peeks at the first byte and closes a client that
\*      starts TLS directly (this server speaks plain TCP); then it answers
\*      every SSL or GSS encryption request with a refusal, and leaves the next
\*      packet, a startup message or a cancel request, unread;
\*   2. the startup: the startup handler reads the startup message. An
\*      anonymous user is let in at once; a DID user is asked for a password
\*      (a signed token), and let in when it checks out;
\*   3. the queries: every later message goes to pgwire's query, extended-query
\*      or copy handler, chosen by the connection's state.
\* The clock is read once when the task starts serving the connection, right
\* after the accept, and 60 seconds on is the startup deadline. Every wait of phases 1 and 2 (the peek, the
\* refusals, the startup reads) ends at that one deadline, and the connection
\* with it. A started connection has no deadline: an idle client may stay.
\*
\* When a message of the extended protocol (Parse, Bind, Describe, Execute,
\* Close, Flush, Sync) fails, Postgres's rule is that the server drops every
\* message up to the next Sync, answers that Sync, and goes on: the state
\* AwaitingSync. A copy from the client (COPY FROM STDIN) ends with CopyDone,
\* CopyFail or an error: a copy begun by a simple query is then ready for the
\* next query, and one begun by Execute drops messages up to its Sync if it
\* failed. A fatal error, and a cancel request (which comes on a connection of
\* its own), close the connection, and the task ends there. The task also ends
\* when the client closes.
\*
\* A tiny example. A client sends a startup message for a DID user, then a
\* wrong token, then a query, all at once. The server asks for the token,
\* finds it wrong, sends a fatal error and closes. The query is never read:
\* the task ends at the close. Had the task gone on reading, the query would
\* have run with nobody logged in.
\*
\* What must never happen:
\*   1. a connection stays open past the startup deadline without finishing
\*      its startup - INV_StartupWithinDeadline;
\*   2. the startup deadline ends a connection that already started
\*      - INV_StartedNeverTimedOut;
\*   3. a query, extended-query or copy handler runs before the startup
\*      handler finished the startup - INV_NoQueryBeforeStartup;
\*   4. after an extended-protocol error, a message other than Sync is handled
\*      before that Sync - INV_SkipUntilSync;
\*   5. the connection stays in AwaitingSync after its Sync was answered
\*      - INV_SyncResumes;
\*   6. a copy outlives its CopyDone, its CopyFail or its error
\*      - INV_CopyEnds;
\*   7. a message is read after a fatal error or a cancel request
\*      - INV_NothingAfterEnd;
\*   8. the client closed, or a fatal error or a cancel request ended the
\*      connection, and the task never ends - LIVE_Ends.
\*
\* What the startup handler must uphold (a contract, with its own RED
\* configuration): every error it returns is FATAL (StartupErrorsFatal).
\* DIDAuthHandler::on_startup (crates/pg-compat/src/handler/auth.rs) returns
\* InvalidPassword, UnsupportedProtocolVersion, FailedToCoercePasswordMessage
\* and I/O errors, which pgwire's ErrorInfo maps to FATAL. A handler that
\* returned a plain ERROR would leave the connection in ReadyForQuery with
\* nobody logged in, since report() treats the startup message like any
\* simple message.
\*
\* Correspondence with the Rust code (crates/pg-compat/src/wire/ unless
\* named; line numbers are process.rs unless named):
\*
\*   | Model              | Rust                                                     |
\*   |--------------------|----------------------------------------------------------|
\*   | now, Tick          | the clock nuclei::time reads                             |
\*   | Limit              | STARTUP_TIMEOUT (process.rs:24), 60 seconds              |
\*   | dl                 | `deadline` (serve, process.rs:42), passed to run         |
\*   | pc = "peek", Peek  | serve: timeout_at(deadline, socket.peek) (process.rs:    |
\*   |                    | 43-49); a first byte 0x16 (TLS_HANDSHAKE) ends it        |
\*   | pc = "ssl", SslStep| run: timeout_at(deadline, refuse_ssl) (process.rs:86-91) |
\*   |                    | and refuse_ssl (process.rs:135-161)                      |
\*   | pc = "loop"        | run's loop (process.rs:94-129)                           |
\*   | LoopStep           | one turn: the read (process.rs:95-107), the extended     |
\*   |                    | flag (108-111), dispatch (112-121, 164-239), report on   |
\*   |                    | an error (122-125, 244-271), the close check (126-128)   |
\*   | TimeOut            | a timeout_at wait finds the deadline passed: `else       |
\*   |                    | { return Ok(()) }` (process.rs:44-46, 86-88) and         |
\*   |                    | `.ok().flatten()` (process.rs:101)                       |
\*   | st                 | the client's PgWireConnectionState (connection.rs:152-   |
\*   |                    | 158): "ssl" AwaitingSslRequest, "startup"                |
\*   |                    | AwaitingStartup, "auth" AuthenticationInProgress,        |
\*   |                    | "ready" ReadyForQuery and QueryInProgress, "sync"        |
\*   |                    | AwaitingSync, "copyS" CopyInProgress(false), "copyE"     |
\*   |                    | CopyInProgress(true)                                     |
\*   | closed             | Connection::closed (connection.rs:37), set by poll_close |
\*   |                    | (connection.rs:119-124), read by is_closed (54-56)       |
\*   | inbox, eof         | what the client sent and has not been read yet, and its  |
\*   |                    | close: Connection::next returns None (connection.rs:77)  |
\*   | started            | history: the startup handler finished the startup        |
\*   |                    | (finish_authentication in DIDAuthHandler::on_startup)    |
\*   | skipping, fin      | history: what Postgres's protocol says (skip until Sync; |
\*   |                    | the connection is over); nothing in the Rust             |
\*   | broke              | history of violations, to state the invariants           |
\*   | DeadlineAtAccept   | the deadline read before the peek (process.rs:42-46)     |
\*   |                    | (RED: read after it, the reviewed bug)                   |
\*   | SslArmed           | timeout_at around refuse_ssl (process.rs:86) (RED: none) |
\*   | StartupArmed       | timeout_at around a startup read (process.rs:100-101)    |
\*   |                    | (RED: a plain read)                                      |
\*   | OneDeadline        | one `deadline` for every wait (RED: a new timeout per    |
\*   |                    | wait, as a fresh sleep per read would give)              |
\*   | StartupOnly        | the `starting` check (process.rs:95-99) (RED: reads of a |
\*   |                    | started connection also under the deadline)              |
\*   | WaitForSync        | report's `if wait_for_sync` (process.rs:253-255) (RED:   |
\*   |                    | always ReadyForQuery)                                    |
\*   | SyncOnly           | AwaitingSync handles only Sync (process.rs:191-196)      |
\*   |                    | (RED: handles every message)                             |
\*   | SyncResumes        | set_state(ReadyForQuery) after on_sync (process.rs:194)  |
\*   |                    | (RED: stays AwaitingSync)                                |
\*   | CopyFlagFromState  | the extended flag from CopyInProgress (process.rs:108-   |
\*   |                    | 111) (RED: from the message alone)                       |
\*   | CopyDoneLeaves     | set_state(ReadyForQuery) on every CopyDone               |
\*   |                    | (process.rs:199-205) (RED: stays in the copy)            |
\*   | CloseOnEnd         | report closes on a fatal error (process.rs:264-266),     |
\*   |                    | dispatch closes after a cancel (process.rs:181-185)      |
\*   |                    | (RED: neither closes)                                    |
\*   | StopOnClose        | `if connection.is_closed() { return Ok(()) }`            |
\*   |                    | (process.rs:126-128) (RED: the loop reads on, the CI bug)|
\*   | StartupErrorsFatal | contract: DIDAuthHandler::on_startup's errors are FATAL  |
\*   |                    | (handler/auth.rs:41-118; pgwire error.rs ErrorInfo)      |
\*   | SimpleCopy,        | whether a query handler may start a copy from a Query or |
\*   | ExtendedCopy       | from an Execute; DefraQueryHandler starts none           |
\*
\* Gaps, explained:
\*   * A handler's work is one step that takes no time. The startup handler
\*     does no remote I/O (the token is checked locally) and writes a few
\*     short messages, which the socket's send buffer takes at once. A write
\*     that fails ends the task with its error, which only ends it sooner.
\*   * A message already buffered is read even when the deadline has passed:
\*     nuclei's Timeout polls its future before its timer (time/timeout.rs).
\*     Here the read and the timeout are both allowed at the deadline, and
\*     time stops there until the task leaves that wait. In the Rust a client
\*     that keeps the buffer full gets at most one cooperative budget (128
\*     socket reads, nuclei coop.rs) before the read answers Pending and the
\*     timer is seen.
\*   * The client's framing is honest: before its startup message it sends
\*     only the opening packets, and typed messages only after it. A
\*     malformed packet makes Connection::next return an error, and the loop
\*     ends (process.rs:105-107), which ends the task sooner.
\*   * One extended-protocol message ("ext") stands for Parse, Bind, Describe,
\*     Execute, Close and Flush: they share their handling here.
\*   * A message that is not a copy message during a copy is dropped, as in
\*     pgwire (process.rs:217); Postgres would end the copy with an error.
\*   * Terminate is dropped like any message with no handler; the client
\*     closes after it.
\*   * Time zero is when serve starts, in the task the listener spawned for
\*     the connection (crates/pg-compat/src/lib.rs, the accept loop); the
\*     moment between the accept and that task's first poll is not counted.

EXTENDS Naturals, Sequences
\* Naturals gives numbers; Sequences gives the client's queue of messages.

CONSTANTS Limit, MaxTime, MaxMsgs, PreMsgs, PostMsgs,
          SimpleCopy, ExtendedCopy, StartupErrorsFatal,
          DeadlineAtAccept, SslArmed, StartupArmed, OneDeadline, StartupOnly,
          WaitForSync, SyncOnly, SyncResumes, CopyFlagFromState,
          CopyDoneLeaves, CloseOnEnd, StopOnClose
\* Limit is the startup time limit and MaxTime how far the clock may run;
\* MaxMsgs is how many messages the client may send; PreMsgs the messages it
\* may send before its startup message is out, PostMsgs the ones after;
\* SimpleCopy and ExtendedCopy say whether a handler may start a copy from a
\* Query or from an Execute; StartupErrorsFatal is the startup handler's
\* contract. Every other switch is a guard in the code: TRUE as written, and
\* each RED configuration turns one off.

ASSUME /\ Limit \in 1..3
       \* the time limit is a few ticks;
       /\ MaxTime \in 0..4
       \* the clock runs a few ticks at most;
       /\ MaxMsgs \in 1..6
       \* the client sends a few messages at most;
       /\ PreMsgs \subseteq {"tls", "ssl", "gss", "anon", "did", "cancel"}
       \* the opening packets: a TLS handshake, an SSL or GSS request, a
       \* startup for an anonymous or a DID user, or a cancel request;
       /\ PostMsgs \subseteq {"pwOk", "pwBad", "query", "ext", "sync", "data", "done", "fail"}
       \* the typed messages: a good or a wrong password, a simple query, an
       \* extended-protocol message, Sync, CopyData, CopyDone and CopyFail;
       /\ \A s \in {SimpleCopy, ExtendedCopy, StartupErrorsFatal, DeadlineAtAccept,
                    SslArmed, StartupArmed, OneDeadline, StartupOnly, WaitForSync,
                    SyncOnly, SyncResumes, CopyFlagFromState, CopyDoneLeaves,
                    CloseOnEnd, StopOnClose} : s \in BOOLEAN
       \* and every switch is on or off.

VARIABLES now, dl, pc, st, sslDone, gssDone, inbox, sent, cph, eof,
          closed, fin, started, skipping, broke
\* now: the clock. dl: the deadline the current wait ends at. pc: where the
\* task is ("peek", "ssl", "loop", or "done" once it returned). st: the
\* connection's state. sslDone, gssDone: an SSL or a GSS request was refused.
\* inbox: what the client sent and the task has not read. sent: how many
\* messages the client sent. cph: what the client may send next ("pre",
\* "post", or "over" after a TLS handshake or a cancel request). eof: the
\* client closed. closed: the task closed the connection. fin: a fatal error
\* or a cancel request ended the connection. started: the startup finished.
\* skipping: Postgres says to drop messages until a Sync. broke: the
\* promises some step broke.

vars == <<now, dl, pc, st, sslDone, gssDone, inbox, sent, cph, eof,
          closed, fin, started, skipping, broke>>
\* Every variable, for the stuttering steps.

StartupStates == {"ssl", "startup", "auth"}
\* The states of a connection that has not started.

CopyStates == {"copyS", "copyE"}
\* The states of a copy, begun by a simple query or by Execute.

Starting == st \in StartupStates
\* The connection has not started yet.

Armed ==
  \* The task's current wait ends at the deadline:
  \/ pc = "peek" /\ DeadlineAtAccept
  \* the peek, when the deadline was read before it;
  \/ pc = "ssl" /\ SslArmed
  \* the refusals, when they are under the deadline;
  \/ pc = "loop" /\ Starting /\ StartupArmed
  \* a startup read, when it is under the deadline;
  \/ pc = "loop" /\ ~Starting /\ ~StartupOnly
  \* a read after the startup, only in the RED that drops the `starting` check.

NextDl == IF OneDeadline THEN dl ELSE now + Limit
\* The deadline of the next wait: the same one, or (RED) a new one from now.

IsExt(m) == m \in {"ext", "sync"}
\* pgwire's is_extended_query: the message belongs to the extended protocol.

TrueExt(m) == IsExt(m) \/ st = "copyE"
\* What Postgres says: an error here makes the server skip until Sync,
\* because the message is extended or the copy was begun by Execute.

CodeExt(m) ==
  \* What the loop computes, before dispatch, as report's wait_for_sync:
  IF CopyFlagFromState /\ st \in CopyStates
  \* in a copy, the flag the copy state carries;
  THEN st = "copyE"
  \* otherwise, whether the message is extended.
  ELSE IsExt(m)

Handler(m) ==
  \* Which handler dispatch calls for message m in the current state:
  CASE m = "cancel" -> "cancel"
  \* a cancel request goes to the cancel handler in any state;
       [] st \in {"startup", "auth"} -> "startup"
  \* during the startup, every message goes to the startup handler;
       [] st = "sync" /\ m = "sync" -> "sync"
  \* in AwaitingSync, Sync goes to on_sync;
       [] st = "sync" /\ SyncOnly -> "drop"
  \* and every other message is dropped;
       [] st \in CopyStates /\ m \in {"data", "done", "fail"} -> m
  \* in a copy, the copy messages go to the copy handler;
       [] st \in CopyStates -> "drop"
  \* and every other message is dropped;
       [] m \in {"query", "ext", "sync"} -> m
  \* otherwise queries, extended messages and Sync go to their handlers;
       [] OTHER -> "drop"
  \* and everything else is dropped.

StartupErr == IF StartupErrorsFatal THEN {"fatal"} ELSE {"err", "fatal"}
\* An error from the startup handler: FATAL by its contract, or (RED) either.

Outcomes(m) ==
  \* What the handler for m may answer: "ok", an error "err", a fatal error
  \* "fatal", or "copy" (it started a copy):
  CASE Handler(m) \in {"cancel", "drop"} -> {"ok"}
  \* a dropped message and a cancel request do nothing that can fail;
       [] Handler(m) = "startup" /\ m \in {"ssl", "gss"} -> {"fatal"}
  \* an encryption request after the opening reads as a startup with an
  \* unknown protocol version, which pgwire refuses as FATAL;
       [] Handler(m) = "startup" /\ m \in {"anon", "did", "pwOk"} -> {"ok"} \cup StartupErr
  \* a startup or a good password succeeds, or fails on its write;
       [] Handler(m) = "startup" /\ m = "pwBad" -> StartupErr
  \* a wrong password is refused;
       [] Handler(m) = "startup" -> {"ok"}
  \* any other message during the startup is ignored by the handler;
       [] Handler(m) = "query" -> {"ok", "err", "fatal"} \cup (IF SimpleCopy THEN {"copy"} ELSE {})
  \* a query succeeds, fails, or starts a copy;
       [] Handler(m) = "ext" -> {"ok", "err", "fatal"} \cup (IF ExtendedCopy THEN {"copy"} ELSE {})
  \* an extended message succeeds, fails, or (Execute) starts a copy;
       [] Handler(m) = "fail" -> {"err", "fatal"}
  \* CopyFail always ends the copy with an error;
       [] OTHER -> {"ok", "err", "fatal"}
  \* Sync, CopyData and CopyDone succeed or fail.

OkState(m) ==
  \* The state after the handler for m succeeded:
  CASE Handler(m) = "startup" /\ st = "startup" /\ m = "anon" -> "ready"
  \* an anonymous user is let in;
       [] Handler(m) = "startup" /\ st = "startup" /\ m = "did" -> "auth"
  \* a DID user is asked for its token;
       [] Handler(m) = "startup" /\ st = "auth" /\ m = "pwOk" -> "ready"
  \* a good token lets the DID user in;
       [] Handler(m) \in {"query", "ext"} -> "ready"
  \* a query or an extended message leaves it ready;
       [] Handler(m) = "sync" /\ st = "sync" -> IF SyncResumes THEN "ready" ELSE "sync"
  \* the Sync that ends a skip makes it ready again;
       [] Handler(m) = "sync" -> "ready"
  \* a Sync on a ready connection leaves it ready;
       [] Handler(m) = "done" /\ st \in CopyStates -> IF CopyDoneLeaves THEN "ready" ELSE st
  \* a copy's CopyDone makes it ready: a simple copy is answered at once, an
  \* extended one by the Sync that follows;
       [] OTHER -> st
  \* and everything else changes nothing: CopyData, a dropped message.

QueryHandlers == {"query", "ext", "sync", "data", "done", "fail"}
\* The handlers that must not run before the startup finished.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ now \in 0..MaxTime
  \* the clock is within its bound;
  /\ dl \in Nat
  \* the deadline is a time;
  /\ pc \in {"peek", "ssl", "loop", "done"}
  \* the task is at one of its waits, or returned;
  /\ st \in StartupStates \cup {"ready", "sync"} \cup CopyStates
  \* the connection is in one of pgwire's states;
  /\ sslDone \in BOOLEAN /\ gssDone \in BOOLEAN
  \* each refusal happened or not;
  /\ inbox \in Seq(PreMsgs \cup PostMsgs)
  \* the unread messages are messages;
  /\ sent \in 0..MaxMsgs
  \* the client sent within its budget;
  /\ cph \in {"pre", "post", "over"}
  \* the client is in one of its phases;
  /\ eof \in BOOLEAN /\ closed \in BOOLEAN /\ fin \in BOOLEAN
  /\ started \in BOOLEAN /\ skipping \in BOOLEAN
  \* the flags are flags;
  /\ broke \subseteq {"startedTimeout", "unauth", "skip", "copy", "afterEnd"}
  \* and only these promises can be broken.

Init ==
  \* The connection was just accepted, at time zero:
  /\ now = 0
  \* the clock reads zero;
  /\ dl = Limit
  \* the deadline is the time limit from now;
  /\ pc = "peek"
  \* the task is about to peek at the first byte;
  /\ st = "ssl"
  \* pgwire's first state, AwaitingSslRequest;
  /\ sslDone = FALSE /\ gssDone = FALSE
  \* nothing refused yet;
  /\ inbox = <<>> /\ sent = 0 /\ cph = "pre" /\ eof = FALSE
  \* the client has sent nothing and not closed;
  /\ closed = FALSE /\ fin = FALSE /\ started = FALSE /\ skipping = FALSE
  \* nothing closed, ended, started or skipped;
  /\ broke = {}
  \* and no promise broken.

ClientSend(m) ==
  \* The client sends message m:
  /\ pc # "done"
  \* while the task still serves it;
  /\ ~eof
  \* before it closed;
  /\ sent < MaxMsgs
  \* within its budget of messages;
  /\ \/ /\ cph = "pre"
        /\ m \in PreMsgs
        /\ m = "tls" => sent = 0
  \* an opening packet before its startup (a TLS handshake only first);
     \/ /\ cph = "post"
        /\ m \in PostMsgs
  \* or a typed message after it;
  /\ inbox' = Append(inbox, m)
  \* the message waits in the socket;
  /\ sent' = sent + 1
  \* one more sent;
  /\ cph' = CASE m \in {"anon", "did"} -> "post"
  \* after a startup message come typed messages;
              [] m \in {"tls", "cancel"} -> "over"
  \* after a TLS handshake or a cancel request, nothing;
              [] OTHER -> cph
  \* otherwise the client stays where it was.
  /\ UNCHANGED <<now, dl, pc, st, sslDone, gssDone, eof, closed, fin, started, skipping, broke>>

ClientClose ==
  \* The client closes its side, at any time.
  /\ pc # "done"
  /\ ~eof
  /\ eof' = TRUE
  /\ UNCHANGED <<now, dl, pc, st, sslDone, gssDone, inbox, sent, cph, closed, fin, started, skipping, broke>>

Tick ==
  \* Time passes, unless the task waits on a deadline that has come: then
  \* its timer fires first (it does, on time, in the runtime).
  /\ pc # "done"
  /\ now < MaxTime
  /\ ~(Armed /\ now >= dl)
  /\ now' = now + 1
  /\ UNCHANGED <<dl, pc, st, sslDone, gssDone, inbox, sent, cph, eof, closed, fin, started, skipping, broke>>

TimeOut ==
  \* The task's wait reaches its deadline: the task returns, dropping the
  \* connection.
  /\ pc # "done"
  /\ Armed
  /\ now >= dl
  /\ pc' = "done"
  /\ broke' = IF Starting THEN broke ELSE broke \cup {"startedTimeout"}
  \* (a started connection ended this way breaks a promise)
  /\ UNCHANGED <<now, dl, st, sslDone, gssDone, inbox, sent, cph, eof, closed, fin, started, skipping>>

Peek ==
  \* serve's peek returns: a first byte arrived, or the client closed.
  /\ pc = "peek"
  /\ inbox # <<>> \/ eof
  /\ pc' = IF inbox # <<>> /\ Head(inbox) = "tls" THEN "done" ELSE "ssl"
  \* a TLS handshake is closed at once; anything else goes to the refusals;
  /\ dl' = IF DeadlineAtAccept THEN NextDl ELSE now + Limit
  \* (RED: the deadline is read only now, after the peek)
  /\ UNCHANGED <<now, st, sslDone, gssDone, inbox, sent, cph, eof, closed, fin, started, skipping, broke>>

SslStep ==
  \* refuse_ssl reads one packet, or finds the client gone.
  /\ pc = "ssl"
  /\ inbox # <<>> \/ eof
  /\ IF inbox = <<>>
     \* The client closed: refuse_ssl answers false and the task returns.
     THEN /\ pc' = "done"
          /\ UNCHANGED <<st, sslDone, gssDone, inbox>>
     ELSE IF Head(inbox) \in {"ssl", "gss"}
     \* An SSL or GSS request: refused, and read.
     THEN LET s == sslDone \/ Head(inbox) = "ssl"
              \* SSL refused now or before;
              g == gssDone \/ Head(inbox) = "gss"
              \* GSS refused now or before.
          IN /\ sslDone' = s
             /\ gssDone' = g
             /\ inbox' = Tail(inbox)
             /\ IF s /\ g
                \* Both refused: the opening is over.
                THEN pc' = "loop" /\ st' = "startup"
                ELSE UNCHANGED <<pc, st>>
     \* Anything else (a startup or a cancel request): left unread, and the
     \* opening is over.
     ELSE /\ pc' = "loop"
          /\ st' = "startup"
          /\ UNCHANGED <<sslDone, gssDone, inbox>>
  /\ dl' = NextDl
  /\ UNCHANGED <<now, sent, cph, eof, closed, fin, started, skipping, broke>>

Dispatch(m, o) ==
  \* run's loop reads message m and its handler answers o.
  LET h == Handler(m)
      \* the handler dispatch calls;
      err == o \in {"err", "fatal"}
      \* the handler failed;
      sNew == CASE o = "copy" /\ h = "query" -> "copyS"
      \* a query that starts a copy;
                [] o = "copy" -> "copyE"
      \* an Execute that starts one;
                [] err /\ CodeExt(m) /\ WaitForSync -> "sync"
      \* report after an extended error: AwaitingSync;
                [] err -> "ready"
      \* report after any other error: ReadyForQuery;
                [] OTHER -> OkState(m)
      \* and a success leaves the state its handler set.
      ending == h = "cancel" \/ o = "fatal"
      \* a cancel request, or a fatal error, ends the connection;
      closedNew == closed \/ (ending /\ CloseOnEnd)
      \* and the task closes it (RED: does not).
  IN /\ inbox' = Tail(inbox)
     \* the message is read;
     /\ st' = sNew
     /\ closed' = closedNew
     /\ fin' = (fin \/ ending)
     /\ started' = (started \/ (h = "startup" /\ o = "ok" /\ sNew = "ready"))
     \* the startup finished when its handler let the user in;
     /\ skipping' = CASE err /\ TrueExt(m) -> TRUE
     \* an extended error starts a skip;
                      [] h = "sync" -> FALSE
     \* an answered Sync ends one;
                      [] OTHER -> skipping
     /\ pc' = IF closedNew /\ StopOnClose THEN "done" ELSE "loop"
     \* the task returns once the connection is closed (RED: reads on);
     /\ broke' = broke
                 \cup (IF fin THEN {"afterEnd"} ELSE {})
     \* reading anything after the end breaks a promise;
                 \cup (IF h \in QueryHandlers /\ ~started THEN {"unauth"} ELSE {})
     \* so does a query handler before the startup finished;
                 \cup (IF skipping /\ h \notin {"drop", "sync", "cancel"} THEN {"skip"} ELSE {})
     \* so does handling anything but Sync during a skip;
                 \cup (IF st \in CopyStates /\ (h \in {"done", "fail"} \/ err) /\ sNew \in CopyStates
                       THEN {"copy"} ELSE {})
     \* and so does a copy still running after it ended.

LoopStep ==
  \* One turn of run's loop: the client closed, or a message is read.
  /\ pc = "loop"
  /\ inbox # <<>> \/ eof
  /\ IF inbox = <<>>
     \* The client closed: Connection::next answers None and the task returns.
     THEN /\ pc' = "done"
          /\ UNCHANGED <<st, inbox, closed, fin, started, skipping, broke>>
     ELSE \E o \in Outcomes(Head(inbox)) : Dispatch(Head(inbox), o)
     \* Otherwise the first message is dispatched, with any outcome.
  /\ dl' = NextDl
  /\ UNCHANGED <<now, sslDone, gssDone, sent, cph, eof>>

Client == (\E m \in PreMsgs \cup PostMsgs : ClientSend(m)) \/ ClientClose
\* The client sends or closes, as it likes.

Server == Peek \/ SslStep \/ LoopStep \/ TimeOut
\* The task takes a step.

Next == Client \/ Tick \/ Server
\* Each step is the client's, the clock's or the task's.

Spec == Init /\ [][Next]_vars /\ WF_vars(Server) /\ WF_vars(Tick)
\* The task never stops while it can step, and time keeps passing (up to
\* MaxTime); the client owes nothing: it may stay silent and never close.

INV_StartupWithinDeadline == (pc # "done" /\ Starting) => now <= Limit
\* Promise: no connection is still served past the time limit, counted from
\* when serve started on it, without having finished its startup.

INV_StartedNeverTimedOut == "startedTimeout" \notin broke
\* Promise: the startup deadline never ends a connection that started.

INV_NoQueryBeforeStartup == "unauth" \notin broke
\* Promise: no query, extended-query or copy handler runs before the startup
\* handler let the user in.

INV_SkipUntilSync == "skip" \notin broke
\* Promise: after an extended-protocol error, nothing but a Sync is handled
\* until that Sync.

INV_SyncResumes == (pc # "done" /\ ~skipping) => st # "sync"
\* Promise: the connection waits for a Sync only while it owes a skip, so the
\* answered Sync makes it take messages again.

INV_CopyEnds == "copy" \notin broke
\* Promise: a copy never outlives its CopyDone, its CopyFail or its error.

INV_NothingAfterEnd == "afterEnd" \notin broke
\* Promise: after a fatal error or a cancel request, no message is read.

LIVE_Ends == (eof \/ fin) ~> (pc = "done")
\* Promise: once the client closed, or a fatal error or a cancel request
\* ended the connection, the task ends.

====
