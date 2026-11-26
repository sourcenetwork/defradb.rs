---- MODULE StorePurge ----
\* The story, in plain words.
\*
\* A client asks a running node to purge: forget everything it stored and
\* start again with the same options, as Go's PurgeAndRestart does. The node
\* answers the request at once, then a process outside the request tears the
\* node down, wipes its store and starts it again.
\*
\* Who acts:
\*   * a client, which sends up to two purge requests while the node serves;
\*   * the purge handler, which answers each request: it refuses one when
\*     the node is not in development mode (403, as Go's PurgeAndRestart),
\*     then one with no purge handler registered (501), and otherwise,
\*     through a once-only gate, starts at most one teardown per run;
\*   * the teardown: drain the requests, stop the embedded Vera node, close
\*     the database, then wipe and start the node again;
\*   * on the Embedded path only, the embedding application, which may keep
\*     a second handle to the node, and may itself call Node::shutdown before
\*     the purge's teardown starts.
\*
\* The teardown takes one of two paths:
\*   * Embedded: defra-node's Node::shutdown_and_wipe, whose shutdown (the
\*     NodeShutdown model) returns an error when the Vera node or the
\*     database did not stop, and which then wipes nothing. A closed store's
\*     directory stays locked until the node itself is dropped, so it wipes
\*     only once it has dropped the node as its last handle; and it wipes
\*     nothing after a shutdown another caller ran, whose outcome it never saw;
\*   * Cli: `defradb start`, whose run loop stops Vera and closes the
\*     database itself, owns the only handle, and returns Purged only when
\*     both succeeded.
\*
\* What the store holds, by key class:
\*   * Data: documents, blocks, indexes and ACP state, in the store;
\*   * StoreHeldPeerKey: a peer key kept in the store (no keyring);
\*   * KeyringHeld: the node identity and peer key in the keyring, which
\*     lives outside the store.
\* The store is on disk (Persistent) or in memory (Memory); the embedded
\* Vera node, when there is one, keeps its directories on disk either way.
\*
\* What must never happen:
\*   1. the wipe runs while the store is open or still locked by a live
\*      node, or the Vera node holds its directories - INV_NoWipeWhileOpen;
\*   2. the node opens again before the wipe ran - INV_NoOpenBeforeWipe;
\*   3. anything of the old store, the store-held peer key or the old Vera
\*      directories is still there when the node serves again -
\*      INV_NothingSurvives;
\*   4. the keyring is removed - INV_KeyringKept;
\*   5. a refused purge (no development mode, or no handler) changes
\*      anything - INV_RefusedPurgeTouchesNothing;
\*   6. a purge is accepted on a node not in development mode -
\*      INV_PurgeOnlyInDevelopment;
\*   7. a run starts two teardowns - INV_OnePurgePerRun;
\*   8. the node starts again after a teardown or a wipe that failed -
\*      INV_NoRestartAfterFailedTeardown;
\*   9. on the Cli path, the node starts again without the Orbis remote
\*      signer `defradb start` registered once, before its first run: each
\*      run ends by clearing the process's identity store, which drops
\*      every signer nothing holds - INV_SignerSurvivesRestart;
\* and what must happen: an accepted purge ends with the node serving again,
\* or with the node exited on a failure - LIVE_PurgeServesAgain.
\*
\* Each guard has a RED configuration that removes it (see each .cfg).
\*
\* Rust: crates/surfaces/http/src/handlers/utility.rs purge;
\* crates/entry-points/defra-node/src/purge.rs PurgeGate, Node::purge_as,
\* Node::shutdown_and_wipe; crates/entry-points/defra-node/src/lib.rs
\* Node::shutdown; crates/engine/storage/src/wipe.rs wipe;
\* crates/entry-points/cli/src/commands/start/run.rs Node::run_until;
\* crates/entry-points/cli/src/commands/start/purge.rs outcome;
\* crates/entry-points/cli/src/commands/start/mod.rs StartArgs::execute,
\* StartArgs::hold_remote_signer.
EXTENDS Naturals

