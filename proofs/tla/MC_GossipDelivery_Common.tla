---- MODULE MC_GossipDelivery_Common ----
\* The sizes the GossipDelivery configurations check, scaled down: gossip's
\* max_message_size is 10, the topic adds 3, a broadcast's overhead is 2, so
\* gossip carries content up to 6 and a frame may be 13 (10 without the
\* topic allowance).
EXTENDS GossipDelivery

\* Four messages: one small, one at the budget's edge (its 12-byte frame fits
\* 13 but not 10), one over the budget, and a large control message.
MCSize == [m \in {"small", "edge", "huge", "ctl"} |->
             CASE m = "small" -> 1
               [] m = "edge"  -> 6
               [] m = "huge"  -> 9
               [] m = "ctl"   -> 12]

\* The control message is the protocol's own.
MCControl == {"ctl"}
====
