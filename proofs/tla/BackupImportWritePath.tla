---- MODULE BackupImportWritePath ----
\* The story, in plain words.
\*
\* A caller hands a node a backup file: a list of documents, each carrying
\* the ID it had where it was exported (its alias). The node creates every
\* document afresh and records the file's ID as an alias of the new one.
\*
\* The caller reaches the node through one of four edges: the in-process
\* Node, the HTTP server a NodeBuilder starts, the HTTP server
\* `defradb start` starts, and the C bindings (FFI). Each edge has its own
\* request path for a write: the way a create sent through that edge is
\* admitted, signed, owned and broadcast. The import must write through
\* that same path, so an imported document is exactly what a create of it
\* through the same edge would have been.
\*
\* Who acts:
\*   * the import driver, which walks the file and gathers consecutive
\*     documents into a batch, then saves the batch in three separate steps:
\*     it checks that no alias of the batch is held, it creates the batch's
\*     documents in one request (all or nothing), and it registers their
\*     aliases in one transaction. A batch refused at the check or the
\*     create saved nothing, so its documents are then saved one at a time
\*     the same way, up to the first one refused. A registration refused by
\*     an alias taken since the check is redone one document at a time, so
\*     the others still get theirs;
\*   * another writer, which may claim an alias at any moment, between any
\*     two of those steps;
\*   * the node's shutdown, which stops admitting requests and then drains:
\*     the drain finishes once no admitted request is still running, and the
\*     database closes after it;
\*   * the Node import's embedded caller, which may stop waiting for the
\*     import at any moment (a timeout or a select around it).
\*
\* What must never happen:
\*   1. an imported document signed otherwise than a create would be, or
\*      not broadcast - INV_ImportSigned, INV_ImportReplicated;
\*   2. an import write that runs through a request path other than its
\*      edge's own - INV_HostFromEdge;
\*   3. the driver itself running on a database worker on the Node or HTTP
\*      edges (only its writes and its store checks go there) - INV_DriverOnCaller;
\*   4. two imported documents sharing an alias, or one taking an alias
\*      already in the store - INV_NoDuplicateAlias;
\*   5. an alias another writer holds being taken over - INV_AliasNeverStolen;
\*   6. an FFI import inside an explicit transaction writing anything
\*      (imports run on a node only) - INV_FfiTxnRefused;
\*   7. an imported document owned by someone a create would not be owned
\*      by - INV_CreatorAsCreate;
\*   8. a Node import that ended, or was dropped when its caller stopped
\*      waiting, with a saved document whose alias nobody holds -
\*      INV_SavedRegistered;
\*   9. the Node's drain finishing while an import still runs -
\*      INV_DrainWaitsForImport;
\*  10. a failed import that lost a document before the one it failed at -
\*      INV_KeepsPrefix.
\*
\* Each guard has a RED configuration that removes it:
\*   * OwnMutator = FALSE: writes go through the edge's host (Red_OwnMutator);
\*   * KeepSigningChoice: the host keeps the edge's signer (Red_NoSigningChoice);
\*   * AliasInTxn: registration reads validated (Red_AliasOutsideTxn);
\*   * CheckBeforeCreate: a batch's aliases are checked before its create
\*     (Red_AliasCheckAfterCreate);
\*   * FfiOwnPath: FFI writes through its DocumentCall (Red_FfiViaNode);
\*   * KeepCaller: the host acts as the caller (Red_CallerIgnored);
\*   * HoldImport: the Node import is one admitted request that its writes
\*     continue (Red_WritesAdmittedAlone);
\*   * DetachedImport: the Node import runs as a network task of its own,
\*     which goes on to its end when its caller stops waiting
\*     (Red_ImportOnCallerTask);
\*   * AnonAsCreate: an anonymous import acts as an anonymous create (Red_ActorNoOne);
\*   * AdapterTakesHost: the cli adapter forwards the handler's host (Red_AdapterOwnHost);
\*   * DriverOnCaller: import_backup_with runs the driver where it is called
\*     (Red_PrepareOnPool);
\*   * FlushOnPendingAlias: a batch is saved before a document whose alias
\*     it holds joins it (Red_BatchSkipsPendingAlias);
\*   * OneByOneOnRefusal: a refused batch is saved again one document at a
\*     time (Red_RefusedBatchDropped);
\*   * RegisterEachOnRefusal: a registration refused by one taken alias
\*     registers every other document's alias on its own
\*     (Red_RegistrationAllOrNothing);
\*   * RequireNode: the FFI refuses a transaction handle (Red_FfiTxnAccepted).
\*
\* What is abstracted: a document is its alias; relation fields are left out
\* (a batch is also saved before a document whose relation points into it,
\* so the target's new ID is known, which changes no alias rule); a create
\* is refused only by a held alias or by shutdown; one import per run; the
\* edge, the caller, whether the node signs and the FFI target are picked at
\* the start; the drain's deadline is not modelled (past it the database
\* closes under any request still running, the import's as any other's);
\* only the Node edge's admission and its caller stopping are modelled.
\*
\* Rust: crates/database/db/src/backup/import.rs import_database_as, save,
\* create, record; crates/database/db/src/backup/host.rs ImportHost,
\* RunnerHost; crates/database/db/src/backup/batch.rs Batch::must_save_before;
\* crates/database/db/src/backup/alias.rs register, check_free;
\* crates/entry-points/defra-node/src/ops/backup.rs Node::import_backup_as,
\* Node::import_admitted, NodeOps::import_backup_with, NodeHost;
\* crates/entry-points/defra-node/src/lib.rs Node::as_request, Admit;
\* crates/entry-points/defra-node/src/request_path.rs RequestPath::execute,
\* RequestPath::admitted; crates/foundation/edge/src/handle.rs Edge::spawn,
\* Edge::spawn_continuation;
\* crates/entry-points/defra-node/src/ops/http_ops.rs HttpNodeOps::import;
\* crates/surfaces/http/src/handlers/backup.rs import;
\* crates/surfaces/http/src/request_path.rs Server;
\* crates/entry-points/cli/src/backup_adapter.rs BackupAdapter::import;
\* crates/entry-points/ffi/src/backup/import.rs defra_backup_import, FfiHost.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  OwnMutator,          \* TRUE: the driver writes with a mutator of its own (the bug)
  KeepSigningChoice,   \* TRUE: the host signs as the edge would
  AliasInTxn,          \* TRUE: alias registration is a validated read-then-write
  CheckBeforeCreate,   \* TRUE: a batch's aliases are checked before it is created
  FfiOwnPath,          \* TRUE: the FFI writes through its own DocumentCall
  KeepCaller,          \* TRUE: the host acts as the caller who imports
  HoldImport,          \* TRUE: a Node import is one admitted request its writes continue
  DetachedImport,      \* TRUE: a Node import runs as a task of its own, past its caller
  AnonAsCreate,        \* TRUE: an anonymous import acts as an anonymous create does
  AdapterTakesHost,    \* TRUE: the cli adapter writes through the handler's host
  DriverOnCaller,      \* TRUE: import_backup_with runs the driver on its caller
  FlushOnPendingAlias, \* TRUE: a batch is saved before a document whose alias it holds
  OneByOneOnRefusal,   \* TRUE: a refused batch is saved again one document at a time
  RegisterEachOnRefusal, \* TRUE: a refused registration is redone one document at a time
  RequireNode          \* TRUE: the FFI import refuses a transaction handle

