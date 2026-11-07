---- MODULE NodeStartup ----
\* The story, in plain words.
\*
\* An app starts a node with a config (crates/defra-node/src/assemble.rs
\* assemble). The config picks a transport (libp2p or iroh), may give its
\* own replicator retry intervals, may ask for node access control (NAC), and,
\* for libp2p, names the addresses to listen on and the peers to dial once the
\* node is up. Starting runs one sequence:
\*   1. check the config: the retry intervals are a valid ladder, and NAC was
\*      only asked for by a node with an identity to own it;
\*   2. open the database;
\*   3. start the transport, handing it the retry ladder (setup_libp2p,
\*      setup_iroh in crates/defra-node/src/p2p_runtime.rs and libp2p_runtime.rs);
\*   4. libp2p only: listen on each address in turn;
\*   5. create the NAC manager, owned by the node identity (create_nac_manager
\*      in crates/defra-node/src/node_acp.rs), and wire it into the transport;
\*   6. libp2p only: dial the bootstrap peers.
\* Any step can fail, and then the node does not start.
\*
\* What must never happen:
\*   1. startup fails but leaves the transport host running or the database
\*      open, so nothing can ever close them - INV_FailedHoldsNothing;
\*   2. the node starts with a retry site on the default ladder although the
\*      config gave its own - INV_OneSchedule;
\*   3. the node starts with NAC on and nobody owning it, or with NAC not as
\*      the config asked - INV_NacAsAsked;
\*   4. a bootstrap peer is dialed before NAC is in place, so its management
\*      requests would meet no access control - INV_NoPeerBeforeNac;
\*   5. an unreachable bootstrap peer stops the node from starting, which Go
\*      does not do - INV_BootstrapNonFatal;
\*   6. startup never finishes - LIVE_StartupEnds.
\*
\* The Rust code each guard models:
\*   ScheduleToIroh       crates/defra-node/src/p2p_runtime.rs setup_iroh: the
\*                        ladder goes into the iroh peer config and its retry
\*                        peerstore, as setup_libp2p and assemble_libp2p give
\*                        it to the libp2p peerstore and document pusher;
\*   ShutdownOnListenFail crates/defra-node/src/libp2p_runtime.rs assemble_libp2p: a
\*                        listen that fails calls shutdown_libp2p_host, and
\*                        assemble then closes the database (the background
\*                        tasks start last, so none is running yet);
\*   ValidateFirst        crates/defra-node/src/assemble.rs assemble: the NAC
\*                        owner is checked with the retry ladder, before the
\*                        database opens;
\*   OwnerPassed          crates/defra-node/src/node_acp.rs create_nac_manager:
\*                        NAC is enabled with the node identity as its owner;
\*   ConnectAfterNac      crates/defra-node/src/assemble.rs assemble: bootstrap
\*                        peers are dialed after the manage hooks are set;
\*   BootstrapNonFatal    the same loop logs a failed dial and goes on.
\*
\* Out of the model: an iroh bind that fails (iroh binds one address, set up
\* by IrohPeer::start), and a NAC manager whose storage fails to initialize.

EXTENDS Naturals, Sequences  \* numbers and lists, for the listen addresses

CONSTANTS
  ScheduleToIroh,        \* TRUE: iroh's retry sites get the configured ladder
  ShutdownOnListenFail,  \* TRUE: a failed listen stops the host and the database
  ValidateFirst,         \* TRUE: the NAC owner is checked before anything opens
  OwnerPassed,           \* TRUE: NAC is enabled owned by the node identity
  ConnectAfterNac,       \* TRUE: bootstrap peers are dialed after NAC is in place
  BootstrapNonFatal      \* TRUE: a failed bootstrap dial is logged, not fatal

\* Every place that schedules a replicator retry, per transport.
Sites == {"libp2pPeerstore", "libp2pPusher", "irohPeer", "irohPeerstore"}

\* The retry sites the given transport uses.
SitesOf(t) == IF t = "libp2p" THEN {"libp2pPeerstore", "libp2pPusher"}
              ELSE {"irohPeer", "irohPeerstore"}

\* The configs the model tries: every mix of these choices.
Transports == {"libp2p", "iroh"}                       \* which transport
Ladders    == {"default", "custom", "invalid"}         \* the retry intervals
AddrLists  == {<<"ok">>, <<"ok", "bad">>, <<"bad">>}   \* addresses, in order
PeerKinds  == {"none", "up", "down"}                   \* one bootstrap peer

\* The startup steps.
Phases == {"Validate", "Transport", "Listen", "Nac", "Bootstrap", "Started",
           "Failed"}

\* The step after the transport is up: NAC first, unless peers are dialed first.
AfterTransport == IF ConnectAfterNac THEN "Nac" ELSE "Bootstrap"

\* The step after NAC: the peers, unless they were dialed already.
AfterNac == IF ConnectAfterNac THEN "Bootstrap" ELSE "Started"

