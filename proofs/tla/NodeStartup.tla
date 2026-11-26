---- MODULE NodeStartup ----
\* The story, in plain words.
\*
\* An app starts a node with a config (crates/entry-points/defra-node/src/assemble.rs
\* assemble). The config may give its own replicator retry intervals, may ask
\* for node access control (NAC), and names the peers to dial once the node
\* is up. Starting runs one sequence:
\*   1. check the config: the retry intervals are a valid ladder, and NAC was
\*      only asked for by a node with an identity to own it;
\*   2. open the database;
\*   3. start the pinc peer, handing it the retry ladder (setup_pinc in
\*      crates/entry-points/defra-node/src/pinc_runtime.rs);
\*   4. create the NAC manager, owned by the node identity (create_nac_manager
\*      in crates/entry-points/defra-node/src/node_acp.rs), and wire it into the peer;
\*   5. dial the bootstrap peers.
\* Any step can fail, and then the node does not start.
\*
\* What must never happen:
\*   1. startup fails but leaves the peer running or the database open, so
\*      nothing can ever close them - INV_FailedHoldsNothing;
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
\*   ScheduleToPinc       crates/entry-points/defra-node/src/pinc_runtime.rs setup_pinc: the
\*                        ladder goes into the pinc peer config and its retry
\*                        peerstore;
\*   ValidateFirst        crates/entry-points/defra-node/src/assemble.rs assemble: the NAC
\*                        owner is checked with the retry ladder, before the
\*                        database opens;
\*   OwnerPassed          crates/entry-points/defra-node/src/node_acp.rs create_nac_manager:
\*                        NAC is enabled with the node identity as its owner;
\*   ConnectAfterNac      crates/entry-points/defra-node/src/assemble.rs assemble: the
\*                        bootstrap dial setup_pinc prepares runs after the
\*                        manage hooks are set;
\*   BootstrapNonFatal    setup_pinc's bootstrap dial runs each dial as a
\*                        background service that logs a failure and goes on.
\*
\* Out of the model: a pinc bind that fails (the peer binds one address, set
\* up by PincPeer::start), and a NAC manager whose storage fails to initialize.

CONSTANTS
  ScheduleToPinc,        \* TRUE: the pinc peer's retry sites get the configured ladder
  ValidateFirst,         \* TRUE: the NAC owner is checked before anything opens
  OwnerPassed,           \* TRUE: NAC is enabled owned by the node identity
  ConnectAfterNac,       \* TRUE: bootstrap peers are dialed after NAC is in place
  BootstrapNonFatal      \* TRUE: a failed bootstrap dial is logged, not fatal

\* Every place that schedules a replicator retry: the peer and its peerstore.
Sites == {"pincPeer", "pincPeerstore"}

\* The configs the model tries: every mix of these choices.
Ladders    == {"default", "custom", "invalid"}         \* the retry intervals
PeerKinds  == {"none", "up", "down"}                   \* one bootstrap peer

\* The startup steps.
Phases == {"Validate", "Transport", "Nac", "Bootstrap", "Started", "Failed"}

\* The step after the peer is up: NAC first, unless peers are dialed first.
AfterTransport == IF ConnectAfterNac THEN "Nac" ELSE "Bootstrap"

\* The step after NAC: the peers, unless they were dialed already.
AfterNac == IF ConnectAfterNac THEN "Bootstrap" ELSE "Started"

\* The step after the peers: started, unless NAC is still to come.
AfterBootstrap == IF ConnectAfterNac THEN "Started" ELSE "Nac"

VARIABLES
  ladder,         \* config: the retry intervals
  hasIdentity,    \* config: TRUE if the node has an identity
  nacAsked,       \* config: TRUE if the config asks for NAC
  peer,           \* config: the bootstrap peer, if any, and whether it answers
  phase,          \* which startup step the node is at
  hostRunning,    \* TRUE while the pinc peer runs
  dbOpen,         \* TRUE while the database is open
  sites,          \* per retry site: the ladder it got, or "unset"
  nacOn,          \* TRUE once NAC is enabled
  nacOwner,       \* who owns NAC: "node" or "nobody"
  nacDone,        \* TRUE once the NAC manager is in place
  peerConnected,  \* TRUE once the bootstrap peer was dialed
  failCause       \* why startup failed, or "none"