\* The backup files a run may import, each a sequence of aliases: one alias
\* twice, one alias the other writer may claim, and the alias the store
\* holds first and then last.
Files == {<<"a", "a">>, <<"a", "c">>, <<"p", "b">>, <<"b", "p">>}
\* The aliases the store holds before the import.
Preexisting == {"p"}
\* The alias the other writer may claim.
Contested == "c"
\* The four edges a caller imports through.
Edges == {"Node", "HttpNodeBuilder", "HttpCliStart", "Ffi"}
\* Who imports: no one, or a named identity.
Callers == {"anon", "alice"}
\* A document number: its position in the file.
Docs(f) == 1..Len(f)
\* The smallest number of a non-empty set.
Min(s) == CHOOSE x \in s : \A y \in s : x <= y

VARIABLES
  edge,       \* the edge the import came through
  caller,     \* who imports
  signs,      \* TRUE when the node signs with its identity
  target,     \* the FFI call's target: "node" or "txn"
  file,       \* the file being imported
  phase,      \* "start", "run", "done", "failed" or "dropped"
  driverOn,   \* where the driver runs: "caller" or "worker"
  next,       \* the next document of the file to look at
  batch,      \* the documents gathered and not yet being saved
  unit,       \* the documents being saved now: a batch, or one of it
  rest,       \* when saving one at a time, the batch's documents not yet done
  single,     \* TRUE while a refused batch is saved one document at a time
  stage,      \* the step of saving `unit`: "idle", "check", "save" or "register"
  store,      \* each alias's owner: "imported", "w" (the other writer), "old" or "none"
  committed,  \* the saved documents, each a record of how it was written
  admitting,  \* TRUE while the node admits requests
  drained,    \* TRUE once the node's drain finished (the database closes next)
  claimed,    \* TRUE once the other writer holds the contested alias
  failedAt    \* the document a failed import stopped at, or 0

