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
\*     documents into a batch it saves in one request (all or nothing);
\*   * another writer, which may claim an alias at any moment;
\*   * the node's shutdown, which stops admitting requests.
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
\*   8. a Node write committed after the node stopped admitting - INV_NoWriteAfterClose.
\*
\* Each guard has a RED configuration that removes it:
\*   * OwnMutator = FALSE: writes go through the edge's host (Red_OwnMutator);
\*   * KeepSigningChoice: the host keeps the edge's signer (Red_NoSigningChoice);
\*   * AliasInTxn: registration reads validated (Red_AliasOutsideTxn);
\*   * CheckBeforeCreate: the alias check comes first (Red_AliasCheckAfterCreate);
\*   * FfiOwnPath: FFI writes through its DocumentCall (Red_FfiViaNode);
\*   * KeepCaller: the host acts as the caller (Red_CallerIgnored);
\*   * AdmitEachWrite: the Node host admits each write (Red_ImportAdmittedOnce);
\*   * AnonAsCreate: an anonymous import acts as an anonymous create (Red_ActorNoOne);
\*   * AdapterTakesHost: the cli adapter forwards the handler's host (Red_AdapterOwnHost);
\*   * DriverOnCaller: import_backup_with runs the driver where it is called
\*     (Red_PrepareOnPool);
\*   * FlushOnPendingAlias: a batch flushes before a document whose alias it
\*     holds (Red_BatchSkipsPendingAlias);
\*   * RequireNode: the FFI refuses a transaction handle (Red_FfiTxnAccepted).
\*
\* What is abstracted: a document is its alias; relation fields are left out
\* (a batch also flushes before a document whose relation points into it, so
\* the target's new ID is known, which changes no alias rule); one import
\* per run; the edge, the caller, whether the node signs and the FFI target
\* are picked at the start.
\*
\* Rust: crates/database/db/src/backup/import.rs import_database_as,
\* crates/database/db/src/backup/host.rs ImportHost, RunnerHost;
\* crates/database/db/src/backup/batch.rs Batch::must_save_before;
\* crates/database/db/src/backup/alias.rs register, check_free;
\* crates/entry-points/defra-node/src/ops/backup.rs
\* Node::import_backup_as, NodeOps::import_backup_with, NodeHost;
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
  CheckBeforeCreate,   \* TRUE: a document's alias is checked before it is created
  FfiOwnPath,          \* TRUE: the FFI writes through its own DocumentCall
  KeepCaller,          \* TRUE: the host acts as the caller who imports
  AdmitEachWrite,      \* TRUE: the Node host admits every write on its own
  AnonAsCreate,        \* TRUE: an anonymous import acts as an anonymous create does
  AdapterTakesHost,    \* TRUE: the cli adapter writes through the handler's host
  DriverOnCaller,      \* TRUE: import_backup_with runs the driver on its caller
  FlushOnPendingAlias, \* TRUE: a batch flushes before a document whose alias it holds
  RequireNode          \* TRUE: the FFI import refuses a transaction handle

\* The backup files a run may import, each a sequence of aliases: one alias
\* twice, one alias the other writer may claim, one alias the store holds.
Files == {<<"a", "a">>, <<"a", "c">>, <<"p", "b">>}
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

VARIABLES
  edge,       \* the edge the import came through
  caller,     \* who imports
  signs,      \* TRUE when the node signs with its identity
  target,     \* the FFI call's target: "node" or "txn"
  file,       \* the file being imported
  phase,      \* "start", "run", "done" or "failed"
  driverOn,   \* where the driver runs: "caller" or "worker"
  next,       \* the next document of the file to look at
  batch,      \* the documents gathered and not yet saved
  pending,    \* the aliases of the documents in the batch
  store,      \* each alias's owner: "imported", "w" (the other writer), "old" or "none"
  committed,  \* the saved documents, each a record of how it was written
  admitting,  \* TRUE while the node admits requests
  claimed     \* TRUE once the other writer holds the contested alias

\* Every variable, for stuttering steps.
vars == <<edge, caller, signs, target, file, phase, driverOn, next, batch,
          pending, store, committed, admitting, claimed>>

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
  \* the phase and the driver's place are one of theirs
  /\ phase \in {"start", "run", "done", "failed"}
  /\ driverOn \in {"caller", "worker"}
  \* every alias has an owner or none
  /\ store \in [Aliases -> {"imported", "w", "old", "none"}]
  \* the batch and the next document stay within the file
  /\ batch \subseteq Docs(file) /\ next \in 1..(Len(file) + 1)

\* The run's start.
Init ==
  \* any edge, caller, signing setting, FFI target and file
  /\ edge \in Edges /\ caller \in Callers /\ signs \in BOOLEAN
  /\ target \in {"node", "txn"} /\ file \in Files
  \* the import has not started
  /\ phase = "start" /\ driverOn = "caller"
  \* no document looked at, none gathered, none saved
  /\ next = 1 /\ batch = {} /\ pending = {} /\ committed = {}
  \* the store holds the preexisting aliases and nothing else
  /\ store = [a \in Aliases |-> IF a \in Preexisting THEN "old" ELSE "none"]
  \* the node admits requests; the other writer has not claimed its alias
  /\ admitting = TRUE /\ claimed = FALSE

\* The edge starts the import: the FFI refuses a transaction handle, and
\* the driver runs where its edge places it.
Start ==
  \* only once
  /\ phase = "start"
  \* an FFI call on a transaction is refused before anything runs
  /\ IF edge = "Ffi" /\ target = "txn" /\ RequireNode
       THEN phase' = "failed" /\ UNCHANGED driverOn
       \* a Node import admitted once as a whole is refused after close
       ELSE IF edge = "Node" /\ ~AdmitEachWrite /\ ~admitting
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
  /\ UNCHANGED <<edge, caller, signs, target, file, next, batch, pending,
                 store, committed, admitting, claimed>>

\* Whether alias `a` is held in the store.
Held(a) == store[a] # "none"

\* The driver takes the next document into the batch, after checking its
\* alias against the store when the check comes first.
Gather ==
  \* the import is running and the file has a document left
  /\ phase = "run" /\ next <= Len(file)
  \* a document whose alias the batch holds waits for a flush first
  /\ ~(FlushOnPendingAlias /\ file[next] \in pending)
  \* the alias is checked before the document joins the batch
  /\ IF CheckBeforeCreate /\ Held(file[next])
       \* taken: the import fails with the alias error, nothing new saved
       THEN /\ phase' = "failed"
            /\ UNCHANGED <<next, batch, pending>>
       \* free: the document joins the batch
       ELSE /\ batch' = batch \cup {next}
            /\ pending' = pending \cup {file[next]}
            /\ next' = next + 1
            /\ UNCHANGED phase
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, store,
                 committed, admitting, claimed>>

\* The record of document `d` as the import wrote it.
Written(d) ==
  \* its number and alias, the path it ran through, its signer and owner,
  \* whether it was broadcast and whether the node admitted it
  [doc |-> d, alias |-> file[d], host |-> HostOf(edge),
   signer |-> ImportSigner(edge, caller, signs),
   creator |-> ImportCreator(edge, caller, signs),
   broadcast |-> Broadcast, admitted |-> admitting]

\* Whether every alias of batch `b` may be registered to its document: free,
\* or (without validation) whatever holds it.
Registrable(b) == \A d \in b : ~Held(file[d]) \/ ~AliasInTxn

\* The batch is saved in one request, then its aliases are registered in
\* one transaction. Saving is all or nothing; a refused write saves none.
Flush ==
  \* there is a batch; it may be saved at any moment (the file ended, it hit
  \* its size bound, or the next document must wait for it)
  /\ phase = "run" /\ batch # {}
  /\ IF edge = "Node" /\ AdmitEachWrite /\ ~admitting
       \* the Node host refuses the write after close: nothing saved
       THEN /\ phase' = "failed"
            /\ UNCHANGED <<committed, store>>
       \* an alias checked only after the create fails with the documents saved
       ELSE IF ~CheckBeforeCreate /\ \E d \in batch : store[file[d]] = "old"
         THEN /\ committed' = committed \cup {Written(d) : d \in batch}
              /\ phase' = "failed"
              /\ UNCHANGED store
       \* the registration's validated reads see a taken alias: the
       \* documents stay saved and the import fails with the alias error
       ELSE IF ~Registrable(batch)
         THEN /\ committed' = committed \cup {Written(d) : d \in batch}
              /\ phase' = "failed"
              /\ UNCHANGED store
         \* else the batch is saved and each alias set to its document (the
         \* first of two that share one keeps it)
         ELSE /\ committed' = committed \cup {Written(d) : d \in batch}
              /\ store' = [a \in Aliases |->
                             IF \E d \in batch : file[d] = a
                               THEN "imported"
                               ELSE store[a]]
              \* a batch that held one alias twice fails on its second
              /\ phase' = IF \E d, e \in batch : d # e /\ file[d] = file[e]
                            THEN "failed" ELSE phase
  \* the batch is empty again
  /\ batch' = {} /\ pending' = {}
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, admitting,
                 claimed>>

\* The import ends once the file is done and nothing waits to be saved.
Finish ==
  \* the import runs, the file is done and nothing waits
  /\ phase = "run" /\ next > Len(file) /\ batch = {}
  \* it ends well
  /\ phase' = "done"
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, driverOn, next, batch,
                 pending, store, committed, admitting, claimed>>

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
                 batch, pending, committed, admitting>>

\* The node's shutdown stops admitting requests.
Close ==
  \* once
  /\ admitting
  \* later requests are refused
  /\ admitting' = FALSE
  \* nothing else moves
  /\ UNCHANGED <<edge, caller, signs, target, file, phase, driverOn, next,
                 batch, pending, store, committed, claimed>>

\* Any of the actors takes a step.
Next == Start \/ Gather \/ Flush \/ Finish \/ Claim \/ Close

\* Every run: a start, then any steps.
Spec == Init /\ [][Next]_vars

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

\* Promise: on the Node and HTTP edges the driver runs on its caller, never
\* on a database worker.
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

\* Promise: no Node import write commits after the node stopped admitting.
INV_NoWriteAfterClose ==
  \* every saved Node document was admitted
  edge = "Node" => \A r \in committed : r.admitted

====
