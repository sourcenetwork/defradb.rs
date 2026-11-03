----------------------- MODULE MC_P2PDelivery_Receiver -----------------------
\* The receiver shape: one hub h, just restarted, with durable records for
\* three roots from three providers and a pool of one fetch slot.
\*   x: written on desktop a, which is connected and serves it. It must merge.
\*   t: written on catch-up peer c, which stays connected and sends blocks for
\*      ever without finishing the DAG (a productive, never-ending fetch).
\*   u: written on d, which never connects again (a dead provider).
\* Every root was acknowledged before the restart, so no sender holds a
\* marker: the durable record on h is the only owner. The world picks one of
\* two backlogs: {u, x} fits the map (two places), {u, t, x} does not, so the
\* sweep must rotate. Advisory pull fetches read from c, catch-up traffic can
\* fill h's merge channel, and up to two local merge continuations can take
\* places in the map. Nothing crashes and no connection drops: the restart
\* already happened, and this shape asks only how the pool, its permits and
\* the sweep treat x.
EXTENDS P2PDelivery

\* The hub, its three providers.
rNodes == {"a", "c", "d", "h"}
\* Each provider replicates to the hub.
rRepl == {<<"a", "h">>, <<"c", "h">>, <<"d", "h">>}
\* Three independent roots.
rHeads == {"u", "t", "x"}
\* Who wrote each root.
rAuthor == [hd \in rHeads |-> CASE hd = "u" -> "d" [] hd = "t" -> "c" [] hd = "x" -> "a"]
\* No root has a parent.
rParent == [hd \in rHeads |-> {}]
\* Every root has priority 1.
rPrio == [hd \in rHeads |-> 1]
\* CID order puts the dead root first and x last, the order that starves x.
rKeyOrder == <<"u", "t", "x">>
\* No shared ancestor.
rBase == {}
\* Nothing new is written.
rFresh == {}
\* The two backlogs the hub may restart with.
rBacklogs == {[n \in rNodes |-> IF n = "h" THEN bl ELSE {}] : bl \in {{"u", "x"}, {"u", "t", "x"}}}
\* t never finishes.
rTrickle == {"t"}
\* d never connects.
rDead == {"d"}
\* Pull fetches read from c.
rPullPeer == "c"
\* The hub runs pull fetches and continuations.
rPullAt == {"h"}
\* The hub's merge channel can fill.
rStorm == {"h"}
\* Nobody crashes.
rCrashable == {}
\* Live pairs are connected at the start.
rConnectedAtStart == "full"
=============================================================================