\* Every variable, for stuttering steps.
vars == <<edge, caller, signs, target, file, phase, driverOn, next, batch,
          unit, rest, single, stage, store, committed, admitting, drained,
          claimed, failedAt>>

\* Every alias any file or the store may name.
Aliases == UNION {{f[i] : i \in DOMAIN f} : f \in Files} \cup Preexisting \cup {Contested}

\* The request path an import write runs through on edge `e`.
HostOf(e) ==
  \* the cli adapter's own runner, when it ignores the handler's host
  IF e = "HttpCliStart" /\ ~AdapterTakesHost THEN "adapter"
  \* the Node's path, when the FFI borrows it instead of its DocumentCall
  ELSE IF e = "Ffi" /\ ~FfiOwnPath THEN "Node"
  \* otherwise the edge's own path
  ELSE e

\* Who signs a create sent by caller `c` on a node that signs when `s`: the
\* caller's key when it names one, else the node's.
CreateSigner(c, s) ==
  \* a node that does not sign leaves every block unsigned
  IF ~s THEN "none"
  \* an anonymous caller's create is signed by the node
  ELSE IF c = "anon" THEN "node"
  \* a named caller signs with its own key
  ELSE c

\* Who owns a create sent through edge `e` by caller `c`: the caller when it
\* names one; an anonymous create on a signing Node acts as the node; an
\* anonymous create anywhere else is owned by no one.
CreateCreator(e, c, s) ==
  \* a named caller owns what it creates
  IF c # "anon" THEN c
  \* the Node fills in its own identity for an anonymous request
  ELSE IF e = "Node" /\ s THEN "node"
  \* elsewhere an anonymous create is owned by no one
  ELSE "none"

\* Who signs an import write: what the path it runs through does.
ImportSigner(e, c, s) ==
  \* the driver's own mutator signs nothing
  IF OwnMutator THEN "none"
  \* the adapter's runner has no signing session
  ELSE IF HostOf(e) = "adapter" THEN "none"
  \* a host that drops the edge's signer signs with the node's key only
  ELSE IF ~KeepSigningChoice THEN (IF s THEN "node" ELSE "none")
  \* otherwise the host signs as a create on its path would
  ELSE CreateSigner(c, s)

\* Who owns an import write.
ImportCreator(e, c, s) ==
  \* an anonymous import that skips the edge's default acts as no one
  IF c = "anon" /\ ~AnonAsCreate THEN "none"
  \* a host that drops the caller acts as an anonymous create would
  ELSE IF ~KeepCaller THEN CreateCreator(HostOf(e), "anon", s)
  \* otherwise as a create through the same path
  ELSE CreateCreator(HostOf(e), c, s)

\* Whether the write is broadcast to replicators: every request path
\* broadcasts, the driver's own mutator does not.
Broadcast == ~OwnMutator

\* Every variable holds a value of its kind.
TypeOK ==
  \* the edge, caller and target come from their sets
  /\ edge \in Edges /\ caller \in Callers /\ target \in {"node", "txn"}
  \* the phase, the driver's place and the saving step are one of theirs
  /\ phase \in {"start", "run", "done", "failed", "dropped"}
  /\ driverOn \in {"caller", "worker"}
  /\ stage \in {"idle", "check", "save", "register"}
  \* every alias has an owner or none
  /\ store \in [Aliases -> {"imported", "w", "old", "none"}]
  \* the documents in flight stay within the file
  /\ batch \subseteq Docs(file) /\ unit \subseteq Docs(file)
  /\ rest \subseteq Docs(file) /\ next \in 1..(Len(file) + 1)
  \* the stop is a document of the file, or none
  /\ failedAt \in 0..Len(file)

