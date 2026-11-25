---- MODULE MC_Auth_Red_PeerOnly ----
EXTENDS Auth
\* RED: a hypothetical pinc management stream is marked as mutating but only
\* admits a transport peer. PeerID alone must not authorize node configuration.

mcRequests == {"r_peer"}
mcActors == {"adv"}
mcPermissions == {"P2pCollectionAdd"}
mcEntries == {"PincMgmtStream"}

mcEntryOf == [r \in mcRequests |-> "PincMgmtStream"]
mcPresentedActor == [r \in mcRequests |-> "adv"]
mcRequiredPermission == [r \in mcRequests |-> "P2pCollectionAdd"]
mcInitialCredential == [r \in mcRequests |-> "absent"]
mcInitialGrants == {}
mcMutableGrantPairs == {}
mcGateByEntry == [e \in mcEntries |-> "PeerGate"]
mcEntryCanMutate == [e \in mcEntries |-> TRUE]
====
