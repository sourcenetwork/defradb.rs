---- MODULE ExplicitDisconnect ----
\* The story, in plain words.
\*
\* Three peers can exchange lists naming the other two. A list may arrive
\* after one peer deliberately disconnects from another. Discovery may then
\* look at that list, but it must not redial a peer that this node put on its
\* explicit-disconnect list. Only this node's own explicit dial undoes that
\* marker. A marked inbound libp2p connection is refused until the local
\* peer redials.
\*
\* The state bound keeps one marker, one in-flight list, and one outstanding
\* dial. That is enough for the race while keeping TLC under its time budget;
\* the invariant is quantified over every peer pair.
\*
\* The Rust code this models:
\*   - the explicit-disconnect marker            crates/database/p2p/src/behaviour/disconnect_gate.rs DisconnectGate
\*   - discovery excluding marked peers          crates/database/p2p/src/host/p2p_host/mod.rs run_kademlia_bootstrap
\*   - background dials checking the marker       crates/database/p2p/src/host/command_handler/network.rs handle_dial
\*   - explicit dials clearing the marker        crates/database/p2p/src/host/command_handler/network.rs handle_explicit_dial
\*   - the caller's explicit connect             crates/surfaces/p2p-adapter/src/libp2p.rs connect_peer
\*   - replicator registration clearing it       crates/database/p2p/src/host/command_handler/mod.rs handle_command/CreateReplicator
\*   - marked inbound connections being refused crates/database/p2p/src/behaviour/disconnect_gate.rs handle_established_inbound_connection

EXTENDS Naturals, FiniteSets

CONSTANTS
  Nodes,    \* the three peers in the network
  Half,     \* "Libp2p" or "Vinc": which redial path is under test
  Bug       \* "None", "IgnoresSticky", "ClearsSticky", "AcceptsInbound",
            \* "AcceptsBackgroundInbound", or "RefusesAllowed"

NodePairs == Nodes \X Nodes

VARIABLES
  links,           \* unordered live connections, stored in both directions
  markers,         \* peers this node disconnected on purpose
  obligations,     \* deliberate closes not yet cleared by an explicit connect
  known,           \* peers learned from exchanged lists
  pendingLists,    \* lists in flight: <<receiver, sender, learned peer>>
  backgroundDials, \* discovery dials launched since the last explicit event
  backgroundLinks, \* discovery dials the recipient accepted, live until the link goes
  remoteDials,     \* accepted remote dials
  remoteAttempts,  \* remote attempts, whether accepted or refused
  permittedDials   \* attempts permitted when made, which must produce a link

vars == <<links, markers, obligations, known, pendingLists, backgroundDials, backgroundLinks, remoteDials, remoteAttempts, permittedDials>>

TypeOK ==
  /\ links \subseteq NodePairs
  /\ markers \subseteq NodePairs
  /\ obligations \subseteq NodePairs
  /\ known \in [Nodes -> SUBSET Nodes]
  /\ pendingLists \subseteq Nodes \X Nodes \X Nodes
  /\ backgroundDials \subseteq NodePairs
  /\ backgroundLinks \subseteq NodePairs
  /\ remoteDials \subseteq NodePairs
  /\ remoteAttempts \subseteq NodePairs
  /\ permittedDials \subseteq NodePairs

Linked(a, b) == <<a, b>> \in links \/ <<b, a>> \in links

Marked(a, b) == <<a, b>> \in markers

Init ==
  /\ links = {}
  /\ markers = {}
  /\ obligations = {}
  /\ known = [peer \in Nodes |-> {}]
  /\ pendingLists = {}
  /\ backgroundDials = {}
  /\ backgroundLinks = {}
  /\ remoteDials = {}
  /\ remoteAttempts = {}
  /\ permittedDials = {}