CONSTANTS
  Paths,                  \* the teardown paths a run may take: "Embedded", "Cli"
  Stores,                 \* the store kinds a run may use: "Persistent", "Memory"
  Veras,                  \* whether an embedded Vera node runs: "None", "Embedded"
  Developments,           \* whether the node runs in development mode
  Handlers,               \* for the Embedded path, whether a purge handler is registered
  HeldElsewhere,          \* for the Embedded path, whether the application keeps a second handle
  OtherCallers,           \* for the Embedded path, whether the application may call Node::shutdown itself
  WipeAfterClose,         \* guard: the wipe waits for the database to close
  WaitVera,               \* guard: the wipe waits for the Vera node to stop
  OpenAfterWipe,          \* guard: the node opens again only after the wipe
  ShutdownOutsideRequest, \* guard: the request is answered before the teardown starts
  DevelopmentChecked,     \* guard: outside development mode the purge is refused first and nothing stops
  HandlerChecked,         \* guard: without a handler the purge is refused and nothing stops
  GateOnce,               \* guard: one teardown per run of the node
  NoRestartAfterFailedWipe, \* guard: a failed wipe ends the run instead of restarting
  KeyringExcluded,        \* guard: the keyring is never in the removal set
  MemoryRemovesOwnedDirs, \* guard: the Vera directories are removed under a memory store too
  CliChecksVeraStop,      \* guard: the CLI wipes only when the Vera node stopped
  CliChecksClose,         \* guard: the CLI wipes only when the database closed
  LastHandleChecked,      \* guard: shutdown_and_wipe wipes only as the node's last handle (WipeWhileHeld)
  ShutdownOnce,           \* guard: shutdown_and_wipe wipes nothing after another caller's shutdown (WipeAfterShutdown)
  SignerHeld              \* guard: defradb start holds its Orbis signer across every run (SignerDropped)

\* Every key class the store itself holds.
StoreClasses == {"Data", "StoreHeldPeerKey"}

\* The steps of one purge, in order.
Phases == {"serving", "stopping", "drained", "veraStopped", "closed", "wiping", "exited"}

\* The result of a step that can fail.
Results == {"pending", "ok", "failed"}

VARIABLES
  path,          \* the teardown path this behaviour takes
  store,         \* the store kind
  vera,          \* whether an embedded Vera node runs
  development,   \* whether the node runs in development mode
  handler,       \* whether a purge handler is registered
  otherHandle,   \* the application holds a second handle to the node
  otherCaller,   \* the application may shut the node down itself
  phase,         \* where the node is in serving, tearing down and starting again
  run,           \* which start of the node this is: 1, then 2 after a restart
  storeOpen,     \* the database is open
  storeLocked,   \* a live node still locks the store's directory on disk
  veraRunning,   \* the Vera node runs and holds its directories
  storeData,     \* the key classes of the old store still present
  veraDirs,      \* the old Vera directories are still on disk
  keyring,       \* the keyring still holds the node identity and peer key
  signer,        \* the process's identity store still has the Orbis remote signer
  requests,      \* how many purge requests the client sent
  req,           \* the purge request in flight: "none", "waiting" for the
                 \* handler, or "held" by a teardown running inside it
  purges,        \* how many teardowns the gate let start in this run
  accepted,      \* a purge was accepted in the first run
  refused,       \* a purge was refused (no development mode, or no handler)
  otherShut,     \* the application's own Node::shutdown ran
  blind,         \* the purge's teardown went on without seeing how the shutdown went
  veraStop,      \* how the Vera stop went
  closeResult,   \* how the database close went
  wiped,         \* the wipe ran in this teardown
  wipeResult,    \* how the wipe went
  wipedWhileHeld,       \* ghost: a wipe ran while the store was open or locked, or Vera held its dirs
  openedBeforeWipe,     \* ghost: the node opened again before its wipe
  restartedAfterFailure \* ghost: the node started again after a failed teardown or wipe

vars == <<path, store, vera, development, handler, otherHandle, otherCaller, phase, run,
          storeOpen, storeLocked, veraRunning, storeData, veraDirs, keyring, signer,
          requests, req, purges, accepted, refused, otherShut, blind, veraStop,
          closeResult, wiped, wipeResult, wipedWhileHeld, openedBeforeWipe,
          restartedAfterFailure>>