\* The run's start.
Init ==
  \* any edge, caller, signing setting, FFI target and file
  /\ edge \in Edges /\ caller \in Callers /\ signs \in BOOLEAN
  /\ target \in {"node", "txn"} /\ file \in Files
  \* the import has not started
  /\ phase = "start" /\ driverOn = "caller"
  \* no document looked at, none gathered, none being saved, none saved
  /\ next = 1 /\ batch = {} /\ unit = {} /\ rest = {} /\ single = FALSE
  /\ stage = "idle" /\ committed = {} /\ failedAt = 0
  \* the store holds the preexisting aliases and nothing else
  /\ store = [a \in Aliases |-> IF a \in Preexisting THEN "old" ELSE "none"]
  \* the node admits requests and has not drained; no alias is claimed
  /\ admitting = TRUE /\ drained = FALSE /\ claimed = FALSE

\* Whether the Node import holds its admission now: it was admitted as one
\* request, a network task of its own (Node::as_request), and has not ended.
Holding == edge = "Node" /\ HoldImport /\ phase = "run"

\* The edge starts the import: the FFI refuses a transaction handle, a Node
\* import admitted as one request is refused once the node stops admitting,
\* and the driver runs where its edge places it.
Start ==
  \* only once
  /\ phase = "start"
  \* an FFI call on a transaction is refused before anything runs
  /\ IF edge = "Ffi" /\ target = "txn" /\ RequireNode
       THEN phase' = "failed" /\ UNCHANGED driverOn
       \* the Node admits the import as a request: refused after close
       ELSE IF edge = "Node" /\ HoldImport /\ ~admitting
         THEN phase' = "failed" /\ UNCHANGED driverOn
         ELSE /\ phase' = "run"
              \* the FFI call is placed on a worker as every FFI document
              \* call is; on the Node and NodeBuilder edges
              \* import_backup_with wraps the driver in on_pool only in the
              \* bug; the cli adapter runs it on the handler's task
              /\ driverOn' = IF edge = "Ffi" THEN "worker"
                             ELSE IF edge \in {"Node", "HttpNodeBuilder"} /\ ~DriverOnCaller
                               THEN "worker" ELSE "caller"
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, next, batch, unit, rest,
                 single, stage, store, committed, admitting, drained, claimed,
                 failedAt>>

\* Whether alias `a` is held in the store.
Held(a) == store[a] # "none"

\* The aliases of the documents waiting in the batch.
Pending == {file[d] : d \in batch}

\* The driver takes the next document into the batch. Nothing is checked
\* here: the check runs when the batch is saved.
Gather ==
  \* the import is running, nothing is being saved, a document is left
  /\ phase = "run" /\ stage = "idle" /\ next <= Len(file)
  \* a document whose alias the batch holds waits for the batch's save
  /\ ~(FlushOnPendingAlias /\ file[next] \in Pending)
  \* the document joins the batch
  /\ batch' = batch \cup {next}
  /\ next' = next + 1
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, unit,
                 rest, single, stage, store, committed, admitting, drained,
                 claimed, failedAt>>

\* The driver starts saving the batch: at any moment (the file ended, it hit
\* its size bound, or the next document must wait for it).
Flush ==
  \* there is a batch and nothing is being saved
  /\ phase = "run" /\ stage = "idle" /\ batch # {}
  \* the whole batch is the unit; it starts at the check
  /\ unit' = batch /\ batch' = {} /\ stage' = "check"
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, next,
                 rest, single, store, committed, admitting, drained, claimed,
                 failedAt>>

\* What happens when `unit` is refused at its check or its create, which
\* then saved nothing of it. `stop` is the document the import reports.
Refused(stop) ==
  \* a batch of several, refused for the first time: save it one at a time,
  \* from its first document's check
  IF ~single /\ Cardinality(unit) > 1 /\ OneByOneOnRefusal
    THEN /\ single' = TRUE /\ rest' = unit /\ unit' = {Min(unit)}
         /\ stage' = "check"
         /\ UNCHANGED <<phase, failedAt>>
    \* otherwise the import fails there, the rest of the batch unsaved
    ELSE /\ phase' = "failed" /\ failedAt' = stop
         /\ unit' = {} /\ rest' = {} /\ single' = FALSE /\ stage' = "idle"

\* The check: no alias of the unit may be held in the store. A job on the
\* database (alias::check_free).
Check ==
  \* the unit is at its check
  /\ phase = "run" /\ stage = "check"
  /\ IF CheckBeforeCreate /\ \E d \in unit : Held(file[d])
       \* taken: refused, reporting the first document whose alias is held
       THEN Refused(Min({d \in unit : Held(file[d])}))
       \* free (or not checked): on to the create
       ELSE /\ stage' = "save"
            /\ UNCHANGED <<phase, unit, rest, single, failedAt>>
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 store, committed, admitting, drained, claimed>>

