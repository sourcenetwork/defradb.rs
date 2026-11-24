---- MODULE MC_EmbedderStream_Common ----
EXTENDS EmbedderStream
\* Shared values for the EmbedderStream configs.
\* Peer a opens two streams for a registered protocol: s1, whose handler
\* hangs forever, and s2, a healthy one. Peer b opens u1 with a tag nobody
\* registered. A peer may hold room for one request at a time, so s1 holding
\* its room while it hangs would turn s2 away.
mcStreams == {"s1", "s2", "u1"}
\* the two peers
mcPeers == {"a", "b"}
\* who opened which stream
mcPeerOf == [s \in mcStreams |-> IF s = "u1" THEN "b" ELSE "a"]
\* the streams with a registered tag
mcRegistered == {"s1", "s2"}
\* the stream whose handler never returns
mcStuck == {"s1"}
====
