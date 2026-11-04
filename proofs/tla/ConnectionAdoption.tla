------------------------- MODULE ConnectionAdoption -------------------------
\* This node gets a new connection to a peer in one of three ways: it accepts
\* one, it dials one on purpose, or an RPC dials one on a cache miss. Each new
\* connection is adopted once: counted in the peer map, published as the
\* peer's shared RPC path, rechecked against admission, handed to a pump that
\* serves the streams the peer opens on it, and announced with PeerConnected.
\* That announcement can wait on a full event lane, and the adopt can be
\* dropped while it waits. The peer may already hold a sibling connection this
\* node dialed. What must never happen: an open shared path nobody serves, a
\* count or cache entry nobody releases, or an inbound allowlist miss that
\* costs the sibling. Revocation races are modeled in SharedRpcConnection and
\* GossipOwnership.
\*
\* Code: crates/p2p/src/iroh/endpoint_streams.rs adopt_new_connection,
\* finish_connection and handle_incoming; endpoint_rpc.rs connect_with_cache,
\* remember_connection and restore_displaced_connection; endpoint_commands.rs
\* handle_dial.
EXTENDS TLC, Integers, Sequences
\* Each switch is one safeguard; a RED configuration turns exactly one off.
CONSTANTS AdoptRpcDial, PumpBeforeAnnounce, CleanupRefusedPump, HitSkipsAdoption, NarrowRefusal, RestoreSibling
\* The scenario, fixed at the start: how the connection arose, whether the
\* allowlist admits the peer, whether tasks can still be spawned, and whether
\* a sibling exists. A later connection may also become the sibling.
VARIABLES origin, allowed, registryOpen, sibling,
          \* How many of the adopt's steps ran, and whether it was dropped.
          step, cancelled,
          \* The new connection: open or closed, its peer-map count, refused or not.
          open, count, refused,
          \* The peer's shared RPC slot, and what the publish displaced from it.
          cached, displaced,
          \* Whether the sibling connection is still open.
          siblingOpen,
          \* The pump: registered, and whether it has run its exit path.
          pumped, exited,
          \* Whether a later RPC already reused the connection from the cache.
          hit
\* Every variable, for stuttering steps.
vars == <<origin, allowed, registryOpen, sibling, step, cancelled, open, count, refused,
          cached, displaced, siblingOpen, pumped, exited, hit>>

\* The adopt's steps in code order; the RED order announces before pumping.
Order == IF PumpBeforeAnnounce THEN <<"publish", "check", "spawn", "announce">>
                               ELSE <<"publish", "check", "announce", "spawn">>
\* Whether this connection goes through adoption at all; the pre-fix RPC dial did not.
Adopted == origin # "rpc" \/ AdoptRpcDial
\* An unadopted RPC dial only publishes what it dialed.
Steps == IF Adopted THEN Order ELSE <<"publish">>
\* The adopt has returned: every step ran, or a closed registry ended it early.
Finished == step = Len(Steps)
\* The adopt is still running and its next step is `s`.
Due(s) == ~cancelled /\ ~Finished /\ Steps[step + 1] = s
\* The adopt has stopped changing the slot: returned, dropped, or waiting to announce.
Idle == cancelled \/ Finished \/ Steps[step + 1] = "announce"
\* Only an inbound connection answers to the allowlist.
Miss == origin = "accept" /\ ~allowed

\* Nothing has happened yet, in every possible scenario.
Init == /\ origin \in {"accept", "dial", "rpc"}
        \* The allowlist may or may not admit this peer.
        /\ allowed \in BOOLEAN
        \* The endpoint may already be shutting down its task registry.
        /\ registryOpen \in BOOLEAN
        \* The peer may already hold a sibling this node dialed.
        /\ sibling \in BOOLEAN
        \* No step of the adopt has run.
        /\ step = 0
        \* The adopt has not been dropped.
        /\ cancelled = FALSE
        \* The new connection is not published yet.
        /\ open = FALSE
        \* Nothing counts it yet.
        /\ count = 0
        \* Nothing has refused it yet.
        /\ refused = FALSE
        \* The shared slot holds the sibling, if there is one.
        /\ cached = IF sibling THEN "sibling" ELSE "none"
        \* No publish has displaced anything yet.
        /\ displaced = "none"
        \* A sibling that exists starts open.
        /\ siblingOpen = sibling
        \* No pump has been registered.
        /\ pumped = FALSE
        \* So no pump has exited.
        /\ exited = FALSE
        \* No RPC has reused the connection.
        /\ hit = FALSE

\* Count the connection and make it the peer's shared RPC path.
Publish == /\ Due("publish")
           \* The adopt moves past its first step.
           /\ step' = step + 1
           \* The connection is now live.
           /\ open' = TRUE
           \* Only an adopted connection is counted in the peer map.
           /\ count' = IF Adopted THEN 1 ELSE 0
           \* Remember what the insert replaced, so a refusal can put it back.
           /\ displaced' = cached
           \* The new connection takes the shared slot.
           /\ cached' = "new"
           \* The scenario, the sibling and the pump are untouched.
           /\ UNCHANGED <<origin, allowed, registryOpen, sibling, cancelled, refused, siblingOpen, pumped, exited, hit>>

\* Recheck admission after publishing; an inbound allowlist miss is refused.
Check == /\ Due("check")
         \* The adopt moves past the recheck.
         /\ step' = step + 1
         \* Record whether this connection was refused.
         /\ refused' = Miss
         \* A refused connection is closed.
         /\ open' = (open /\ ~Miss)
         \* A narrow refusal spares the sibling; a peer-wide one closes it too.
         /\ siblingOpen' = (siblingOpen /\ (NarrowRefusal \/ ~Miss))
         \* A peer-wide close empties the slot; a narrow one puts the displaced sibling back.
         /\ cached' = CASE Miss /\ ~NarrowRefusal -> "none"
                        [] Miss /\ RestoreSibling /\ displaced = "sibling" -> "sibling"
                        [] OTHER -> cached
         \* The scenario, the count and the pump are untouched.
         /\ UNCHANGED <<origin, allowed, registryOpen, sibling, cancelled, count, displaced, pumped, exited, hit>>

