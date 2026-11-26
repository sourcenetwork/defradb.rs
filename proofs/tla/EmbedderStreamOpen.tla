---- MODULE EmbedderStreamOpen ----
\* The story. An application opens one of its stream protocols on a peer
\* and retries when the open fails as busy. Opening a stream takes tasks on
\* the opener's endpoint: the opener task, then a pump for each half. The
\* endpoint has room for only so many tasks, so an attempt may find room for
\* its opener task and none for its pumps, and is refused busy.
\*
\* What can happen: the application makes attempts, each finding room for its
\* pumps or not; the peer serves every request that reached it.
\*
\* What must never happen: an attempt refused busy delivers its request, so
\* the peer runs a handler for a request whose opener heard "busy" and
\* retried, and the handler runs twice for one request.
\*
\* Code this models:
\*   crates/database/pinc/src/stream_open.rs open_protocol_stream (Opens and
\*     Refused: the pumps are placed in one task before the request is
\*     written through the bridge);
\*   crates/database/pinc/src/protocol_stream.rs bridge, pump_send.
EXTENDS Naturals

CONSTANTS
  Attempts,       \* how many times the application tries to open the stream
  OpenMode        \* "SpawnFirst": pumps are placed, then the request written; "WriteFirst": the old order

VARIABLES
  tried,          \* attempts made so far
  done,           \* whether some attempt opened the stream
  delivered,      \* how many copies of the request reached the peer
  busy            \* how many attempts the opener heard refused busy

\* All the variables, for stuttering steps.
vars == <<tried, done, delivered, busy>>

\* Nothing has happened yet.
Init == /\ tried = 0
        \* no attempt opened the stream
        /\ done = FALSE
        \* the peer has no request
        /\ delivered = 0
        \* no attempt was refused
        /\ busy = 0

\* An attempt finds room for its pumps: the request is written once and the
\* stream opens.
Opens == /\ ~done
         \* attempts are left
         /\ tried < Attempts
         \* one more attempt
         /\ tried' = tried + 1
         \* the stream is open
         /\ done' = TRUE
         \* its request reaches the peer
         /\ delivered' = delivered + 1
         \* no refusal
         /\ UNCHANGED busy

\* An attempt finds no room for its pumps and is refused busy. The guard:
\* with the pumps placed first, nothing was written; in the old order the
\* request was already on the wire.
Refused == /\ ~done
           \* attempts are left
           /\ tried < Attempts
           \* one more attempt
           /\ tried' = tried + 1
           \* the opener hears busy
           /\ busy' = busy + 1
           \* the request reached the peer only if it was written first
           /\ delivered' = IF OpenMode = "WriteFirst" THEN delivered + 1 ELSE delivered
           \* the stream did not open
           /\ UNCHANGED done

\* Every step the system can take.
Next == Opens \/ Refused

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* Every variable holds a value of its kind.
INV_TypeOK == /\ tried \in 0..Attempts
              \* opened or not
              /\ done \in BOOLEAN
              \* copies delivered
              /\ delivered \in 0..Attempts
              \* refusals heard
              /\ busy \in 0..Attempts

\* An attempt refused busy delivers nothing: the peer has a request only for
\* the attempt that opened, so a retried open runs its handler once.
INV_BusyDeliversNothing == delivered = IF done THEN 1 ELSE 0
====
