---- MODULE MC_QuorumCertificate_Common ----
EXTENDS QuorumCertificate
\* Four validators, one of them cheating (N = 3F + 1 with F = 1), and two
\* contenders for one slot.

mcHonest == {"v1", "v2", "v3"}
mcCheaters == {"b"}
mcContenders == {"c1", "c2"}
====
