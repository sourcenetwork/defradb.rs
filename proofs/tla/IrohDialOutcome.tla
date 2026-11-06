---- MODULE IrohDialOutcome ----
\* The story, in plain words.
\*
\* A node dials a peer over iroh (crates/p2p-adapter/src/iroh.rs
\* `connect_peer`; crates/p2p/src/iroh/endpoint_commands.rs `handle_dial`).
\* The QUIC handshake completes, the dialer registers the connection, and the
\* dial answers. The peer may refuse the dialer (its allowlist does not admit
\* it): it then closes the connection with the "not authorized" reason, a
\* moment after the handshake. The dialer's stream handler sees the close and
\* takes the peer out of its connected set.
\*
\* What must never happen:
\*   1. the dial answers Ok when the refusal had already arrived before it
\*      answered - INV_RefusalKnownIsSaid;
\*   2. a dial is left waiting although its outcome is known: every dial is
\*      answered, by itself, without a clock - LIVE_DialAnswered.
\*
\* The Rust code each guard models:
\*   CheckClose   handle_dial looks at the connection's close reason before
\*                it answers, and answers Unauthorized on the refusal;
\*   NoPoll       connect_peer takes the dial's answer and does not then wait
\*                for the peer to show in the connected set (the old
\*                poll_until_connected loop, which misses a connection that
\*                closed between two polls and waits to its timeout).

EXTENDS Naturals

CONSTANTS
  Refuses,     \* TRUE: the peer refuses the dialer
  CheckClose,  \* TRUE: the dial answers from the connection's close reason
  NoPoll       \* TRUE: connect_peer ends with the dial's answer

VARIABLES
  conn,        \* the connection: "none", "up" (handshake done, registered), "closed" (refused)
  answer,      \* the dial's answer: "none", "ok", "unauthorized"
  inSet,       \* the peer is in the dialer's connected set
  waiting,     \* connect_peer waits for the peer to show in the set (the old poll)
  done,        \* connect_peer returned
  okOnClosed   \* (record) the dial answered ok on a connection the refusal had already closed

vars == <<conn, answer, inSet, waiting, done, okOnClosed>>  \* every variable

Init ==                       \* at the start:
  /\ conn = "none"            \* not dialled yet
  /\ answer = "none"          \* no answer
  /\ inSet = FALSE            \* not connected
  /\ waiting = FALSE          \* not waiting
  /\ done = FALSE             \* not returned
  /\ okOnClosed = FALSE       \* nothing went wrong

\* The handshake completes and the dialer registers the connection.
Handshake ==
  /\ conn = "none"
  /\ conn' = "up" /\ inSet' = TRUE                        \* registered: in the connected set
  /\ UNCHANGED <<answer, waiting, done, okOnClosed>>

\* The peer refuses: it closes the connection, and the dialer's stream handler takes the peer out of the set.
Refuse ==
  /\ Refuses /\ conn = "up"
  /\ conn' = "closed" /\ inSet' = FALSE                   \* closed with the refusal, out of the set
  /\ UNCHANGED <<answer, waiting, done, okOnClosed>>

\* The dial answers, from the connection's close reason when CheckClose holds.
Answer ==
  /\ conn # "none" /\ answer = "none"
  /\ answer' = IF CheckClose /\ conn = "closed" THEN "unauthorized" ELSE "ok"  \* MUTATION no_check: always ok
  /\ okOnClosed' = (okOnClosed \/ (answer' = "ok" /\ conn = "closed"))  \* recorded: ok though refused already
  /\ UNCHANGED <<conn, inSet, waiting, done>>

\* connect_peer takes the answer: an unauthorized ends it; an ok ends it too, or (NoPoll broken) starts the wait.
Take ==
  /\ answer # "none" /\ ~done /\ ~waiting
  /\ IF answer = "ok" /\ ~NoPoll
       THEN waiting' = TRUE /\ UNCHANGED done              \* MUTATION poll: waits to see the peer in the set
       ELSE done' = TRUE /\ UNCHANGED waiting              \* returns the dial's answer
  /\ UNCHANGED <<conn, answer, inSet, okOnClosed>>

\* The old poll: it ends only when a look finds the peer in the set; a peer that left it is never found again.
PollSees ==
  /\ waiting /\ inSet
  /\ done' = TRUE /\ waiting' = FALSE
  /\ UNCHANGED <<conn, answer, inSet, okOnClosed>>

Next == Handshake \/ Refuse \/ Answer \/ Take \/ PollSees

\* Each step that can happen happens; the refusal, if the peer refuses, comes at some moment.
Spec == Init /\ [][Next]_vars /\ WF_vars(Handshake) /\ WF_vars(Answer) /\ WF_vars(Take)
        /\ WF_vars(PollSees) /\ WF_vars(Refuse)

TypeOK ==
  /\ conn \in {"none", "up", "closed"}
  /\ answer \in {"none", "ok", "unauthorized"}
  /\ inSet \in BOOLEAN /\ waiting \in BOOLEAN /\ done \in BOOLEAN /\ okOnClosed \in BOOLEAN

\* A refusal that arrived before the dial answered is what the dial says: never ok on a connection it closed.
INV_RefusalKnownIsSaid == ~okOnClosed
\* Every dial is answered and connect_peer returns.
LIVE_DialAnswered == <>done
====
