---- MODULE MC_SenderBroadcast_Common ----
\* Shared values for the SenderBroadcast configs: three writes against a
\* pending set of two and one publish slot, so a commit can find either full.
EXTENDS SenderBroadcast
\* Three committed writes, each announced once.
mcWrites == {1, 2, 3}
====