\* The step after the peers: started, unless NAC is still to come.
AfterBootstrap == IF ConnectAfterNac THEN "Started" ELSE "Nac"

VARIABLES
  transport,      \* config: the transport
  ladder,         \* config: the retry intervals
  addrs,          \* config: libp2p listen addresses, each "ok" or "bad"
  hasIdentity,    \* config: TRUE if the node has an identity
  nacAsked,       \* config: TRUE if the config asks for NAC
  peer,           \* config: the bootstrap peer, if any, and whether it answers
  phase,          \* which startup step the node is at
  listened,       \* how many listen addresses are bound so far
  hostRunning,    \* TRUE while the transport host runs
  dbOpen,         \* TRUE while the database is open
  sites,          \* per retry site: the ladder it got, or "unset"
  nacOn,          \* TRUE once NAC is enabled
  nacOwner,       \* who owns NAC: "node" or "nobody"
  nacDone,        \* TRUE once the NAC manager is in place
  peerConnected,  \* TRUE once the bootstrap peer was dialed
  failCause       \* why startup failed, or "none"

vars == <<transport, ladder, addrs, hasIdentity, nacAsked, peer, phase,
          listened, hostRunning, dbOpen, sites, nacOn, nacOwner, nacDone,
          peerConnected, failCause>>  \* everything, for stuttering and fairness

\* The config, which never changes once startup begins.
config == <<transport, ladder, addrs, hasIdentity, nacAsked, peer>>

TypeOK ==
  /\ transport \in Transports                             \* a known transport
  /\ ladder \in Ladders                                   \* a known ladder
  /\ addrs \in AddrLists                                  \* a known list
  /\ hasIdentity \in BOOLEAN                              \* a flag
  /\ nacAsked \in BOOLEAN                                 \* a flag
  /\ peer \in PeerKinds                                   \* a known peer
  /\ phase \in Phases                                     \* a known step
  /\ listened \in 0..2                                    \* at most two bound
  /\ hostRunning \in BOOLEAN                              \* a flag
  /\ dbOpen \in BOOLEAN                                   \* a flag
  /\ sites \in [Sites -> {"unset", "default", "custom"}]  \* a ladder per site
  /\ nacOn \in BOOLEAN                                    \* a flag
  /\ nacOwner \in {"node", "nobody"}                      \* a known owner
  /\ nacDone \in BOOLEAN                                  \* a flag
  /\ peerConnected \in BOOLEAN                            \* a flag
  /\ failCause \in {"none", "ladder", "nac", "listen", "bootstrap"} \* a cause

Init ==
  /\ transport \in Transports                \* any transport
  /\ ladder \in Ladders                      \* any retry intervals
  /\ addrs \in AddrLists                     \* any listen addresses
  /\ hasIdentity \in BOOLEAN                 \* with or without an identity
  /\ nacAsked \in BOOLEAN                    \* NAC asked for or not
  /\ peer \in PeerKinds                      \* any bootstrap peer
  /\ phase = "Validate"                      \* startup begins with the checks
  /\ listened = 0                            \* nothing bound yet
  /\ hostRunning = FALSE                     \* no host yet
  /\ dbOpen = FALSE                          \* the database is not open yet
  /\ sites = [s \in Sites |-> "unset"]       \* no retry site exists yet
  /\ nacOn = FALSE                           \* NAC is off
  /\ nacOwner = "nobody"                     \* and nobody owns it
  /\ nacDone = FALSE                         \* no NAC manager yet
  /\ peerConnected = FALSE                   \* no peer dialed
  /\ failCause = "none"                      \* nothing failed

\* Startup gives up for `cause`, holding what it holds now.
Fail(cause) ==
  /\ phase' = "Failed"                       \* the node does not start
  /\ failCause' = cause                      \* and says why

\* Startup gives up for `cause` after stopping the host and closing the
\* database, as build_node does when the transport setup fails.
FailCleanly(cause) ==
  /\ Fail(cause)                             \* the node does not start
  /\ hostRunning' = FALSE                    \* the host is stopped
  /\ dbOpen' = FALSE                         \* the database is closed

\* The NAC request cannot be met: NAC asked for, but no identity to own it.
NacUnownable == nacAsked /\ ~hasIdentity

\* Step 1: check the config, then open the database.
Validate ==
  /\ phase = "Validate"                                      \* this step
  /\ IF ladder = "invalid"                                   \* a bad ladder:
     THEN /\ Fail("ladder")                                  \* refuse to start
          /\ UNCHANGED <<hostRunning, dbOpen>>               \* nothing opened
     ELSE IF ValidateFirst /\ NacUnownable                   \* NAC unownable:
     THEN /\ Fail("nac")                                     \* refuse to start
          /\ UNCHANGED <<hostRunning, dbOpen>>               \* nothing opened
     ELSE /\ dbOpen' = TRUE                                  \* open the database
          /\ phase' = "Transport"                            \* next: transport
          /\ UNCHANGED <<hostRunning, failCause>>
  /\ UNCHANGED <<config, listened, sites, nacOn, nacOwner, nacDone,
                 peerConnected>>