\* Every variable has the kind of value it should.
TypeOK ==
  /\ path \in Paths
  /\ store \in Stores
  /\ vera \in Veras
  /\ development \in BOOLEAN
  /\ handler \in BOOLEAN
  /\ otherHandle \in BOOLEAN
  /\ otherCaller \in BOOLEAN
  /\ phase \in Phases
  /\ run \in 1..2
  /\ storeOpen \in BOOLEAN
  /\ storeLocked \in BOOLEAN
  /\ veraRunning \in BOOLEAN
  /\ storeData \subseteq StoreClasses
  /\ veraDirs \in BOOLEAN
  /\ keyring \in BOOLEAN
  /\ signer \in BOOLEAN
  /\ requests \in 0..2
  /\ req \in {"none", "waiting", "held"}
  /\ purges \in 0..2
  /\ accepted \in BOOLEAN
  /\ refused \in BOOLEAN
  /\ otherShut \in BOOLEAN
  /\ blind \in BOOLEAN
  /\ veraStop \in Results
  /\ closeResult \in Results
  /\ wiped \in BOOLEAN
  /\ wipeResult \in Results
  /\ wipedWhileHeld \in BOOLEAN
  /\ openedBeforeWipe \in BOOLEAN
  /\ restartedAfterFailure \in BOOLEAN

\* A node serving its first run, with everything stored.
Init ==
  \* Each behaviour picks one path, store kind and Vera setting.
  /\ path \in Paths
  /\ store \in Stores
  /\ vera \in Veras
  \* Development mode is a node setting on either path.
  /\ development \in Developments
  \* `defradb start` always has its run loop as the handler.
  /\ handler \in IF path = "Cli" THEN {TRUE} ELSE Handlers
  \* `defradb start` owns the only handle and runs the only shutdown.
  /\ otherHandle \in IF path = "Cli" THEN {FALSE} ELSE HeldElsewhere
  /\ otherCaller \in IF path = "Cli" THEN {FALSE} ELSE OtherCallers
  /\ phase = "serving"
  /\ run = 1
  /\ storeOpen = TRUE
  \* Only a store on disk has a directory to lock.
  /\ storeLocked = (store = "Persistent")
  \* The Vera node runs only when one is configured.
  /\ veraRunning = (vera = "Embedded")
  /\ storeData = StoreClasses
  \* Its directories exist only when it runs.
  /\ veraDirs = (vera = "Embedded")
  /\ keyring = TRUE
  \* `defradb start` registered the Orbis signer before the first run; on
  \* the Embedded path it stands for any signer the application registered.
  /\ signer = TRUE
  /\ requests = 0
  /\ req = "none"
  /\ purges = 0
  /\ accepted = FALSE
  /\ refused = FALSE
  /\ otherShut = FALSE
  /\ blind = FALSE
  /\ veraStop = "pending"
  /\ closeResult = "pending"
  /\ wiped = FALSE
  /\ wipeResult = "pending"
  /\ wipedWhileHeld = FALSE
  /\ openedBeforeWipe = FALSE
  /\ restartedAfterFailure = FALSE

\* The variables the teardown steps never touch.
fixed == <<path, store, vera, development, handler, otherHandle, otherCaller>>

\* The client sends a purge request, in the first run, while the node still
\* accepts requests, one at a time. A node the application shut down
\* accepts none.
Request ==
  /\ run = 1
  /\ phase \in {"serving", "stopping"}
  /\ ~otherShut
  /\ req = "none"
  /\ requests < 2
  /\ requests' = requests + 1
  /\ req' = "waiting"
  /\ UNCHANGED <<fixed, signer, phase, run, storeOpen, storeLocked, veraRunning, storeData,
                 veraDirs, keyring, purges, accepted, refused, otherShut, blind,
                 veraStop, closeResult, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* Whether a request gets past the development-mode check: the node is in
\* development mode, or (Red) the check is gone.
PastDevelopment == development \/ ~DevelopmentChecked

