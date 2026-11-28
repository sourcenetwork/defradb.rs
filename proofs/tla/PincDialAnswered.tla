---- MODULE PincDialAnswered ----
\* The story, in plain words.
\*
\* Our node replicates documents to a target over pinc. The target can stop
\* without a word (a killed process) and come back later as a new life that
\* holds none of the old sessions. Our node keeps a set of peers it counts as
\* connected, and its retry pass pushes the documents still owed to the target
\* only while the target is in that set; while it is not, the pass dials it.
\*
\* pinc hands a dial over before the target answers when the target was
\* allowed lately (an optimistic dial). The target may be down: then the
\* connection never hears it and ends a few seconds later. A push made on it
\* fails, and each failure raises that document's backoff, so a target that
\* comes back finds its next retry far away.
\*
\* The rule: a dial counts the target as connected only once the target was
\* heard on it (pinc's Connection::answered). A connection the target never
\* answered is never in the set, so no push goes into it.
\*
\* A tiny example. The target stops. The pass dials; pinc hands the dial over
\* at once. The target is not heard, so it stays out of the set and nothing is
\* pushed; the dial ends. The target restarts, the next dial is heard, the set
\* holds it again, and the owed documents go out.
\*
\* What must never happen:
\*   1. a push on a connection the target never answered - INV_NoPushUnanswered;
\*   2. the target in the set through a dial it never answered
\*      - INV_ConnectedHeard.
\*
\* Correspondence with the Rust code:
\*
\*   | Model        | Rust                                                        |
\*   |--------------|-------------------------------------------------------------|
\*   | Dial, Hear   | crates/database/p2p/src/pinc/endpoint.rs Node::connection:           |
\*   |              | Endpoint::connect, then Connection::answered, under         |
\*   |              | CONNECT_TIMEOUT; pinc's Connection::answered               |
\*   |              | (pinc:src/conn.rs) and Event::Answered                     |
\*   |              | (pinc-core:src/api.rs)                                      |
\*   | connected    | Node::peers, filled by Node::connected (after the answer),  |
\*   |              | read by Command::ConnectedPeers                             |
\*   | Learn        | Node::serve_connection ends, Node::disconnected             |
\*   | Push         | crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass: replays the |
\*   |              | markers only when connected_peers holds the target          |
\*   | Stop/Restart | a node that is killed, then started again                   |
\*
\* RED configuration: CountAtHandOver = TRUE is the code before the answered
\* check, which counted the target as connected when the dial was handed over.
EXTENDS Naturals

CONSTANTS
    N,                \* the most documents owed to the target at once
    MaxLives,         \* the most lives of the target explored
    CountAtHandOver   \* the target counts as connected when a dial is handed over (the bug)

VARIABLES
    alive,      \* whether the target runs
    life,       \* the life of the target that runs, or ran last
    conn,       \* our connection to the target: its state, whether the target was heard on it, the life it reached
    connected,  \* whether our node counts the target as connected
    pending,    \* documents still owed to the target
    badPush     \* pushes made on a connection the target never answered

vars == <<alive, life, conn, connected, pending, badPush>>

\* No connection at all.
NoConn == [st |-> "none", heard |-> FALSE, life |-> 0]

\* Every variable has the shape it should.
TypeOK ==
    /\ alive \in BOOLEAN                                  \* the target runs or not
    /\ life \in 1..MaxLives                               \* one of the lives explored
    /\ conn.st \in {"none", "handed", "up"}               \* no connection, handed over, or heard
    /\ conn.heard \in BOOLEAN                             \* whether the target was heard on it
    /\ conn.life \in 0..MaxLives                          \* the life it reached, 0 for none
    /\ connected \in BOOLEAN                              \* our node's view
    /\ pending \in 0..N                                   \* the documents owed
    /\ badPush \in Nat                                    \* the count of the bad pushes

Init ==
    /\ alive = TRUE        \* the target runs
    /\ life = 1            \* its first life
    /\ conn = NoConn       \* no connection yet
    /\ connected = FALSE   \* not counted as connected
    /\ pending = 0         \* nothing owed
    /\ badPush = 0         \* no bad push yet

\* Whether the connection reaches the life of the target that runs now.
Live == alive /\ conn.life = life

\* A document is written that the target is owed.
Write ==
    /\ pending < N                                        \* room for one more
    /\ pending' = pending + 1                             \* one more owed
    /\ UNCHANGED <<alive, life, conn, connected, badPush>>

\* The target stops without a word: our connection stays until we learn it.
Stop ==
    /\ alive                                              \* it runs
    /\ alive' = FALSE                                     \* and stops
    /\ UNCHANGED <<life, conn, connected, pending, badPush>>

\* The target comes back as a new life, which holds none of the old sessions.
Restart ==
    /\ ~alive /\ life < MaxLives                          \* it is down; one more life explored
    /\ alive' = TRUE                                      \* it runs again
    /\ life' = life + 1                                   \* as a new life
    /\ UNCHANGED <<conn, connected, pending, badPush>>

\* The retry pass dials a target it does not count as connected. pinc hands the
\* dial over at once, before the target answers. The bug counts it connected now.
Dial ==
    /\ conn.st = "none" /\ ~connected                     \* no connection and not counted
    /\ conn' = [st |-> "handed", heard |-> FALSE, life |-> life]  \* handed over, not heard
    /\ connected' = CountAtHandOver                       \* counted only under the bug
    /\ UNCHANGED <<alive, life, pending, badPush>>

\* The target answers the dial: the connection hears it, and now it counts.
Hear ==
    /\ conn.st = "handed" /\ Live                         \* a handed-over dial the running life holds
    /\ conn' = [conn EXCEPT !.st = "up", !.heard = TRUE]  \* heard
    /\ connected' = TRUE                                  \* the target counts as connected
    /\ UNCHANGED <<alive, life, pending, badPush>>

\* Our node learns the connection ended: the target stopped or restarted, so
\* the connection's life is gone. The target leaves the set.
Learn ==
    /\ conn.st # "none" /\ ~Live                          \* a connection the running life does not hold
    /\ conn' = NoConn                                     \* it is closed
    /\ connected' = FALSE                                 \* the target no longer counts
    /\ UNCHANGED <<alive, life, pending, badPush>>

\* The retry pass pushes an owed document while it counts the target connected.
\* Into the running life it lands; into a gone one it fails and stays owed, and
\* a failure on a connection the target never answered is counted.
Push ==
    /\ connected /\ pending > 0 /\ conn.st # "none"       \* counted, owed, and a connection to push on
    /\ IF Live
       THEN /\ pending' = pending - 1                     \* it lands
            /\ UNCHANGED badPush
       ELSE /\ badPush' = IF conn.heard THEN badPush ELSE badPush + 1  \* it fails; count it if never heard
            /\ UNCHANGED pending
    /\ UNCHANGED <<alive, life, conn, connected>>

Next == Write \/ Stop \/ Restart \/ Dial \/ Hear \/ Learn \/ Push

Spec == Init /\ [][Next]_vars

\* No push is ever made on a connection the target never answered.
INV_NoPushUnanswered == badPush = 0

\* When the target counts as connected, the connection that put it there heard it.
INV_ConnectedHeard == (connected /\ conn.st # "none") => conn.heard

\* The badPush counter stays small enough for TLC to finish on the RED run.
StateBound == badPush <= 2
====