\* Step 3: start the transport host, handing the ladder to its retry sites.
Transport ==
  /\ phase = "Transport"                                     \* this step
  /\ hostRunning' = TRUE                                     \* the host runs
  /\ sites' = [s \in Sites |->                               \* for each site:
                 IF s \notin SitesOf(transport) THEN sites[s] \* not this one's
                 ELSE IF transport = "iroh" /\ ~ScheduleToIroh
                      THEN "default"                         \* ladder dropped
                      ELSE ladder]                           \* the config's
  /\ phase' = IF transport = "libp2p" THEN "Listen"          \* libp2p listens
              ELSE AfterTransport                            \* iroh is bound
  /\ UNCHANGED <<config, listened, dbOpen, nacOn, nacOwner, nacDone,
                 peerConnected, failCause>>

\* Step 4: libp2p binds its next address, or moves on once all are bound.
Listen ==
  /\ phase = "Listen"                                        \* this step
  /\ IF listened = Len(addrs)                                \* all bound:
     THEN /\ phase' = AfterTransport                         \* move on
          /\ UNCHANGED <<listened, hostRunning, dbOpen, failCause>>
     ELSE IF addrs[listened + 1] = "ok"                      \* this one binds:
     THEN /\ listened' = listened + 1                        \* one more bound
          /\ UNCHANGED <<phase, hostRunning, dbOpen, failCause>>
     ELSE IF ShutdownOnListenFail                            \* it fails, and
     THEN /\ FailCleanly("listen")                           \* all is stopped
          /\ UNCHANGED listened
     ELSE /\ Fail("listen")                                  \* or just returns
          /\ dbOpen' = FALSE                                 \* the db closes
          /\ UNCHANGED <<listened, hostRunning>>             \* host left up
  /\ UNCHANGED <<config, sites, nacOn, nacOwner, nacDone, peerConnected>>

\* Step 5: create the NAC manager. A NAC request the checks did not refuse,
\* but that cannot be owned, fails here, after the host and database opened,
\* by returning early as `?` does.
Nac ==
  /\ phase = "Nac"                                           \* this step
  /\ IF NacUnownable                                         \* only if not checked
     THEN /\ Fail("nac")                                     \* returns early
          /\ UNCHANGED <<nacOn, nacOwner, nacDone>>          \* holding the rest
     ELSE /\ nacOn' = nacAsked                               \* on if asked
          /\ nacOwner' = IF nacAsked /\ OwnerPassed          \* owned by the node
                         THEN "node" ELSE "nobody"           \* or by nobody
          /\ nacDone' = TRUE                                 \* NAC is in place
          /\ phase' = AfterNac                               \* move on
          /\ UNCHANGED failCause
  /\ UNCHANGED <<config, listened, hostRunning, dbOpen, sites, peerConnected>>

\* Step 6: libp2p dials its bootstrap peer. A peer that does not answer is
\* logged, or, without that guard, fails startup after closing everything.
Bootstrap ==
  /\ phase = "Bootstrap"                                     \* this step
  /\ IF transport = "libp2p" /\ peer = "down" /\ ~BootstrapNonFatal
     THEN FailCleanly("bootstrap")                           \* gives up
     ELSE /\ phase' = AfterBootstrap                         \* move on
          /\ UNCHANGED <<hostRunning, dbOpen, failCause>>
  /\ peerConnected' = (transport = "libp2p" /\ peer = "up")  \* dialed if it answers
  /\ UNCHANGED <<config, listened, sites, nacOn, nacOwner, nacDone>>

\* Any one step happens.
Next == Validate \/ Transport \/ Listen \/ Nac \/ Bootstrap

\* Fairness: each step is taken once it can be.
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* ---- What must hold ----

\* A failed startup holds nothing: no host is left running, no database open.
INV_FailedHoldsNothing == phase = "Failed" => ~hostRunning /\ ~dbOpen

\* A started node's every retry site uses the ladder the config named.
INV_OneSchedule ==
  phase = "Started" => \A s \in SitesOf(transport) : sites[s] = ladder

\* A started node has NAC exactly as asked, and NAC that is on has an owner.
INV_NacAsAsked ==
  phase = "Started" => /\ nacOn = nacAsked                 \* as the config asked
                       /\ nacOn => nacOwner = "node"       \* owned by the node

\* No bootstrap peer is dialed before NAC is in place.
INV_NoPeerBeforeNac == peerConnected => nacDone

\* An unreachable bootstrap peer never stops the node.
INV_BootstrapNonFatal == failCause # "bootstrap"

\* Startup always ends: the node starts, or it fails.
LIVE_StartupEnds == <>(phase \in {"Started", "Failed"})

====
