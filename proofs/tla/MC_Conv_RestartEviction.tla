---- MODULE MC_Conv_RestartEviction ----
EXTENDS Convergence

\* Same DAG as MC_Conv_Eventual, but the cfg enables one arbitrary restart per
\* node, which evicts its in-memory pending map; the durable records bring the
\* roots back.
mcNodes   == {"n1", "n2"}
mcBlocks  == {"b0", "b1", "b2"}
mcParents == [b \in mcBlocks |->
                CASE b = "b1" -> {"b0"}
                  [] b = "b2" -> {"b0"}
                  [] OTHER    -> {}]
mcHeads   == {"b1", "b2"}
mcInitialConnected == {}
====