vars == <<ladder, hasIdentity, nacAsked, peer, phase, hostRunning, dbOpen,
          sites, nacOn, nacOwner, nacDone, peerConnected, failCause>>
          \* everything, for stuttering and fairness

\* The config, which never changes once startup begins.
config == <<ladder, hasIdentity, nacAsked, peer>>

TypeOK ==
  /\ ladder \in Ladders                                   \* a known ladder
  /\ hasIdentity \in BOOLEAN                              \* a flag
  /\ nacAsked \in BOOLEAN                                 \* a flag
  /\ peer \in PeerKinds                                   \* a known peer
  /\ phase \in Phases                                     \* a known step
  /\ hostRunning \in BOOLEAN                              \* a flag
  /\ dbOpen \in BOOLEAN                                   \* a flag
  /\ sites \in [Sites -> {"unset", "default", "custom"}]  \* a ladder per site
  /\ nacOn \in BOOLEAN                                    \* a flag
  /\ nacOwner \in {"node", "nobody"}                      \* a known owner
  /\ nacDone \in BOOLEAN                                  \* a flag
  /\ peerConnected \in BOOLEAN                            \* a flag
  /\ failCause \in {"none", "ladder", "nac", "bootstrap"} \* a known cause

Init ==
  /\ ladder \in Ladders                      \* any retry intervals
  /\ hasIdentity \in BOOLEAN                 \* with or without an identity
  /\ nacAsked \in BOOLEAN                    \* NAC asked for or not
  /\ peer \in PeerKinds                      \* any bootstrap peer
  /\ phase = "Validate"                      \* startup begins with the checks
  /\ hostRunning = FALSE                     \* no peer yet
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

\* Startup gives up for `cause` after stopping the peer and closing the
\* database, as build_node does when a later step fails.
FailCleanly(cause) ==
  /\ Fail(cause)                             \* the node does not start
  /\ hostRunning' = FALSE                    \* the peer is stopped
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
          /\ phase' = "Transport"                            \* next: the peer
          /\ UNCHANGED <<hostRunning, failCause>>
  /\ UNCHANGED <<config, sites, nacOn, nacOwner, nacDone, peerConnected>>

\* Step 3: start the pinc peer, handing the ladder to its retry sites.
Transport ==
  /\ phase = "Transport"                                     \* this step
  /\ hostRunning' = TRUE                                     \* the peer runs
  /\ sites' = [s \in Sites |->                               \* for each site:
                 IF ScheduleToPinc THEN ladder               \* the config's
                 ELSE "default"]                             \* ladder dropped
  /\ phase' = AfterTransport                                 \* the peer is bound
  /\ UNCHANGED <<config, dbOpen, nacOn, nacOwner, nacDone, peerConnected,
                 failCause>>

\* Step 4: create the NAC manager. A NAC request the checks did not refuse,
\* but that cannot be owned, fails here, after the peer and database opened,
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
  /\ UNCHANGED <<config, hostRunning, dbOpen, sites, peerConnected>>

\* Step 5: the node dials its bootstrap peer. A peer that does not answer is
\* logged, or, without that guard, fails startup after closing everything.
Bootstrap ==
  /\ phase = "Bootstrap"                                     \* this step
  /\ IF peer = "down" /\ ~BootstrapNonFatal                  \* an unreachable peer
     THEN FailCleanly("bootstrap")                           \* gives up
     ELSE /\ phase' = AfterBootstrap                         \* move on
          /\ UNCHANGED <<hostRunning, dbOpen, failCause>>
  /\ peerConnected' = (peer = "up")                          \* dialed if it answers
  /\ UNCHANGED <<config, sites, nacOn, nacOwner, nacDone>>

\* Any one step happens.
Next == Validate \/ Transport \/ Nac \/ Bootstrap

\* Fairness: each step is taken once it can be.
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* ---- What must hold ----

\* A failed startup holds nothing: no peer is left running, no database open.
INV_FailedHoldsNothing == phase = "Failed" => ~hostRunning /\ ~dbOpen

\* A started node's every retry site uses the ladder the config named.
INV_OneSchedule ==
  phase = "Started" => \A s \in Sites : sites[s] = ladder

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