\* The handler answers a request on a node not in development mode, before
\* any other check (PurgeGate::request, the route's 403). Guarded: it is
\* refused and nothing stops. Red: this answer never comes, and the request
\* goes on to the handler checks below as if the node were in development
\* mode.
HandleNotDevelopment ==
  /\ req = "waiting"
  /\ DevelopmentChecked
  /\ ~development
  /\ req' = "none"
  /\ refused' = TRUE
  /\ UNCHANGED <<fixed, signer, phase, run, storeOpen, storeLocked, veraRunning, storeData,
                 veraDirs, keyring, requests, purges, accepted, otherShut, blind,
                 veraStop, closeResult, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* The handler answers a request the node has no purge handler for.
\* Guarded: it is refused with 501 and nothing stops. Red: the node stops
\* anyway, as if the refusal were only a message.
HandleNoHandler ==
  /\ req = "waiting"
  /\ PastDevelopment
  /\ ~handler
  /\ req' = "none"
  /\ refused' = TRUE
  \* Without the guard the teardown starts all the same.
  /\ phase' = IF HandlerChecked THEN phase ELSE "stopping"
  /\ purges' = IF HandlerChecked THEN purges ELSE purges + 1
  /\ UNCHANGED <<fixed, signer, run, storeOpen, storeLocked, veraRunning, storeData, veraDirs,
                 keyring, requests, accepted, otherShut, blind, veraStop, closeResult,
                 wiped, wipeResult, wipedWhileHeld, openedBeforeWipe,
                 restartedAfterFailure>>

\* The handler answers a request the gate lets through: a teardown starts.
\* Guarded: the 200 goes out first and the teardown runs outside the request.
\* Red: the teardown runs inside the request, which then waits for a drain
\* that waits for it.
HandleAccepted ==
  /\ req = "waiting"
  /\ PastDevelopment
  /\ handler
  \* The gate opens once per run; without its guard it opens every time.
  /\ purges = 0 \/ ~GateOnce
  /\ purges' = purges + 1
  /\ accepted' = TRUE
  /\ phase' = "stopping"
  \* Answered now, or held until the teardown it runs ends.
  /\ req' = IF ShutdownOutsideRequest THEN "none" ELSE "held"
  /\ UNCHANGED <<fixed, signer, run, storeOpen, storeLocked, veraRunning, storeData, veraDirs,
                 keyring, requests, refused, otherShut, blind, veraStop, closeResult,
                 wiped, wipeResult, wipedWhileHeld, openedBeforeWipe,
                 restartedAfterFailure>>

\* A request the gate already let one teardown through for: answered 200,
\* nothing more starts.
HandleRepeat ==
  /\ req = "waiting"
  /\ PastDevelopment
  /\ handler
  /\ GateOnce
  /\ purges >= 1
  /\ req' = "none"
  /\ UNCHANGED <<fixed, signer, phase, run, storeOpen, storeLocked, veraRunning, storeData,
                 veraDirs, keyring, requests, purges, accepted, refused, otherShut,
                 blind, veraStop, closeResult, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* The application calls Node::shutdown itself before the purge's teardown
\* claims the node: it drains, stops Vera and closes the database, each of
\* which may fail. Its outcome goes to the application, not to the purge.
OtherShutdown ==
  /\ otherCaller
  /\ ~otherShut
  /\ run = 1
  /\ phase \in {"serving", "stopping"}
  /\ req = "none"
  /\ otherShut' = TRUE
  /\ veraStop' \in IF vera = "Embedded" THEN {"ok", "failed"} ELSE {"ok"}
  \* Only a stop that succeeded frees the directories.
  /\ veraRunning' = (veraRunning /\ veraStop' = "failed")
  /\ closeResult' \in {"ok", "failed"}
  /\ storeOpen' = (closeResult' = "failed")
  \* A memory store's contents go with the closed process memory.
  /\ storeData' = IF store = "Memory" /\ closeResult' = "ok" THEN {} ELSE storeData
  /\ UNCHANGED <<fixed, signer, phase, run, storeLocked, veraDirs, keyring, requests, req,
                 purges, accepted, refused, blind, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* The purge's teardown claims the node and drains the requests in flight.
\* Guarded: after the application's own shutdown it stops there, wiping
\* nothing (WipeAfterShutdown). Red: it goes on as if its own shutdown had
\* just succeeded, never seeing how the other one went.
Drain ==
  /\ phase = "stopping"
  \* A held request never ends: it waits for this very drain.
  /\ req = "none"
  /\ phase' = CASE ~otherShut   -> "drained"
                [] ShutdownOnce -> "exited"
                [] OTHER        -> "closed"
  /\ blind' = (otherShut /\ ~ShutdownOnce)
  /\ UNCHANGED <<fixed, signer, run, storeOpen, storeLocked, veraRunning, storeData, veraDirs,
                 keyring, requests, req, purges, accepted, refused, otherShut,
                 veraStop, closeResult, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* The Vera node is told to stop. It stops, or misses its deadline and keeps