\* The record of document `d` as the import wrote it.
Written(d) ==
  \* its number and alias, the path it ran through, its signer and owner,
  \* and whether it was broadcast
  [doc |-> d, alias |-> file[d], host |-> HostOf(edge),
   signer |-> ImportSigner(edge, caller, signs),
   creator |-> ImportCreator(edge, caller, signs),
   broadcast |-> Broadcast]

\* Whether the Node refuses the import's create now. A write that continues
\* the admitted import (Admit::Continuation) is refused only once the drain
\* finished; a write admitted on its own is refused once the node stops
\* admitting.
SaveRefused ==
  \* only the Node edge's admission is modelled
  /\ edge = "Node"
  \* which rule refuses it
  /\ IF HoldImport THEN drained ELSE ~admitting

\* The create: the unit's documents in one request, all or nothing.
Save ==
  \* the unit is at its create
  /\ phase = "run" /\ stage = "save"
  /\ IF SaveRefused
       \* refused: nothing saved
       THEN /\ Refused(Min(unit))
            /\ UNCHANGED committed
       \* created: every document of the unit is saved; on to registration
       ELSE /\ committed' = committed \cup {Written(d) : d \in unit}
            /\ stage' = "register"
            /\ UNCHANGED <<phase, unit, rest, single, failedAt>>
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 store, admitting, drained, claimed>>

\* Whether every alias of `u` may be registered to its document: free, or
\* (without validation) whatever holds it.
Registrable(u) == \A d \in u : ~Held(file[d]) \/ ~AliasInTxn

\* After the unit's registration: the next document of a batch being saved
\* one at a time, or back to gathering.
AfterUnit ==
  IF single /\ rest \ unit # {}
    \* the next document of the refused batch, from its check
    THEN /\ rest' = rest \ unit /\ unit' = {Min(rest \ unit)}
         /\ stage' = "check" /\ UNCHANGED single
    \* the batch is done
    ELSE /\ rest' = {} /\ unit' = {} /\ single' = FALSE /\ stage' = "idle"

\* The import fails at the unit's first document with its documents saved.
FailSaved ==
  \* the import stops; nothing is being saved any more
  /\ phase' = "failed" /\ failedAt' = Min(unit)
  /\ unit' = {} /\ rest' = {} /\ single' = FALSE /\ stage' = "idle"

\* The registration: the unit's aliases in one transaction on the database
\* (alias::register), after its create. Once the drain finished, the
\* database closes and refuses it.
Register ==
  \* the unit is at its registration
  /\ phase = "run" /\ stage = "register"
  /\ IF edge = "Node" /\ drained
       \* the database closed under the import: its documents stay saved,
       \* their aliases unregistered
       THEN FailSaved /\ UNCHANGED store
       \* the validated reads see a taken alias: the documents stay saved,
       \* each one's alias is then registered on its own where it is free,
       \* and the import fails with the alias error
       ELSE IF ~Registrable(unit)
         THEN /\ FailSaved
              /\ store' = IF RegisterEachOnRefusal
                            THEN [a \in Aliases |->
                                    IF \E d \in unit : file[d] = a /\ ~Held(a)
                                      THEN "imported"
                                      ELSE store[a]]
                            ELSE store
         \* else each alias is set to its document (the first of two that
         \* share one keeps it)
         ELSE /\ store' = [a \in Aliases |->
                             IF \E d \in unit : file[d] = a
                               THEN "imported"
                               ELSE store[a]]
              \* a unit that held one alias twice fails on its second
              /\ IF \E d, e \in unit : d # e /\ file[d] = file[e]
                   THEN FailSaved
                   ELSE AfterUnit /\ UNCHANGED <<phase, failedAt>>
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 committed, admitting, drained, claimed>>

\* The import ends once the file is done and nothing waits to be saved.
Finish ==
  \* the import runs, the file is done and nothing waits
  /\ phase = "run" /\ next > Len(file) /\ batch = {} /\ stage = "idle"
  \* it ends well
  /\ phase' = "done"
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 unit, rest, single, stage, store, committed, admitting,
                 drained, claimed, failedAt>>

\* Another writer claims the contested alias, through a validated write of
\* its own: only while nothing holds it.
Claim ==
  \* once, and only while the alias is free
  /\ ~claimed /\ ~Held(Contested)
  \* the writer's document now holds the alias
  /\ store' = [store EXCEPT ![Contested] = "w"]
  \* remember the writer holds it
  /\ claimed' = TRUE
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, next,
                 batch, unit, rest, single, stage, committed, admitting,
                 drained, failedAt>>

