---- MODULE MC_GossipOwnership_Common ----
EXTENDS GossipOwnership
\* Shared values for the GossipOwnership configs.
\* Peer h is healthy and never revoked. Peer r may be revoked at any moment,
\* and gossip may keep hearing about it from others afterwards.
\* The network may drop a connection once.
mcPeers == {"h", "r"}
mcRevocable == {"r"}
\* Peer r may also run the older mux, in the configs that say so.
mcLegacy == {"r"}
====