\* holding its directories.
StopVera ==
  /\ phase = "drained"
  /\ veraStop' \in IF vera = "Embedded" THEN {"ok", "failed"} ELSE {"ok"}
  \* Only a stop that succeeded frees the directories.
  /\ veraRunning' = (veraRunning /\ veraStop' = "failed")
  /\ phase' = "veraStopped"
  /\ UNCHANGED <<fixed, signer, run, storeOpen, storeLocked, storeData, veraDirs, keyring,
                 requests, req, purges, accepted, refused, otherShut, blind,
                 closeResult, wiped, wipeResult, wipedWhileHeld, openedBeforeWipe,
                 restartedAfterFailure>>

\* The database closes, or fails to and stays open. Either way the node
\* still exists, so a store on disk stays locked.
Close ==
  /\ phase = "veraStopped"
  /\ closeResult' \in {"ok", "failed"}
  /\ storeOpen' = (closeResult' = "failed")
  \* A memory store's contents go with the closed process memory.
  /\ storeData' = IF store = "Memory" /\ closeResult' = "ok" THEN {} ELSE storeData
  /\ phase' = "closed"
  \* The CLI's run then clears the identity store (clear_identity_store),
  \* which keeps a signer only while something holds it; without the guard
  \* the Orbis signer is only pinned, and goes.
  /\ signer' = IF path = "Cli" /\ ~SignerHeld THEN FALSE ELSE signer
  /\ UNCHANGED <<fixed, run, storeLocked, veraRunning, veraDirs, keyring, requests,
                 req, purges, accepted, refused, otherShut, blind, veraStop, wiped,
                 wipeResult, wipedWhileHeld, openedBeforeWipe, restartedAfterFailure>>

\* Whether this path treats the teardown as failed. The Embedded path's
\* shutdown returns both errors; the CLI checks each under its guard. A
\* blind teardown saw neither.
TeardownFails ==
  /\ ~blind
  /\ \/ veraStop = "failed" /\ (path = "Embedded" \/ CliChecksVeraStop)
     \/ closeResult = "failed" /\ (path = "Embedded" \/ CliChecksClose)

\* Whether the teardown really failed, whatever the code checked.
TeardownFailed == veraStop = "failed" \/ closeResult = "failed"

\* Whether shutdown_and_wipe stops because another handle still holds the
\* node (WipeWhileHeld).
HandleHeld == otherHandle /\ LastHandleChecked

\* After the teardown: a failure, or a node still held elsewhere, ends the
\* run with its error; otherwise the purge drops its handle and the wipe
\* comes next. Dropping it unlocks the store only if no other handle is left.
Decide ==
  /\ phase = "closed"
  /\ phase' = IF TeardownFails \/ HandleHeld THEN "exited" ELSE "wiping"
  /\ storeLocked' = IF phase' = "wiping" THEN storeLocked /\ otherHandle ELSE storeLocked
  /\ UNCHANGED <<fixed, signer, run, storeOpen, veraRunning, storeData, veraDirs, keyring,
                 requests, req, purges, accepted, refused, otherShut, blind, veraStop,
                 closeResult, wiped, wipeResult, wipedWhileHeld, openedBeforeWipe,
                 restartedAfterFailure>>

\* Where the wipe may run: after the teardown; before the close without its
\* guard; before the Vera stop without that guard.
WipeAllowed ==
  \/ phase = "wiping"
  \/ phase = "veraStopped" /\ ~WipeAfterClose
  \/ phase = "drained" /\ ~WaitVera

