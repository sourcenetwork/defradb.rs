---- MODULE MC_Conv_NoHeadRediscovery ----
EXTENDS Convergence

\* Red scenario: n1 only published its heads while n2 was away, so no retry
\* marker and no pending record owes them. The transport reconnects, but
\* nothing rediscovers heads on connect, so n2 never learns them: eventual
\* connectivity alone is not enough.
mcNodes   == {"n1", "n2"}
mcBlocks  == {"b0", "b1", "b2"}
mcParents == [b \in mcBlocks |->
                CASE b = "b1" -> {"b0"}
                  [] b = "b2" -> {"b0"}
                  [] OTHER    -> {}]
mcHeads   == {"b1", "b2"}
mcInitialConnected == {}
====
