---- MODULE BackupExportAccess ----
\* The story: a caller asks a node for a backup export, the JSON of its
\* documents. The request can come in four ways: the embedded node, the HTTP
\* API (the node's own server or `defradb start`'s), the CLI (which calls
\* the HTTP API) and FFI. With node ACP on, the export needs the caller to
\* hold the `read-document` node permission.
\*
\* Who acts: one export request per way in. Whether the caller holds the
\* permission is chosen up front, so the model explores both.
\*
\* What happens: the HTTP API checks the permission at its route before it
\* calls the core; every way in then calls the core's export, which checks
\* the permission itself before it reads any document.
\*
\* What must never happen: a way in reads documents for a caller who lacks
\* the permission, or two ways in answer the same caller differently (one
\* refuses while another exports).
\*
\* Code it models:
\*   crates/database/db/src/backup/export.rs export_database_as  the core's
\*     check, then the read
\*   crates/entry-points/defra-node/src/ops/backup.rs NodeOps::export_backup,
\*     crates/entry-points/cli/src/backup_adapter.rs BackupAdapter::export and
\*     crates/entry-points/ffi/src/backup/export.rs defra_backup_export  the
\*     ways in, which only forward to the core
\*   crates/surfaces/http/src/handlers/backup.rs export  the route's own check
EXTENDS Naturals

CONSTANTS
  CoreChecks,   \* TRUE when the core's export checks the permission
  Forwarding    \* the ways in that call the core's export (the rest read on their own)

\* The four ways an export reaches a node.
Edges == {"node", "http", "cli", "ffi"}

\* The ways in that check the permission at the HTTP route first.
RouteChecked == {"http", "cli"}

ASSUME CoreChecks \in BOOLEAN
ASSUME Forwarding \subseteq Edges

\* Where a request is: at the route, in the core, finished.
Steps == {"route", "core", "done"}

VARIABLES
  allowed,  \* whether the caller holds `read-document`
  step,     \* step[e]: how far the request through way in e got
  read,     \* read[e]: the request through e read the documents
  refused   \* refused[e]: the request through e ended with `not authorized`

vars == <<allowed, step, read, refused>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ allowed \in BOOLEAN
  /\ step \in [Edges -> Steps]
  /\ read \in [Edges -> BOOLEAN]
  /\ refused \in [Edges -> BOOLEAN]

\* The caller may or may not hold the permission; nothing has run yet.
Init ==
  /\ allowed \in BOOLEAN
  /\ step = [e \in Edges |-> "route"]
  /\ read = [e \in Edges |-> FALSE]
  /\ refused = [e \in Edges |-> FALSE]

\* The HTTP route checks the permission; the other ways in have no route
\* and go straight on.
Route(e) ==
  /\ step[e] = "route"
  /\ IF e \in RouteChecked /\ ~allowed
       THEN /\ refused' = [refused EXCEPT ![e] = TRUE]
            /\ step' = [step EXCEPT ![e] = "done"]
       ELSE /\ step' = [step EXCEPT ![e] = "core"]
            /\ UNCHANGED refused
  /\ UNCHANGED <<allowed, read>>

\* The export itself. A way in that forwards runs the core's check when the
\* core has one; a way in that does not forward reads with no check at all
\* (the copy a RED configuration plants).
Export(e) ==
  /\ step[e] = "core"
  /\ LET checked == e \in Forwarding /\ CoreChecks IN
     IF checked /\ ~allowed
       THEN /\ refused' = [refused EXCEPT ![e] = TRUE]
            /\ UNCHANGED read
       ELSE /\ read' = [read EXCEPT ![e] = TRUE]
            /\ UNCHANGED refused
  /\ step' = [step EXCEPT ![e] = "done"]
  /\ UNCHANGED allowed

\* Some way in takes its next step.
Next == \E e \in Edges : Route(e) \/ Export(e)

\* Every request finished stays finished.
Done == (\A e \in Edges : step[e] = "done") /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Done]_vars

\* No way in reads the documents for a caller without the permission.
INV_NoReadWithoutPermission ==
  \A e \in Edges : read[e] => allowed

\* Once every request has finished, all four ways in gave the caller the
\* same answer.
INV_SameAnswer ==
  (\A e \in Edges : step[e] = "done") =>
    \A e, f \in Edges : refused[e] = refused[f] /\ read[e] = read[f]
====
