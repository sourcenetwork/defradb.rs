---- MODULE MC_PushBacklog_Common ----
EXTENDS PushBacklog
\* Shared model values for the PushBacklog GREEN/RED configs. Three peers - one slow
\* (sends always time out) and two healthy, one of them heavy (weight 2) so the byte cap
\* is exercised independently of the item cap. Arrivals run well past the queue cap so
\* SpawnPerItem residency growth is reachable, two workers against PerPeerCap 1 so the
\* slow peer can hold at most half the pool, while the state space stays tiny. One send
\* to a healthy peer may time out, so a peer that answers again is covered.
mcPeers == {"slow", "h1", "h2"}
mcSlowPeers == {"slow"}
mcWeight == [p \in mcPeers |-> IF p = "h2" THEN 2 ELSE 1]
mcMaxArrivals == 5
mcQueueCap == 2
mcByteCap == 3
mcWorkers == 2
mcPerPeerCap == 1
mcMaxGlitches == 1

\* Audit F24: two dead peers and one healthy peer. With PerPeerCap 1 each dead peer can
\* hold one of the two workers, so together they can hold the whole pool.
mcTwoDeadPeers == {"s1", "s2", "h"}
mcTwoDeadSlowPeers == {"s1", "s2"}
mcTwoDeadWeight == [p \in mcTwoDeadPeers |-> 1]
\* The same peers with three workers and PerPeerCap 2, the shape of the real pool (8 and
\* 4): one unreachable peer may run two probes, and a reply ends its spell while its
\* other probe still runs.
mcSharedCapWorkers == 3
mcSharedCapPerPeerCap == 2

\* No dead peer: two healthy peers, and one of their sends may time out once.
mcHealthyPeers == {"h1", "h2"}
mcNoSlowPeers == {}
mcHealthyWeight == [p \in mcHealthyPeers |-> 1]
====
