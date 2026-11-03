-------------------------- MODULE MC_P2PDelivery_E2E --------------------------
\* The end-to-end shape: desktop a replicates to hub h, and h replicates to
\* spoke b and back to a. One document, whose first head g every node has
\* merged already, and two heads written during the run on top of it:
\*   ya: written on a.
\*   yb: written on h, before h merges ya.
\* ya and yb are siblings with the same priority, and h holds both once it
\* merges ya. h forwards what it merges to b and a, and its own write fans out
\* to both, so one commit registers markers for two peers. The world may
\* crash a or h once, fail one marker registration, and choose how each
\* connection comes up. Fetch pools are one slot wide and every map has room
\* for every head: this shape asks only whether the hand-offs keep each head
\* owed and get it everywhere.
EXTENDS P2PDelivery

\* Desktop, hub, spoke.
eNodes == {"a", "h", "b"}
\* a to h, h to b, and h back to a.
eRepl == {<<"a", "h">>, <<"h", "b">>, <<"h", "a">>}
\* One document's heads.
eHeads == {"g", "ya", "yb"}
\* g and ya on the desktop, yb on the hub.
eAuthor == [hd \in eHeads |-> IF hd = "yb" THEN "h" ELSE "a"]
\* ya and yb both sit on g.
eParent == [hd \in eHeads |-> IF hd = "g" THEN {} ELSE {"g"}]
\* g has priority 1, both siblings 2.
ePrio == [hd \in eHeads |-> IF hd = "g" THEN 1 ELSE 2]
\* CID order: ya sorts before yb.
eKeyOrder == <<"g", "ya", "yb">>
\* Every node has merged g.
eBase == {"g"}
\* ya and yb are written during the run.
eFresh == {"ya", "yb"}
\* Nobody starts with a durable record.
eBacklogs == {[n \in eNodes |-> {}]}
\* Every provider finishes.
eTrickle == {}
\* Nobody is dead.
eDead == {}
\* No pull fetches.
ePullPeer == "none"
\* Nobody runs pull fetches or continuations.
ePullAt == {}
\* No merge channel fills.
eStorm == {}
\* The desktop and the hub may crash.
eCrashable == {"a", "h"}
\* Every pair starts disconnected, so the first contact decides how it connects.
eConnectedAtStart == "down"
=============================================================================
