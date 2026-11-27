---- MODULE EmbeddedVeraSwitch ----
\* The story, in plain words.
\*
\* A node runs a Vera node in its own process, as one of several devices of
\* an account (vera-embedded HostedNode, crates/engine/vera-embedded/src/host.rs).
\* Four callers reach that Vera node at once:
\*   * a watch: the host waits to hear that another device took the active
\*     role (HostedNode::demoted, HostedNode::contested). It reads the node
\*     for as long as it waits, possibly for ever;
\*   * a call that reads the node until it ends by itself: a handover, a
\*     resume, a takeover change (HostedNode::hand_over, resume,
\*     takeover_change). Vera bounds each, though maybe past the stop's
\*     deadline;
\*   * a switch: the host makes this device the active one, or a backup of
\*     another (HostedNode::take_over, HostedNode::follow). Vera stops the
\*     old node ("halts" it: it tells the node's thread to stop and waits
\*     for that thread, which first lets the blocks in flight finalize, for
\*     up to two seconds) and then launches a new one in its place: it
\*     spawns the new node's thread and waits for it to serve. The host then
\*     points the node's transports (the link the document ACP provider
\*     calls through) at the new one and records the device the slot now
\*     runs. The launch may be refused before any thread starts, the new
\*     thread may end by itself, or the launch may hang. The switch is
\*     bounded by the start deadline, and abandoned where it stands once a
\*     stop begins: a thread Vera was waiting for, or had just spawned, then
\*     runs on with nobody waiting for it, though it has been told to stop.
\*     The caller may also drop the switch future before it returns; the
\*     host then settles whatever node is left in the slot.
\*     A follow has one more step after its node serves: it reads the
\*     report of submissions the switch voided. That read can fail while
\*     the replacement still runs; the host must recognize the committed
\*     replacement from the running node, update the slot and link, and
\*     then return the report error.
\*     A switch that left no node behind is started again by a restart
\*     (HostedNode::complete_switch, HostedNode::restart), which launches
\*     the node the switch was heading to once no thread holds the
\*     directory any more;
\*   * the stop: the node's shutdown stops the Vera node
\*     (HostedNode::stop). It marks the host as stopping, closes the link,
\*     and waits, up to a deadline, to take the Vera node, stop it and wait
\*     for its thread, and then for the node directory to be released: the
\*     lock every Vera node thread holds on it until its runtime stopped.
\* The Vera node sits in one slot behind a reader-writer lock
\* (crates/engine/vera-embedded/src/host/held.rs Held). The watch and the
\* calls read it; the switch, the restart and the stop write it. A writer
\* first says it asks for the slot (Held::replace, Asking), and a watch that
\* hears it lets go and waits until no writer asks any more. A stop whose
\* deadline passes stops waiting and returns, leaving its ask behind unless
\* the ask withdraws itself. It leaves a reaper on the network runtime in
\* its place: a task that asks for the slot in turn and tells the node it
\* finds there to stop, or gives up after a deadline of its own so it never
\* holds the runtime's drain for long. A node the reaper did not reach is
\* told to stop when the host drops the HostedNode.
\*
\* What must never happen:
\*   1. once the stop returned, a transport still reaches a Vera node
\*      - INV_ClosedAfterStop;
\*   2. a switch or restart starts a Vera node after the stop began
\*      - INV_NoStartAfterStop;
\*   3. the slot holds a halted node while nobody writes it, so stop, term
\*      and export answer for a node that is gone - INV_SlotRuns;
\*   4. the stop reports that it stopped the node while a Vera node thread
\*      still runs and holds the directory - INV_StoppedMeansEnded;
\*   5. the slot records another device than the node in it runs as, so
\*      is_active answers with the old role - INV_DeviceNamesNode;
\*   6. a Vera node runs on for good after the stop returned, unless the
\*      reaper gave up on it - LIVE_NoNodeAfterStop; and then past the
\*      host dropping it - LIVE_DropStops;
\*   7. a switch never ends, because a watch holds the slot or a launch
\*      hangs - LIVE_SwitchEnds;
\*   8. a watch waits for good for writers that no longer exist
\*      - LIVE_WatchWakes;
\*   9. the stop never returns - LIVE_StopReturns;
\*  10. a switch that left no node is never undone by a host that asks
\*      - LIVE_Recovers.
\*
\* The Rust code each guard models:
\*   EvictWatch     crates/engine/vera-embedded/src/host/held.rs Held::watch,
\*                  which races the watched future against the `asked`
\*                  notification and lets go of the slot when it fires;
\*   AskWithdraws   crates/engine/vera-embedded/src/host/held.rs Asking::drop,
\*                  which withdraws the ask of a writer whose wait is dropped,
\*                  as the stop's is at its deadline;
\*   Reaps          crates/engine/vera-embedded/src/host.rs HostedNode::stop,
\*                  which spawns the reaper at the deadline;
\*   AbortOnStop    crates/engine/vera-embedded/src/host.rs
\*                  HostedNode::unless_stopped, which polls the `stopping`
\*                  event before the switch's or restart's future, so a set
\*                  event drops that future before it takes another step;
\*   SwitchDeadline the same function's start deadline, which drops a hung
\*                  switch or restart;
\*   ClearsHalted   crates/engine/vera-embedded/src/host/switch.rs settle and
\*                  Settling::drop, which take a halted node out of the slot,
\*                  and record the device the switch was heading to;
\*   RecordsTarget  the same function, which records that device too when
\*                  the switch succeeded and its node runs;
\*   ReportFails    HostedNode::follow and Vera's EmbeddedNode::follow, whose
\*                  final report read can fail after the replacement started;
\*   ObservesCommit HostedNode::follow, which recognizes the replacement by
\*                  the running node's term and records it despite that error;
\*   Restarts       crates/engine/vera-embedded/src/host/restart.rs
\*                  HostedNode::start_again, behind complete_switch and
\*                  restart;
\*   LinkShuts      crates/engine/vera-embedded/src/link.rs Link::open, which
\*                  points the link at a new node only while it is open, in one
\*                  atomic update, so a closed link stays closed;
\*   WaitsRelease   crates/engine/vera-embedded/src/host/release.rs released,
\*                  which HostedNode::stop waits on after the node it took
\*                  stopped: the directory lock Vera's node threads hold
\*                  (vera_node's `.node-hold` claim), which a thread an
\*                  abandoned switch left running still holds.
\* Vera's own steps: crates/vera-node/src/embedded/mod.rs in vera.rs,
\* EmbeddedNode::restart (halt, then launch), halt (send the stop, wait for
\* the exit, join), launch (claim the directory, spawn the thread, wait for
\* it to serve) and Drop (send the stop).
\* Regressions: crates/entry-points/defra-node/tests/embedded_vera/switching.rs
\* two_devices_switch_the_active_role_and_settle_what_the_backup_owed, and
\* crates/entry-points/defra-node/tests/embedded_vera/switch_recovery.rs
\* a_stop_during_a_switch_starts_no_node,
\* a_stop_during_a_switch_waits_for_the_node and
\* a_failed_take_over_is_completed_in_place and
\* a_failed_follow_report_leaves_the_started_backup_running.

EXTENDS Naturals  \* numbers, for counting writers and readers

CONSTANTS
  EvictWatch,     \* TRUE: a watch lets go of the slot when a writer asks
  AskWithdraws,   \* TRUE: a writer that stops waiting withdraws its ask
  AbortOnStop,    \* TRUE: a stop that begins drops a switch or restart
  SwitchDeadline, \* TRUE: a hung switch or restart ends at the start deadline
  ClearsHalted,   \* TRUE: a switch that left the node halted empties the slot
  RecordsTarget,  \* TRUE: a switch that succeeded records its new device
  ReportFails,    \* TRUE: the switch's discarded-submission report read fails
  ObservesCommit, \* TRUE: a failed report still records the replacement that runs
  Restarts,       \* TRUE: the host can start the node again in the slot
  LinkShuts,      \* TRUE: a switch opens the link only while it is not closed
  Reaps,          \* TRUE: a stop that reaches its deadline leaves a reaper
  WaitsRelease    \* TRUE: the stop waits for the directory to be released

\* The two Vera nodes: the one the host started first, and the one a switch
\* or a restart launches.
Nodes == {"old", "new"}

VARIABLES
  slot,       \* which Vera node the slot holds: "old", "new" or "none"
  running,    \* the Vera nodes whose threads still run and hold the directory
  told,       \* the Vera nodes whose threads were told to stop
  device,     \* the device the slot records: "from" (the first) or "to"
  link,       \* which Vera node the transports reach: "old", "new" or "closed"
  stopping,   \* TRUE once the stop began
  asking,     \* how many writers asked for the slot and have not withdrawn
  readers,    \* how many callers hold the slot to read it (0 to 2)
  writer,     \* who writes the slot: "none", "switch", "stop" or "reaper"
  sw,         \* the switch or restart: "idle", "asking", "holding",
              \* "halting", "halted", "launching", "stuck", "started" or "done"
  mode,       \* what `sw` runs: "switch" or "restart"
  lateStart,  \* TRUE once a node was launched after the stop began
  st,         \* the stop: "idle", "asking", "holding", "joining",
              \* "releasing", "done" or "elapsed"
  taken,      \* the Vera node the stop took out of the slot, or "none"
  wa,         \* the watch: "check", "got", "watching", "parked" or "done"
  rc,         \* the read call: "idle", "holding" or "done"
  rp,         \* the reaper: "none", "asking", "done" or "gaveup"
  dropped     \* TRUE once the host dropped the HostedNode

vars == <<slot, running, told, device, link, stopping, asking, readers,
          writer, sw, mode, lateStart, st, taken, wa, rc, rp, dropped>> \* all

TypeOK ==
  /\ slot \in Nodes \cup {"none"}                                      \* a known node
  /\ running \subseteq Nodes                                           \* nodes that run
  /\ told \subseteq Nodes                                              \* nodes told to stop
  /\ device \in {"from", "to"}                                         \* a known device
  /\ link \in {"old", "new", "closed"}                                 \* a known target
  /\ stopping \in BOOLEAN                                              \* a flag
  /\ asking \in 0..3                                                   \* at most every writer
  /\ readers \in 0..2                                                  \* the watch and the call
  /\ writer \in {"none", "switch", "stop", "reaper"}                   \* one writer at most
  /\ sw \in {"idle", "asking", "holding", "halting", "halted",
             "launching", "stuck", "started", "done"}                  \* a known step
  /\ mode \in {"switch", "restart"}                                    \* what runs
  /\ lateStart \in BOOLEAN                                             \* a flag
  /\ st \in {"idle", "asking", "holding", "joining", "releasing",
             "done", "elapsed"}                                        \* a known step
  /\ taken \in Nodes \cup {"none"}                                     \* a known node
  /\ wa \in {"check", "got", "watching", "parked", "done"}             \* a known step
  /\ rc \in {"idle", "holding", "done"}                                \* a known step
  /\ rp \in {"none", "asking", "done", "gaveup"}                       \* a known step
  /\ dropped \in BOOLEAN                                               \* a flag

Init ==
  /\ slot = "old"         \* the first Vera node is in the slot
  /\ running = {"old"}    \* and it runs
  /\ told = {}            \* nobody told it to stop
  /\ device = "from"      \* the slot records the device it runs as
  /\ link = "old"         \* the transports reach it
  /\ stopping = FALSE     \* nobody stops it yet
  /\ asking = 0           \* no writer asks
  /\ readers = 0          \* nobody reads the slot yet
  /\ writer = "none"      \* nobody writes it
  /\ sw = "idle"          \* the host has not asked for a switch
  /\ mode = "switch"      \* and the first one is a switch
  /\ lateStart = FALSE    \* no late start yet
  /\ st = "idle"          \* nor for the stop
  /\ taken = "none"       \* which has taken no node
  /\ wa = "check"         \* the watch is about to look
  /\ rc = "idle"          \* the read call has not begun
  /\ rp = "none"          \* no reaper yet
  /\ dropped = FALSE      \* the host still holds the HostedNode

\* The slot is free to write: no reader and no other writer.
Free == readers = 0 /\ writer = "none"

\* The node in the slot as a set: empty when the slot is empty.
InSlot == IF slot = "none" THEN {} ELSE {slot}

\* The steps of a switch or restart a stop or the deadline may cut short.
Interruptible == {"holding", "halting", "halted", "launching", "stuck"}

\* A step may run: the stop has not begun, or nothing checks for it.
MayLaunch == ~stopping \/ ~AbortOnStop

\* What a switch that ends with no node of its own leaves in the slot, and
\* the device the slot then records: nothing and the device the switch was
\* heading to, with ClearsHalted; or the halted node, as it was.
Leave ==
  /\ slot' = IF ClearsHalted THEN "none" ELSE slot     \* cleared, or left halted
  /\ device' = IF ClearsHalted THEN "to" ELSE device   \* and what it records

\* A Vera node thread told to stop ends by itself, a while later, and lets
\* go of the directory: in Vera it lets the blocks in flight finalize, stops
\* its runtime and drops its claim.
NodeEnds(n) ==
  /\ n \in running /\ n \in told                  \* it runs and was told
  /\ running' = running \ {n}                     \* it ends
  /\ UNCHANGED <<slot, told, device, link, stopping, asking, readers, writer,
                 sw, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* ---- The switch, and the restart that reuses its steps ----

\* The host asks for a switch. A host that began stopping refuses it at once.
SwitchBegin ==
  /\ sw = "idle"                                  \* not asked yet
  /\ IF stopping
       THEN sw' = "done" /\ UNCHANGED asking     \* refused: the host stops
       ELSE sw' = "asking" /\ asking' = asking + 1 \* it asks for the slot
  /\ UNCHANGED <<slot, running, told, device, link, stopping, readers, writer,
                 mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* A switch left the slot empty; the host asks for a restart. Refused at
\* once while stopping, as the switch is.
RestartBegin ==
  /\ Restarts                                     \* the code offers it
  /\ sw = "done" /\ slot = "none" /\ ~stopping    \* nothing runs, no stop
  /\ ~dropped                                     \* the host still has it
  /\ sw' = "asking" /\ asking' = asking + 1       \* it asks for the slot
  /\ mode' = "restart"                            \* as a restart
  /\ UNCHANGED <<slot, running, told, device, link, stopping, readers, writer,
                 lateStart, st, taken, wa, rc, rp, dropped>>

\* It takes the slot once it is free. A restart has no node to halt.
SwitchAcquire ==
  /\ sw = "asking"                                \* it asked
  /\ Free                                         \* nobody holds the slot
  /\ writer' = "switch"                           \* it writes now
  /\ sw' = IF mode = "restart" THEN "halted" ELSE "holding" \* and holds it
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* Vera refuses the switch before it stops anything: the old node runs on.
SwitchRefused ==
  /\ sw = "holding" /\ MayLaunch                  \* holding, still polled
  /\ sw' = "started"                              \* ready to let go
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 writer, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* Vera accepts the certificate and tells the old node to stop. It waits
\* for the thread next; the node runs on until it ends.
SwitchHalt ==
  /\ sw = "holding" /\ MayLaunch                  \* holding, still polled
  /\ told' = told \cup InSlot                     \* the old node is told
  /\ sw' = "halting"                              \* and waited for
  /\ UNCHANGED <<slot, running, device, link, stopping, asking, readers,
                 writer, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* The old node's thread ended and Vera joined it. The node stays in the
\* slot as an object whose thread has ended.
HaltJoined ==
  /\ sw = "halting" /\ MayLaunch                  \* waiting, still polled
  /\ slot \notin running                          \* its thread ended
  /\ sw' = "halted"                               \* the launch comes next
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 writer, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* Vera claims the directory and spawns the new node's thread, which runs
\* from here on. The claim needs a directory no thread holds; a restart
\* asks again until then.
LaunchSpawns ==
  /\ sw = "halted" /\ MayLaunch                   \* halted, still polled
  /\ running = {}                                 \* the directory is free
  /\ running' = {"new"}                           \* the new node runs
  /\ told' = told \ {"new"}                       \* told nothing yet
  /\ lateStart' = (lateStart \/ stopping)         \* late if the stop began
  /\ sw' = "launching"                            \* it waits for it to serve
  /\ UNCHANGED <<slot, device, link, stopping, asking, readers, writer, mode,
                 st, taken, wa, rc, rp, dropped>>

\* Vera refuses the launch before any thread starts (its configuration,
\* its stored term): the halted node is all that is left.
LaunchRefused ==
  /\ sw = "halted" /\ MayLaunch                   \* halted, still polled
  /\ Leave                                        \* cleared, or left halted
  /\ sw' = "started"                              \* ready to let go
  /\ UNCHANGED <<running, told, link, stopping, asking, readers, writer, mode,
                 lateStart, st, taken, wa, rc, rp, dropped>>

\* The new node serves: it takes the slot.
LaunchReady ==
  /\ sw = "launching" /\ MayLaunch                \* launching, still polled
  /\ slot' = "new"                                \* the new node takes the slot
  /\ sw' = "started"                              \* ready to let go
  /\ UNCHANGED <<running, told, device, link, stopping, asking, readers,
                 writer, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* The new node's thread ends by itself before it serves, and Vera joins it.
LaunchFails ==
  /\ sw = "launching" /\ MayLaunch                \* launching, still polled
  /\ running' = running \ {"new"}                 \* its thread ended
  /\ Leave                                        \* cleared, or left halted
  /\ sw' = "started"                              \* ready to let go
  /\ UNCHANGED <<told, link, stopping, asking, readers, writer, mode,
                 lateStart, st, taken, wa, rc, rp, dropped>>

\* The launch hangs: only the deadline or a stop ends it.
LaunchHangs ==
  /\ sw = "launching" /\ MayLaunch                \* launching, still polled
  /\ sw' = "stuck"                                \* it waits
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 writer, mode, lateStart, st, taken, wa, rc, rp, dropped>>

\* The switch is dropped where it stands, by a stop, the deadline or its
\* caller. A node not yet halted runs on in the slot. Past that the slot is
\* left as a failed launch leaves it, and whatever thread Vera was waiting
\* for runs on with nobody waiting for it: the old node, already told to
\* stop, or the new one, told to stop as its stop sender is dropped.
Interrupted ==
  /\ IF sw = "holding"
       THEN UNCHANGED <<slot, device>>            \* nothing was stopped
       ELSE Leave                                 \* what is left
  /\ told' = IF sw \in {"launching", "stuck"}
               THEN told \cup {"new"}             \* the new node is told
               ELSE told                          \* nothing more is told
  /\ sw' = "started"                              \* ready to let go
  /\ UNCHANGED <<running, link, stopping, asking, readers, writer, mode,
                 lateStart, st, taken, wa, rc, rp, dropped>>

\* A stop began: with AbortOnStop the switch is dropped.
SwitchAborted ==
  /\ AbortOnStop /\ stopping                      \* the stop event fired
  /\ sw \in Interruptible                         \* while it runs
  /\ Interrupted                                  \* it is dropped

\* The start deadline passes first.
SwitchElapses ==
  /\ SwitchDeadline                               \* the code bounds it
  /\ sw \in Interruptible                         \* while it runs
  /\ Interrupted                                  \* it is dropped

\* The caller drops the switch future before it returns. The host's drop
\* guard leaves the slot as an interruption does, so a restart can recover.
SwitchCancelled ==
  /\ sw \in Interruptible                         \* while it runs
  /\ Interrupted                                  \* it is dropped

\* The deadline of a switch that hangs passes for sure.
SwitchElapsesStuck == sw = "stuck" /\ SwitchElapses

\* It points the link at the node it started, records the device that node
\* runs as, and lets go of the slot. With LinkShuts a closed link stays
\* closed; without RecordsTarget the slot keeps the device it had.
SwitchRelease ==
  /\ sw = "started"                                     \* it is done
  /\ link' = IF slot = "new" /\ ~(LinkShuts /\ link = "closed")
              THEN "new"                                \* transports follow
              ELSE link                                 \* or stay as they are
  /\ device' = IF slot = "new" /\ RecordsTarget
                         /\ (~ReportFails \/ ObservesCommit)
                THEN "to"                               \* the new role
                ELSE device                             \* or as it was
  /\ writer' = "none"                                   \* it lets go
  /\ asking' = asking - 1                               \* and withdraws its ask
  /\ sw' = "done"                                       \* the call returns
  /\ UNCHANGED <<slot, running, told, stopping, readers, mode, lateStart, st,
                 taken, wa, rc, rp, dropped>>

\* ---- The stop ----

\* The stop begins: from now on no switch starts, and the link is closed.
StopBegin ==
  /\ st = "idle"              \* not begun
  /\ stopping' = TRUE         \* the host stops
  /\ link' = "closed"         \* transports refuse every call
  /\ asking' = asking + 1     \* it asks for the slot
  /\ st' = "asking"           \* and waits for it
  /\ UNCHANGED <<slot, running, told, device, readers, writer, sw, mode,
                 lateStart, taken, wa, rc, rp, dropped>>

\* The stop takes the slot once it is free.
StopAcquire ==
  /\ st = "asking"            \* it asked
  /\ Free                     \* nobody holds the slot
  /\ writer' = "stop"         \* it writes now
  /\ st' = "holding"          \* and holds it
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 sw, mode, lateStart, taken, wa, rc, rp, dropped>>

\* Holding the slot, the stop takes the node out of it and tells it to stop.
StopTake ==
  /\ st = "holding"                                \* it holds the slot
  /\ taken' = slot                                 \* the node it took
  /\ told' = told \cup InSlot                      \* is told to stop
  /\ slot' = "none"                                \* and leaves the slot
  /\ st' = "joining"                               \* the stop waits for it
  /\ UNCHANGED <<running, device, link, stopping, asking, readers, writer, sw,
                 mode, lateStart, wa, rc, rp, dropped>>

\* What the stop does once it is through: it lets go of the slot and
\* withdraws its ask.
StopLetsGo ==
  /\ writer' = "none"         \* the stop lets go
  /\ asking' = asking - 1     \* and withdraws its ask

\* The node the stop took has ended, and Vera joined its thread. With
\* WaitsRelease the stop then waits for the directory; without, it returns.
StopJoined ==
  /\ st = "joining"                                \* it waits for its node
  /\ taken \notin running                          \* which ended
  /\ IF WaitsRelease
       THEN st' = "releasing" /\ UNCHANGED <<writer, asking>> \* wait more
       ELSE st' = "done" /\ StopLetsGo                        \* or return
  /\ UNCHANGED <<slot, running, told, device, link, stopping, readers, sw,
                 mode, lateStart, taken, wa, rc, rp, dropped>>

\* No Vera node thread holds the directory any more: the stop returns.
StopReleased ==
  /\ st = "releasing"                              \* it waits for the directory
  /\ running = {}                                  \* no thread holds it
  /\ st' = "done"                                  \* the stop returns
  /\ StopLetsGo                                    \* and lets go
  /\ UNCHANGED <<slot, running, told, device, link, stopping, readers, sw,
                 mode, lateStart, taken, wa, rc, rp, dropped>>

\* The stop's deadline passes while it waits, for the slot, its node or the
\* directory: it returns. Whatever it held is let go. With AskWithdraws its
\* ask goes with it; without, the ask stays behind. With Reaps a reaper
\* asks for the slot in its place.
StopElapses ==
  /\ st \in {"asking", "joining", "releasing"}              \* still waiting
  /\ writer' = IF st = "asking" THEN writer ELSE "none"     \* held: let go
  /\ asking' = (IF AskWithdraws THEN asking - 1 ELSE asking) \* withdrawn, or left
              + (IF Reaps THEN 1 ELSE 0)                    \* and the reaper asks
  /\ rp' = IF Reaps THEN "asking" ELSE "none"               \* the reaper starts
  /\ st' = "elapsed"                                        \* the stop returns
  /\ UNCHANGED <<slot, running, told, device, link, stopping, readers, sw,
                 mode, lateStart, taken, wa, rc, dropped>>

\* The reaper takes the slot once it is free, drops the node it finds there,
\* which tells it to stop, and lets go, in one step.
ReaperTakes ==
  /\ rp = "asking"                \* it asked
  /\ Free                         \* nobody holds the slot
  /\ told' = told \cup InSlot     \* the node is told to stop
  /\ slot' = "none"               \* and leaves the slot
  /\ asking' = asking - 1         \* the reaper withdraws its ask
  /\ rp' = "done"                 \* and ends
  /\ UNCHANGED <<running, device, link, stopping, readers, writer, sw, mode,
                 lateStart, st, taken, wa, rc, dropped>>

\* The reaper's own deadline passes first: it withdraws its ask and ends,
\* leaving the node to the host's drop. Once the host dropped it too, the
\* reaper held the last share of the slot, which goes with it.
ReaperGivesUp ==
  /\ rp = "asking"                \* it still waits
  /\ asking' = asking - 1         \* it withdraws its ask
  /\ rp' = "gaveup"               \* and ends
  /\ IF dropped
       THEN slot' = "none" /\ told' = told \cup InSlot \* the slot goes
       ELSE UNCHANGED <<slot, told>>                   \* the host has it
  /\ UNCHANGED <<running, device, link, stopping, readers, writer, sw, mode,
                 lateStart, st, taken, wa, rc, dropped>>

\* The host drops the HostedNode once its stop returned and none of its
\* calls runs. Without a reaper holding it, the slot goes too, and the node
\* in it is told to stop.
HostDrop ==
  /\ st \in {"done", "elapsed"} /\ ~dropped            \* stopped, still held
  /\ sw \in {"idle", "done"} /\ rc # "holding"         \* no call borrows it
  /\ wa = "done"                                       \* nor the watch
  /\ dropped' = TRUE                                   \* the host drops it
  /\ IF rp = "asking"
       THEN UNCHANGED <<slot, told>>                   \* the reaper has it
       ELSE /\ slot' = "none"                          \* the slot goes
            /\ told' = told \cup InSlot                \* its node is told
  /\ UNCHANGED <<running, device, link, stopping, asking, readers, writer, sw,
                 mode, lateStart, st, taken, wa, rc, rp>>

\* ---- The read call ----

\* A handover, resume or takeover change reads the slot when no writer holds it.
CallAcquire ==
  /\ rc = "idle"              \* not begun
  /\ writer = "none"          \* no writer holds the slot
  /\ readers' = readers + 1   \* it reads
  /\ rc' = "holding"          \* until it ends
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, wa, rp, dropped>>

\* Vera ends the call by itself, however long it takes.
CallEnds ==
  /\ rc = "holding"           \* it reads
  /\ readers' = readers - 1   \* it lets go
  /\ rc' = "done"             \* and returns
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, wa, rp, dropped>>

\* ---- The watch ----

\* The watch takes the slot to read it whenever no writer holds it.
WatchAcquire ==
  /\ wa = "check"             \* about to look
  /\ writer = "none"          \* no writer holds the slot
  /\ readers' = readers + 1   \* it reads
  /\ wa' = "got"              \* and looks again
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, rc, rp, dropped>>

\* Holding the slot, the watch lets go and parks when a writer asks, returns
\* nothing when the slot is empty, and watches the node otherwise.
WatchLook ==
  /\ wa = "got"                                         \* it holds the slot
  /\ IF asking > 0
       THEN readers' = readers - 1 /\ wa' = "parked"    \* a writer goes first
       ELSE IF slot = "none"
              THEN readers' = readers - 1 /\ wa' = "done" \* stopped: no answer
              ELSE wa' = "watching" /\ UNCHANGED readers \* it watches the node
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, rc, rp, dropped>>

\* A writer asks while the watch watches. With EvictWatch the watch hears it
\* and lets go; without, it keeps the slot.
WatchEvicted ==
  /\ wa = "watching"          \* it watches
  /\ asking > 0               \* a writer asks
  /\ EvictWatch               \* and the watch hears the ask
  /\ readers' = readers - 1   \* it lets go
  /\ wa' = "check"            \* and looks again later
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, rc, rp, dropped>>

\* No writer asks any more: the parked watch wakes and looks again.
WatchWakes ==
  /\ wa = "parked"            \* it waits for the writers
  /\ asking = 0               \* none asks now
  /\ wa' = "check"            \* it looks again
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, readers,
                 writer, sw, mode, lateStart, st, taken, rc, rp, dropped>>

\* What the watch waits for happens: another device took the role.
WatchResolves ==
  /\ wa = "watching"          \* it watches
  /\ readers' = readers - 1   \* it lets go
  /\ wa' = "done"             \* and returns the answer
  /\ UNCHANGED <<slot, running, told, device, link, stopping, asking, writer,
                 sw, mode, lateStart, st, taken, rc, rp, dropped>>

\* Every launch outcome, for the fairness that some outcome happens.
Launch == LaunchSpawns \/ LaunchRefused \/ LaunchReady \/ LaunchFails \/ LaunchHangs

\* Every step that ends what the stop waits for.
StopWaits == StopJoined \/ StopReleased \/ StopElapses

Next ==
  \/ SwitchBegin \/ RestartBegin \/ SwitchAcquire                       \* asking
  \/ SwitchRefused \/ SwitchHalt \/ HaltJoined \/ Launch                \* switching
  \/ SwitchAborted \/ SwitchElapses \/ SwitchCancelled \/ SwitchRelease \* ending
  \/ StopBegin \/ StopAcquire \/ StopTake \/ StopWaits                  \* stop
  \/ ReaperTakes \/ ReaperGivesUp \/ HostDrop                           \* after it
  \/ \E n \in Nodes : NodeEnds(n)                                       \* threads end
  \/ CallAcquire \/ CallEnds                                            \* read call
  \/ WatchAcquire \/ WatchLook \/ WatchEvicted \/ WatchWakes \/ WatchResolves \* watch

\* Fairness: each step of the switch and the stop that can run does; a
\* thread told to stop ends; a launch has some outcome, and one that keeps
\* being tried spawns its thread and serves; a hung launch reaches its
\* deadline and a stop drops it; the stop gets the slot, its node and the
\* directory or reaches its deadline; the reaper gets the slot or gives up;
\* Vera ends a read call; the watch keeps looking and hears an ask; a host
\* whose switch left no node asks for a restart. Whether the host asks for
\* a switch, a stop or a drop, and whether another device takes the role,
\* are not promised.
Spec ==
  /\ Init                                                     \* start
  /\ [][Next]_vars                                            \* then take steps
  /\ WF_vars(SwitchAcquire) /\ WF_vars(SwitchRefused \/ SwitchHalt) \* it moves
  /\ WF_vars(HaltJoined)                                      \* it joins
  /\ WF_vars(NodeEnds("old")) /\ WF_vars(NodeEnds("new"))     \* threads end
  /\ WF_vars(Launch) /\ SF_vars(LaunchSpawns) /\ SF_vars(LaunchReady) \* it launches
  /\ WF_vars(SwitchElapsesStuck) /\ WF_vars(SwitchAborted)    \* it is cut short
  /\ WF_vars(SwitchRelease) /\ WF_vars(RestartBegin)          \* it ends
  /\ WF_vars(StopAcquire \/ StopElapses) /\ WF_vars(StopTake) \* the stop goes on
  /\ WF_vars(StopWaits)                                       \* and ends
  /\ WF_vars(ReaperTakes \/ ReaperGivesUp)                    \* the reaper ends
  /\ WF_vars(CallEnds)                                        \* a call ends
  /\ WF_vars(WatchAcquire) /\ WF_vars(WatchLook)              \* the watch looks
  /\ WF_vars(WatchEvicted) /\ WF_vars(WatchWakes)             \* and yields

\* ---- What must hold ----

\* Whether the stop has returned, with the node or at its deadline.
Returned == st \in {"done", "elapsed"}

\* Once the stop returned, no transport reaches a Vera node.
INV_ClosedAfterStop == Returned => link = "closed"

\* No switch or restart launches a node once the stop began.
INV_NoStartAfterStop == ~lateStart

\* While nobody writes the slot, a node in it runs.
INV_SlotRuns == (writer = "none" /\ slot # "none") => slot \in running

\* A stop that says it stopped the node leaves no Vera node thread running
\* and holding the directory.
INV_StoppedMeansEnded == st = "done" => running = {}

\* While nobody writes the slot, it records the device its node runs as:
\* the first device for the first node, the new one for the node a switch
\* or restart launched.
INV_DeviceNamesNode ==
  writer = "none" =>
    /\ (slot = "old" => device = "from")          \* the first node's role
    /\ (slot = "new" => device = "to")            \* the new node's role

\* After the stop returned every node ends, unless the reaper gave up.
LIVE_NoNodeAfterStop == Returned ~> (running = {} \/ rp = "gaveup")

\* Once the host dropped the HostedNode every node ends.
LIVE_DropStops == dropped ~> (running = {})

\* A switch or restart that asked for the slot ends.
LIVE_SwitchEnds == (sw = "asking") ~> (sw = "done")

\* A parked watch wakes again.
LIVE_WatchWakes == (wa = "parked") ~> (wa # "parked")

\* A stop that began returns.
LIVE_StopReturns == (st # "idle") ~> Returned

\* A switch that left no node gets one back, unless a stop begins.
LIVE_Recovers ==
  (sw = "done" /\ slot = "none" /\ ~stopping) ~> (slot # "none" \/ stopping)

====