\* Register the pump that serves the connection and owns its release.
Spawn == /\ Due("spawn")
         /\ IF registryOpen
              \* The pump now owns the connection.
              THEN /\ pumped' = TRUE
                   \* The adopt moves past this step.
                   /\ step' = step + 1
                   \* Spawning closes and releases nothing.
                   /\ UNCHANGED <<open, count, cached>>
              \* A closed registry ends the adopt, which closes the connection.
              ELSE /\ step' = Len(Steps)
                   \* Only its own handle is closed.
                   /\ open' = FALSE
                   \* The guarded adopt releases the count itself, since no pump will.
                   /\ count' = IF CleanupRefusedPump THEN count - 1 ELSE count
                   \* And it evicts its own cache entry the same way.
                   /\ cached' = IF CleanupRefusedPump /\ cached = "new" THEN "none" ELSE cached
                   \* No pump was registered.
                   /\ UNCHANGED pumped
         \* The scenario and the sibling are untouched.
         /\ UNCHANGED <<origin, allowed, registryOpen, sibling, cancelled, refused, displaced, siblingOpen, exited, hit>>

\* The PeerConnected send completes; a refused connection skips it.
Announce == /\ Due("announce")
            \* The adopt moves past this step.
            /\ step' = step + 1
            \* Announcing changes no connection state.
            /\ UNCHANGED <<origin, allowed, registryOpen, sibling, cancelled, open, count, refused, cached, displaced, siblingOpen, pumped, exited, hit>>

\* The adopt is dropped while its PeerConnected send waits on a full lane.
Cancel == /\ Due("announce")
          \* A refused connection never waits to be announced.
          /\ ~refused
          \* The adopt stops here, whatever steps remain.
          /\ cancelled' = TRUE
          \* Dropping it closes and releases nothing by itself.
          /\ UNCHANGED <<origin, allowed, registryOpen, sibling, step, open, count, refused, cached, displaced, siblingOpen, pumped, exited, hit>>

\* The peer, or this node, can close a published connection at any time.
Close == /\ step >= 1
         \* Only an open connection can close.
         /\ open
         \* It is closed from now on.
         /\ open' = FALSE
         \* Closing it touches no bookkeeping by itself.
         /\ UNCHANGED <<origin, allowed, registryOpen, sibling, step, cancelled, count, refused, cached, displaced, siblingOpen, pumped, exited, hit>>

\* The pump sees its connection close and releases it, once.
PumpExit == /\ pumped
            \* It exits only when the connection has closed.
            /\ ~open
            \* And only once.
            /\ ~exited
            \* Record the exit.
            /\ exited' = TRUE
            \* The pump drops the count it owns.
            /\ count' = count - 1
            \* And evicts only its own cache entry.
            /\ cached' = IF cached = "new" THEN "none" ELSE cached
            \* Everything else is untouched.
            /\ UNCHANGED <<origin, allowed, registryOpen, sibling, step, cancelled, open, refused, displaced, siblingOpen, pumped, hit>>

\* A later RPC finds the open connection in the cache after the adopt returned.
Hit == /\ Finished
       \* The cached handle is this connection, and it is still open.
       /\ cached = "new" /\ open
       \* One reuse is enough to show whether a hit is counted again.
       /\ ~hit
       \* Record the reuse.
       /\ hit' = TRUE
       \* A hit returns the handle; adopting it again would count it twice.
       /\ count' = IF HitSkipsAdoption THEN count ELSE count + 1
       \* Everything else is untouched.
       /\ UNCHANGED <<origin, allowed, registryOpen, sibling, step, cancelled, open, refused, cached, displaced, siblingOpen, pumped, exited>>

\* Another connection takes the shared slot while the RPC connection stays open.
Replace == /\ Finished /\ origin = "rpc" /\ ~refused /\ open
           /\ ~sibling /\ cached = "new"
           /\ sibling' = TRUE /\ siblingOpen' = TRUE /\ cached' = "sibling"
           /\ UNCHANGED <<origin, allowed, registryOpen, step, cancelled, open, count, refused, displaced, pumped, exited, hit>>

\* Revocation closes every retained handle, including a displaced sibling.
Revoke == /\ Finished /\ ~refused /\ open
          /\ open' = FALSE
          /\ siblingOpen' = FALSE
          /\ cached' = "none"
          /\ UNCHANGED <<origin, allowed, registryOpen, sibling, step, cancelled, count, refused, displaced, pumped, exited, hit>>

\* Any of these may happen next, in any order the guards allow.
Next == Publish \/ Check \/ Spawn \/ Announce \/ Cancel \/ Close \/ PumpExit \/ Hit \/ Replace \/ Revoke
\* The whole behavior: start, then take steps.
Spec == Init /\ [][Next]_vars

\* Every open new connection remains counted even after a sibling takes the shared slot.
OpenIsCounted == open => count >= 1
\* Once the adopt stops changing the slot, an open shared path has a pump serving it.
Served == (cached = "new" /\ open /\ Idle) => pumped
\* A closed connection with no pump left to run holds no count and no cache entry.
Released == (~open /\ (Finished \/ cancelled) /\ (pumped => exited)) => (count = 0 /\ cached # "new")
\* An inbound allowlist refusal leaves the sibling open and the shared RPC path.
SiblingKept == (refused /\ sibling) => (siblingOpen /\ cached = "sibling")
=============================================================================
