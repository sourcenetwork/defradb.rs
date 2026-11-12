---- MODULE MC_JailRevocation_Common ----
EXTENDS JailRevocation
\* Two linked nodes, n1 and n2. The cheater can make two keys but the admin
\* admits only one of them (Budget 1). The honest writer "h" writes on n1.

mcNodes == {"n1", "n2"}
mcPeers == [n \in mcNodes |-> mcNodes \ {n}]
mcCheaterKeys == {"k1", "k2"}
mcHonestKey == "h"
mcWriterHome == "n1"
mcBudget == 1
====