\* A caller explicitly connects to a peer. This is the local action that
\* clears an earlier deliberate disconnect, even when a link already exists.
\* Whether a link already exists is read in this same step: the host answers
\* it while handling the explicit dial, so the caller never acts on a link
\* that closed before the dial ran.
ExplicitConnect(a, b) ==
  /\ a # b
  /\ markers' = markers \ {<<a, b>>}
  /\ obligations' = obligations \ {<<a, b>>}
  /\ IF Linked(a, b) \/ (Half = "Libp2p" /\ Marked(b, a))
       THEN /\ UNCHANGED <<links, known>>
       ELSE /\ links' = links \cup {<<a, b>>, <<b, a>>}
            /\ known' = [known EXCEPT ![a] = known[a] \cup {b}, ![b] = known[b] \cup {a}]
  /\ UNCHANGED <<pendingLists, backgroundDials, backgroundLinks, remoteDials, remoteAttempts, permittedDials>>

\* The remote peer explicitly dials in. The local marker stays: accepting the
\* connection is not the local caller saying "resume discovery".
RemoteExplicitDial(a, b) ==
  /\ a # b
  /\ ~Linked(a, b)
  /\ Cardinality(remoteAttempts) < 1
  /\ LET permitted == Half # "Libp2p" \/ ~Marked(b, a)
     IN /\ remoteAttempts' = remoteAttempts \cup {<<a, b>>}
        /\ permittedDials' = IF permitted THEN permittedDials \cup {<<a, b>>} ELSE permittedDials
        /\ IF (permitted /\ Bug # "RefusesAllowed") \/ Bug = "AcceptsInbound"
             THEN /\ links' = links \cup {<<a, b>>, <<b, a>>}
                  /\ known' = [known EXCEPT ![a] = known[a] \cup {b}, ![b] = known[b] \cup {a}]
                  /\ remoteDials' = remoteDials \cup {<<a, b>>}
             ELSE /\ UNCHANGED <<links, known, remoteDials>>
  /\ UNCHANGED <<markers, obligations, pendingLists, backgroundDials, backgroundLinks>>

\* One connected peer sends a list naming a third peer.
Exchange(a, b, learned) ==
  /\ a # b
  /\ learned # a
  /\ learned # b
  /\ Linked(a, b)
  /\ Cardinality(pendingLists) < 1
  /\ pendingLists' = pendingLists \cup {<<b, a, learned>>}
  /\ UNCHANGED <<links, markers, obligations, known, backgroundDials, backgroundLinks, remoteDials, remoteAttempts, permittedDials>>

\* A list in flight arrives. Learning a name does not itself dial anybody.
LearnList ==
  /\ \E list \in pendingLists :
        /\ list[3] # list[1]
        /\ known' = [known EXCEPT ![list[1]] = known[list[1]] \cup {list[3]}]
        /\ pendingLists' = pendingLists \ {list}
  /\ UNCHANGED <<links, markers, obligations, backgroundDials, backgroundLinks, remoteDials, remoteAttempts, permittedDials>>

\* A normal link loss is not deliberate. It preserves any marker.
DropLink(a, b) ==
  /\ a # b
  /\ Linked(a, b)
  /\ links' = links \ {<<a, b>>, <<b, a>>}
  /\ remoteDials' = remoteDials \ {<<a, b>>, <<b, a>>}
  /\ remoteAttempts' = remoteAttempts \ {<<a, b>>, <<b, a>>}
  /\ permittedDials' = permittedDials \ {<<a, b>>, <<b, a>>}
  /\ backgroundLinks' = backgroundLinks \ {<<a, b>>, <<b, a>>}
  /\ UNCHANGED <<markers, obligations, known, pendingLists, backgroundDials>>

\* The caller explicitly disconnects. The marker survives later lists and
\* ordinary link losses; only an explicit local action removes it.
ExplicitDisconnect(a, b) ==
  /\ a # b
  /\ Cardinality(markers) < 1
  /\ markers' = markers \cup {<<a, b>>}
  /\ obligations' = obligations \cup {<<a, b>>}
  /\ links' = links \ {<<a, b>>, <<b, a>>}
  /\ remoteDials' = remoteDials \ {<<a, b>>, <<b, a>>}
  /\ remoteAttempts' = remoteAttempts \ {<<a, b>>, <<b, a>>}
  /\ permittedDials' = permittedDials \ {<<a, b>>, <<b, a>>}
  /\ backgroundDials' = {dial \in backgroundDials : dial[1] # a}
  /\ backgroundLinks' = backgroundLinks \ {<<a, b>>, <<b, a>>}
  /\ UNCHANGED <<known, pendingLists>>

\* Discovery checks the marker before dialing. The RED configurations either
\* ignore the marker (the pre-fix race) or let a background dial clear it (the
\* pre-fix generic Dial path).
\* The recipient runs the same inbound gate as for an explicit dial: in
\* libp2p, when the recipient marked the dialer, the attempt is recorded but
\* no link forms and nobody learns anything new.
BackgroundDial(a, b) ==
  /\ a # b
  /\ b \in known[a]
  /\ ~Linked(a, b)
  /\ Cardinality(backgroundDials) < 1
  /\ IF Bug \in {"IgnoresSticky", "ClearsSticky"}
       THEN TRUE
       ELSE ~Marked(a, b)
  /\ backgroundDials' = backgroundDials \cup {<<a, b>>}
  /\ IF Bug = "ClearsSticky"
       THEN markers' = markers \ {<<a, b>>}
       ELSE UNCHANGED markers
  \* refused: libp2p mode, the recipient marked the dialer, and no RED
  \* configuration plants a gate that lets it through
  /\ IF /\ Half = "Libp2p"
        /\ Marked(b, a)
        /\ Bug \notin {"AcceptsInbound", "AcceptsBackgroundInbound"}
       THEN UNCHANGED <<links, known, backgroundLinks>>
       ELSE /\ links' = links \cup {<<a, b>>, <<b, a>>}
            /\ known' = [known EXCEPT ![a] = known[a] \cup {b}, ![b] = known[b] \cup {a}]
            /\ backgroundLinks' = backgroundLinks \cup {<<a, b>>}
  /\ UNCHANGED <<pendingLists, remoteDials, remoteAttempts, permittedDials, obligations>>

Next ==
  \/ \E a, b \in Nodes : ExplicitConnect(a, b)
  \/ \E a, b \in Nodes : RemoteExplicitDial(a, b)
  \/ \E a, b, learned \in Nodes : Exchange(a, b, learned)
  \/ LearnList
  \/ \E a, b \in Nodes : DropLink(a, b)
  \/ \E a, b \in Nodes : ExplicitDisconnect(a, b)
  \/ \E a, b \in Nodes : BackgroundDial(a, b)

Spec == Init /\ [][Next]_vars /\ WF_vars(LearnList)

\* No background dial exists for a peer the local node deliberately
\* disconnected. Explicit local actions remove the marker before dialing.
INV_NoRedialAfterDeliberateClose ==
  \A a, b \in Nodes :
     <<a, b>> \in obligations => <<a, b>> \notin backgroundDials

\* Record each remote attempt, including refusal. A permitted attempt must
\* connect; accepting an inbound connection never clears the local marker.
INV_RemoteExplicitDialAccepted ==
  \A dial \in permittedDials :
     Linked(dial[1], dial[2])

\* Libp2p refuses an inbound dial, explicit or background, while the local
\* marker stands. The bug configurations let one through, so this fails.
INV_MarkedInboundRefused ==
  \A a, b \in Nodes :
     Marked(b, a) =>
        \* no accepted remote explicit dial from the marked peer
        /\ <<a, b>> \notin remoteDials
        \* no accepted background dial from the marked peer
        /\ <<a, b>> \notin backgroundLinks

====