\* The wipe: drop every key of a persistent store, then remove the owned
\* directories. It succeeds, or fails part way and leaves things as they were.
Wipe ==
  /\ ~wiped
  /\ WipeAllowed
  /\ wiped' = TRUE
  /\ wipeResult' \in {"ok", "failed"}
  \* A memory store has nothing on disk to drop.
  /\ storeData' = IF wipeResult' = "ok" /\ store = "Persistent" THEN {} ELSE storeData
  \* The Vera directories are owned under either store kind; without its
  \* guard a memory store skips them.
  /\ veraDirs' = IF wipeResult' = "ok" /\ (store = "Persistent" \/ MemoryRemovesOwnedDirs)
                 THEN FALSE ELSE veraDirs
  \* The keyring is never in the removal set; without the guard it is.
  /\ keyring' = IF wipeResult' = "ok" /\ ~KeyringExcluded THEN FALSE ELSE keyring
  /\ wipedWhileHeld' = (wipedWhileHeld \/ storeOpen \/ storeLocked \/ veraRunning)
  /\ UNCHANGED <<fixed, signer, phase, run, storeOpen, storeLocked, veraRunning, requests, req,
                 purges, accepted, refused, otherShut, blind, veraStop, closeResult,
                 openedBeforeWipe, restartedAfterFailure>>

\* A failed wipe ends the run with its error.
WipeFailedExit ==
  /\ phase = "wiping"
  /\ wiped
  /\ wipeResult = "failed"
  /\ NoRestartAfterFailedWipe
  /\ phase' = "exited"
  /\ UNCHANGED <<fixed, signer, run, storeOpen, storeLocked, veraRunning, storeData, veraDirs,
                 keyring, requests, req, purges, accepted, refused, otherShut, blind,
                 veraStop, closeResult, wiped, wipeResult, wipedWhileHeld,
                 openedBeforeWipe, restartedAfterFailure>>

\* The node starts again with the same options: a new run, the store and the
\* Vera node open, the gate closed again. Guarded: only after a wipe that
\* succeeded. Red: before the wipe, or after a failed one.
Open ==
  /\ phase = "wiping"
  /\ wiped \/ ~OpenAfterWipe
  /\ (wiped => (wipeResult = "ok" \/ ~NoRestartAfterFailedWipe))
  /\ phase' = "serving"
  /\ run' = 2
  /\ storeOpen' = TRUE
  /\ storeLocked' = (store = "Persistent")
  /\ veraRunning' = (vera = "Embedded")
  /\ purges' = 0
  /\ openedBeforeWipe' = (openedBeforeWipe \/ ~wiped)
  /\ restartedAfterFailure' = (restartedAfterFailure \/ TeardownFailed \/ wipeResult = "failed")
  /\ UNCHANGED <<fixed, signer, storeData, veraDirs, keyring, requests, req, accepted,
                 refused, otherShut, blind, veraStop, closeResult, wiped, wipeResult,
                 wipedWhileHeld>>

Next ==
  \/ Request
  \/ HandleNotDevelopment
  \/ HandleNoHandler
  \/ HandleAccepted
  \/ HandleRepeat
  \/ OtherShutdown
  \/ Drain
  \/ StopVera
  \/ Close
  \/ Decide
  \/ Wipe
  \/ WipeFailedExit
  \/ Open

\* Every step that can be taken is eventually taken.
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* The wipe never runs while the store is open or still locked by a live
\* node, or the Vera node holds its directories.
INV_NoWipeWhileOpen == ~wipedWhileHeld

\* The node never opens again before its wipe ran.
INV_NoOpenBeforeWipe == ~openedBeforeWipe

\* Once the node serves again, nothing of the old store, its peer key or the
\* old Vera directories is left.
INV_NothingSurvives == (run = 2) => (storeData = {} /\ ~veraDirs)

\* The keyring always keeps the node identity and peer key.
INV_KeyringKept == keyring

\* Once the node serves again, the remote signer it signed with is still
\* registered.
INV_SignerSurvivesRestart == (run = 2) => signer

\* A refused purge leaves the node serving with everything it stored, unless
\* the application shut the node down itself.
INV_RefusedPurgeTouchesNothing ==
  (refused /\ ~otherShut) => (phase = "serving" /\ storeOpen /\ storeData = StoreClasses /\ keyring)

\* A purge is accepted only on a node in development mode.
INV_PurgeOnlyInDevelopment == accepted => development

\* One teardown per run of the node.
INV_OnePurgePerRun == purges <= 1

\* The node never starts again after a teardown or a wipe that failed.
INV_NoRestartAfterFailedTeardown == ~restartedAfterFailure

\* An accepted purge ends with the node serving again, or exited on an error.
LIVE_PurgeServesAgain == accepted ~> (run = 2 \/ phase = "exited")
====
