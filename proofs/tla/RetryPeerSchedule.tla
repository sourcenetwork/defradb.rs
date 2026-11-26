---- MODULE RetryPeerSchedule ----
\* The story. A sender owes one replicator peer some document scopes. Each
\* owed scope has a durable retry marker, and the peer has one durable
\* schedule row. The retry pass finds the peers it visits by their schedule
\* rows (crates/engine/storage/src/stores/peerstore.rs Peerstore::get_retry_peers),
\* so a marker whose peer has no row is never retried until something writes
\* the row again.
\*
\* Three kinds of transaction touch the markers and the row. Each is an
\* optimistic regolith transaction: it reads at a snapshot taken when it
\* begins, and at commit it is aborted if a key it validated was written by
\* another commit since then; the code then runs it again.
\*   - a registration (peerstore.rs RegisterScopeMarkerAttempt::run) writes one
\*     scope's marker and, when it read no row, the row;
\*   - an acknowledgement (peerstore.rs PeerstoreRetryAttempt::CompleteMarker)
\*     deletes one scope's marker, looks for any other marker of the peer, and
\*     when it finds none deletes the row;
\*   - an empty-peer clear (peerstore.rs PeerstoreRetryAttempt::ClearPeer, run
\*     at the end of a retry pass by crates/surfaces/p2p-adapter/src/retry.rs
\*     record_peer_finished) looks for any marker and when it finds none
\*     deletes the row.
\* Registrations and acknowledgements of different scopes run at once (the
\* recorder's lanes, retry.rs spawn_failure_recorder), and the retry pass runs
\* beside them.
\*
\* The look for "any other marker" is a scan of the peer's marker keys
\* (peerstore.rs scope_markers_left). A plain scan does not notice a marker
\* that another transaction commits into the scanned range after the snapshot.
\*
\* What must never happen: a marker is durable and the peer has no row
\* (INV_EveryMarkerScheduled).
\*
\* The guard: when the scan finds no marker, it is a validated scan, so a
\* marker committed into the range meanwhile aborts the transaction
\* (ValidateEmptyScan). A scan that finds a marker deletes nothing, so it is
\* not validated and never adds a conflict.

\* Naturals gives the numbers 0, 1 and 2 the keys are named by.
EXTENDS Naturals

\* The one switch the configurations flip.
CONSTANTS
  ValidateEmptyScan \* guard: a scan that found no marker is validated at commit

\* The guard is either on or off.
ASSUME ValidateEmptyScan \in BOOLEAN

\* Two scopes are enough for one to be acknowledged while the other registers.
Scopes == {1, 2}
\* The schedule row's key, distinct from every scope's marker key.
Row == 0
\* Every durable key the model tracks: the markers and the row.
Keys == Scopes \cup {Row}

\* The actors: one registration and one acknowledgement per scope, and one
\* clear. A clear owns no scope, so its scope field is 0. A transaction that
\* aborts runs again as a new Begin of the same actor.
Actors == {[kind |-> k, s |-> s] : k \in {"reg", "ack"}, s \in Scopes}
          \cup {[kind |-> "clear", s |-> 0]}

VARIABLES
  marker,  \* marker[s]: scope s's marker key exists (durable)
  row,     \* the peer's schedule row exists (durable)
  running, \* running[a]: actor a has begun a transaction and not ended it
  seenM,   \* seenM[a]: the markers actor a's snapshot holds
  seenR,   \* seenR[a]: whether actor a's snapshot holds the row
  dirty    \* dirty[a]: keys other commits wrote since actor a began

\* Every variable, for steps that change nothing.
vars == <<marker, row, running, seenM, seenR, dirty>>

\* At the start nothing is owed, nothing is scheduled, nothing runs.
Init == /\ marker = [s \in Scopes |-> FALSE]                \* no marker
        /\ row = FALSE                                       \* no row
        /\ running = [a \in Actors |-> FALSE]                \* nobody running
        /\ seenM = [a \in Actors |-> [s \in Scopes |-> FALSE]] \* empty snapshots
        /\ seenR = [a \in Actors |-> FALSE]                  \* no row seen
        /\ dirty = [a \in Actors |-> {}]                     \* nothing moved

\* Whether actor a's snapshot holds no marker other than scope `except`'s.
\* An acknowledgement passes its own scope, since it deletes that marker
\* itself; a clear passes 0, which names no scope.
SawNoOther(a, except) == \A t \in Scopes \ {except} : ~seenM[a][t]

\* The keys actor a validates at commit. A registration point-reads its marker
\* and the row; an acknowledgement point-reads its marker and the row; a clear
\* point-reads the row. A scan that found no marker covers every marker key
\* when the guard is on, as a validated range does; otherwise it adds none.
Validates(a) ==
  LET scanned == IF ValidateEmptyScan /\ SawNoOther(a, a.s) THEN Scopes ELSE {} \* the validated range, if any
  IN CASE a.kind = "reg" -> {a.s, Row}              \* registration's point reads
       [] a.kind = "ack" -> {a.s, Row} \cup scanned \* ack's point reads and its scan
       [] OTHER -> {Row} \cup scanned               \* clear's point read and its scan

\* The keys actor a writes if it commits, decided from its snapshot.
Writes(a) ==
  CASE a.kind = "reg" -> {a.s} \cup (IF seenR[a] THEN {} ELSE {Row})  \* its marker, and the row if it saw none
    [] a.kind = "ack" -> {a.s} \cup (IF SawNoOther(a, a.s) /\ seenR[a] THEN {Row} ELSE {}) \* its marker, and the row if no other marker
    [] OTHER -> IF SawNoOther(a, 0) /\ seenR[a] THEN {Row} ELSE {}    \* the row, if no marker at all

\* Actor a begins: it takes a snapshot. An acknowledgement begins only for a
\* scope whose marker exists, as a peer acknowledges only what was pushed.
Begin(a) == /\ ~running[a]                                 \* not already running
            /\ a.kind = "ack" => marker[a.s]               \* an ack needs its marker
            /\ running' = [running EXCEPT ![a] = TRUE]     \* now running
            /\ seenM' = [seenM EXCEPT ![a] = marker]       \* snapshot of the markers
            /\ seenR' = [seenR EXCEPT ![a] = row]          \* snapshot of the row
            /\ dirty' = [dirty EXCEPT ![a] = {}]           \* nothing moved yet
            /\ UNCHANGED <<marker, row>>                   \* a begin writes nothing

\* Actor a commits: nothing it validated was written since it began. Its
\* writes land, and every other running transaction learns which keys moved.
Commit(a) == /\ running[a]                                 \* it is running
             /\ Validates(a) \cap dirty[a] = {}            \* no validated key moved
             /\ marker' = CASE a.kind = "reg" -> [marker EXCEPT ![a.s] = TRUE]  \* registration adds its marker
                            [] a.kind = "ack" -> [marker EXCEPT ![a.s] = FALSE] \* ack removes its marker
                            [] OTHER -> marker                                  \* clear leaves markers alone
             /\ row' = IF Row \in Writes(a) THEN a.kind = "reg" ELSE row \* a registration writes the row, the others delete it
             /\ running' = [running EXCEPT ![a] = FALSE]   \* it is done
             /\ dirty' = [b \in Actors |->                 \* tell everyone still running
                            IF b # a /\ running[b] THEN dirty[b] \cup Writes(a) ELSE dirty[b]]
             /\ UNCHANGED <<seenM, seenR>>                 \* snapshots do not move

\* Actor a's commit finds a validated key written since it began: regolith
\* reports a conflict and nothing lands. The code runs it again (a new Begin).
Abort(a) == /\ running[a]                                  \* it is running
            /\ Validates(a) \cap dirty[a] # {}             \* a validated key moved
            /\ running' = [running EXCEPT ![a] = FALSE]    \* it ends, writing nothing
            /\ UNCHANGED <<marker, row, seenM, seenR, dirty>> \* nothing durable changes

\* Any actor can take its next step.
Next == \E a \in Actors : Begin(a) \/ Commit(a) \/ Abort(a)

\* Any interleaving at all.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its intended range.
TypeOK == /\ marker \in [Scopes -> BOOLEAN]                \* markers are flags
          /\ row \in BOOLEAN                               \* the row is a flag
          /\ running \in [Actors -> BOOLEAN]               \* running is a flag per actor
          /\ seenM \in [Actors -> [Scopes -> BOOLEAN]]     \* a snapshot of markers per actor
          /\ seenR \in [Actors -> BOOLEAN]                 \* a snapshot of the row per actor
          /\ dirty \in [Actors -> SUBSET Keys]             \* moved keys per actor

\* The promise: every durable marker has the schedule row the retry pass
\* finds its peer by.
INV_EveryMarkerScheduled == \A s \in Scopes : marker[s] => row
====
