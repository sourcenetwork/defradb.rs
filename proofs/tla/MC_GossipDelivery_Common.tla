---- MODULE MC_GossipDelivery_Common ----
\* The sizes the GossipDelivery configurations check, scaled down: a frame
\* may be 13, the topic adds 3 and a broadcast's overhead is 2. Gossip's
\* max_message_size is 13 - 3 = 10, so it carries content up to 6 (with the
\* mismatch its size is 13, and content up to 9 gets through).
EXTENDS GossipDelivery

\* Four messages: one small, one at the budget (its frame is 12), one over
\* the budget (a 15-byte frame), and a large control message.
MCSize == [m \in {"small", "edge", "huge", "ctl"} |->
             CASE m = "small" -> 1
               [] m = "edge"  -> 6
               [] m = "huge"  -> 9
               [] m = "ctl"   -> 12]

\* The control message is the protocol's own.
MCControl == {"ctl"}
====
