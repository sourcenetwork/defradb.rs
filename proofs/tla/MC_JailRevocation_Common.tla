---- MODULE MC_JailRevocation_Common ----
EXTENDS JailRevocation
\* Two linked nodes, n1 and n2. The cheater can make two keys but the admin
\* admits only one of them (Budget 1). The honest writer "h" writes on n1; a
\* restore gives it "h2", unless the restore keeps the old key.

mcNodes == {"n1", "n2"}
mcPeers == [n \in mcNodes |-> mcNodes \ {n}]
mcCheaterKeys == {"k1", "k2"}
mcHonestKey == "h"
mcRestoreKey == "h2"
mcWriterHome == "n1"
mcBudget == 1
\* The restore scenario keeps one cheater key, so it stays small enough to check.
mcOneCheaterKey == {"k1"}
====