\* The Node import's embedded caller stops waiting for it. An import run as
\* a network task of its own (DetachedImport) goes on to its end, so for it
\* this changes nothing and is not a step. One run on its caller's task is
\* dropped where it stands: what it saved stays saved, and nothing after
\* that runs, its registration included.
Abandon ==
  \* only a running Node import on its caller's task
  /\ edge = "Node" /\ phase = "run" /\ ~DetachedImport
  \* the driver is gone; nothing is being saved any more
  /\ phase' = "dropped"
  /\ unit' = {} /\ rest' = {} /\ single' = FALSE /\ stage' = "idle"
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 store, committed, admitting, drained, claimed, failedAt>>

\* The node's shutdown stops admitting requests.
Close ==
  \* once
  /\ admitting
  \* later requests are refused
  /\ admitting' = FALSE
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, next,
                 batch, unit, rest, single, stage, store, committed, drained,
                 claimed, failedAt>>

\* The drain finishes: after the close, once no admitted request still runs.
\* A Node import holding its admission is one of them.
Drain ==
  \* after the close, once
  /\ ~admitting /\ ~drained
  \* nothing admitted still runs
  /\ ~Holding
  \* the drain is over; the database closes
  /\ drained' = TRUE
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, next,
                 batch, unit, rest, single, stage, store, committed,
                 admitting, claimed, failedAt>>

\* Any of the actors takes a step.
Next == Start \/ Gather \/ Flush \/ Check \/ Save \/ Register \/ Finish
        \/ Claim \/ Close \/ Drain \/ Abandon

\* Every run: a start, then any steps.
Spec == Init /\ [][Next]_vars

\* The numbers of the saved documents.
SavedDocs == {r.doc : r \in committed}

\* Promise: every imported document is signed as a create by the same
\* caller on the same node would be.
INV_ImportSigned ==
  \* for each saved document, its signer is a create's
  \A r \in committed : r.signer = CreateSigner(caller, signs)

\* Promise: every imported document is broadcast to the node's replicators.
INV_ImportReplicated ==
  \* for each saved document, it was broadcast
  \A r \in committed : r.broadcast

\* Promise: every import write ran through the request path of the edge it
\* came in by.
INV_HostFromEdge ==
  \* for each saved document, its path is its edge's
  \A r \in committed : r.host = edge

\* Promise: on the Node and HTTP edges the driver runs off the database
\* workers (on its caller, or the Node import's own network task).
INV_DriverOnCaller ==
  \* the FFI call alone is placed on a worker, as every FFI document call is
  edge # "Ffi" => driverOn = "caller"

\* Promise: no two imported documents share an alias, and none takes an
\* alias the store held before the import.
INV_NoDuplicateAlias ==
  \* two different saved documents carry two different aliases
  /\ \A r, q \in committed : r.doc # q.doc => r.alias # q.alias
  \* no saved document carries an alias the store held
  /\ \A r \in committed : r.alias \notin Preexisting

\* Promise: once the other writer holds the contested alias, it keeps it.
INV_AliasNeverStolen ==
  \* the writer's claim is never overwritten
  claimed => store[Contested] = "w"

\* Promise: an FFI import on a transaction handle writes nothing.
INV_FfiTxnRefused ==
  \* refused before any write
  (edge = "Ffi" /\ target = "txn") => committed = {}

\* Promise: every imported document is owned as a create through its edge
\* would be.
INV_CreatorAsCreate ==
  \* for each saved document, its owner is a create's
  \A r \in committed : r.creator = CreateCreator(edge, caller, signs)

\* Promise: once a Node import ended or was dropped, every alias of a
\* document it saved is held: by that document, or by the other writer when it claimed the alias
\* between the check and the registration (which then fails the import).
INV_SavedRegistered ==
  \* an ended or dropped Node import left no saved document's alias free
  (edge = "Node" /\ phase \in {"done", "failed", "dropped"}) =>
    \A r \in committed : Held(r.alias)

\* Promise: the Node's drain never finishes while an import still runs.
INV_DrainWaitsForImport ==
  \* a drained Node runs no import
  (edge = "Node" /\ drained) => phase # "run"

\* Promise: a failed import kept every document before the one it stopped at.
INV_KeepsPrefix ==
  \* each document before the stop was saved
  (phase = "failed" /\ failedAt > 0) => \A d \in 1..(failedAt - 1) : d \in SavedDocs

====
