---- MODULE PincPeerExchange ----
\* The story, in plain words.
\*
\* Two peers, B and C, each connect to a middle node M over pinc. Neither
\* knows the other. When a peer connects, M hands it the peers M already
\* has (a peer exchange), and the peer dials each one it learns, so B and C
\* end up connected to each other.
\*
\* Each peer also sends M its signed record, which later lets anyone dial it
\* by id. A node that names hubs can sign its record only once pinc has
\* settled it on a relay, which happens seconds after it connects. Until
\* then pinc answers "busy, try again shortly".
\*
\* The rules:
\*   1. the exchange hands over each peer with the address M's connection to
\*      it runs to, so a peer can be dialed before its record exists;
\*   2. a peer asks pinc for its record again until pinc has settled, then
\*      sends it.
\*
\* A tiny example. B connects to M; B is not settled, so it has no record
\* yet. C connects to M; M hands C the address of B; C dials B, and B and C
\* are connected. Later B and C settle, ask again, and M gets both records.
\*
\* What must always happen in the end:
\*   1. B and C are connected to each other - LIVE_PeersMeet;
\*   2. M holds the records of both peers - LIVE_RecordsKept.
\*
\* Correspondence with the Rust code:
\*
\*   | Model        | Rust                                                        |
\*   |--------------|-------------------------------------------------------------|
\*   | Connect      | crates/database/pinc/src/endpoint.rs Node::connected:   |
\*   |              | spawns send_record, and share_peers for a new peer          |
\*   | shared       | crates/database/pinc/src/exchange.rs Node::share_peers: |
\*   |              | each other peer with its route address and its record       |
\*   | Learn        | exchange.rs Node::learn_peers: dial by the address          |
\*   | Settle       | pinc's relay client classifies the node and homes it        |
\*   | SendRecord   | crates/database/pinc/src/serve.rs Node::send_record,    |
\*   |              | once_settled: signed_record again while pinc says busy      |
\*
\* RED configurations:
\*   - RecordOnly = TRUE is the exchange before rule 1: it hands over only the
\*     peers whose record M holds. C connects before B settles, M has nothing
\*     to hand over, and B and C never meet (LIVE_PeersMeet).
\*   - NoRetry = TRUE is the record send before rule 2: it asks pinc once, at
\*     connect. B connects before it settles, the one ask is refused busy, and
\*     M never gets B's record (LIVE_RecordsKept).
EXTENDS Naturals

CONSTANTS
    RecordOnly,   \* the exchange hands over only peers with a record (the first bug)
    NoRetry       \* a peer asks pinc for its record once, at connect (the second bug)

\* The two peers of the middle node.
Peers == {"B", "C"}

\* The other peer of `p`.
Other(p) == IF p = "B" THEN "C" ELSE "B"

VARIABLES
    linked,    \* linked[p]: peer p is connected to M
    settled,   \* settled[p]: pinc has settled peer p on a relay, so it can sign its record
    rec,       \* rec[p]: M holds peer p's signed record
    gaveUp,    \* gaveUp[p]: peer p asked pinc once and will not ask again (only under NoRetry)
    shared,    \* the peers M handed over and not yet dialed: pairs <<to, about>>
    met        \* B and C are connected to each other

vars == <<linked, settled, rec, gaveUp, shared, met>>

\* Every variable has the shape it should.
TypeOK ==
    /\ linked \in [Peers -> BOOLEAN]       \* each peer connected to M or not
    /\ settled \in [Peers -> BOOLEAN]      \* each peer settled or not
    /\ rec \in [Peers -> BOOLEAN]          \* M holds each record or not
    /\ gaveUp \in [Peers -> BOOLEAN]       \* each peer stopped asking or not
    /\ shared \subseteq (Peers \X Peers)   \* handed-over peers, by receiver and subject
    /\ met \in BOOLEAN                     \* B and C connected or not

Init ==
    /\ linked = [p \in Peers |-> FALSE]    \* nobody is connected yet
    /\ settled = [p \in Peers |-> FALSE]   \* nobody is settled yet
    /\ rec = [p \in Peers |-> FALSE]       \* M holds no record
    /\ gaveUp = [p \in Peers |-> FALSE]    \* nobody stopped asking
    /\ shared = {}                         \* nothing handed over
    /\ met = FALSE                         \* B and C are strangers

\* Whether M hands peer q over: q is connected to M, and, under the first bug,
\* M already holds q's record.
Handed(q) == linked[q] /\ (RecordOnly => rec[q])

\* Peer p connects to M. M hands p its other peer when it can. Under the
\* second bug p asks pinc for its record once, now: it lands only if p is
\* settled, and either way p does not ask again.
Connect(p) ==
    /\ ~linked[p]                                                   \* p is not connected yet
    /\ linked' = [linked EXCEPT ![p] = TRUE]                        \* now it is
    /\ shared' = IF Handed(Other(p))                                \* M has a peer to hand over
                 THEN shared \cup {<<p, Other(p)>>}                 \* p learns it
                 ELSE shared                                        \* p learns nothing
    /\ IF NoRetry
       THEN /\ rec' = [rec EXCEPT ![p] = settled[p]]                \* the one ask lands only if settled
            /\ gaveUp' = [gaveUp EXCEPT ![p] = TRUE]                \* and is never repeated
       ELSE UNCHANGED <<rec, gaveUp>>                               \* the record goes by SendRecord
    /\ UNCHANGED <<settled, met>>

\* pinc settles peer p on a relay: from now on p can sign its record.
Settle(p) ==
    /\ ~settled[p]                                     \* not settled yet
    /\ settled' = [settled EXCEPT ![p] = TRUE]         \* settled now
    /\ UNCHANGED <<linked, rec, gaveUp, shared, met>>

\* Peer p, connected to M and settled, asks pinc again and sends M its record.
\* A peer that gave up never asks again.
SendRecord(p) ==
    /\ linked[p] /\ settled[p] /\ ~rec[p] /\ ~gaveUp[p]   \* connected, settled, not yet sent, still asking
    /\ rec' = [rec EXCEPT ![p] = TRUE]                     \* M holds the record
    /\ UNCHANGED <<linked, settled, gaveUp, shared, met>>

\* Peer p dials the peer M handed it, by its address (or its record), and the
\* two are connected.
Learn(p) ==
    /\ <<p, Other(p)>> \in shared                          \* p was handed its other peer
    /\ shared' = shared \ {<<p, Other(p)>>}                \* the hand-over is used up
    /\ met' = TRUE                                         \* B and C are connected
    /\ UNCHANGED <<linked, settled, rec, gaveUp>>

Next == \E p \in Peers : Connect(p) \/ Settle(p) \/ SendRecord(p) \/ Learn(p)

\* Every step that can keep happening does happen: peers connect, pinc settles,
\* records are sent, hand-overs are dialed.
Fairness == \A p \in Peers :
    /\ WF_vars(Connect(p))
    /\ WF_vars(Settle(p))
    /\ WF_vars(SendRecord(p))
    /\ WF_vars(Learn(p))

Spec == Init /\ [][Next]_vars /\ Fairness

\* In the end B and C are connected to each other.
LIVE_PeersMeet == <>met

\* In the end M holds the records of both peers.
LIVE_RecordsKept == <>(\A p \in Peers : rec[p])
====
