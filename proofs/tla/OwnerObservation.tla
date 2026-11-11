-------------------------- MODULE OwnerObservation --------------------------
\* A row and its owner must belong to one authorized transaction snapshot.
\* The Rust owner is QueryRunner::observe_owners.
EXTENDS Naturals
\* A RED configuration skips one gate, or reads an owner outside the snapshot.
CONSTANT Fault
\* The request chooses one relevant authorization/membership condition.
Scenarios == {"allowed", "actor", "node", "app", "document", "member", "registered", "bound"}
\* The live owner may change after the reader's transaction begins.
VARIABLES scenario, phase, liveOwner, snapshotOwner, liveRegistered, snapshotRegistered, liveReadable, snapshotReadable, returned, observedOwner, visible
\* One tuple names every variable for stuttering.
vars == <<scenario, phase, liveOwner, snapshotOwner, liveRegistered, snapshotRegistered, liveReadable, snapshotReadable, returned, observedOwner, visible>>
\* Begin from a captured owner and no released result.
Init == /\ scenario \in Scenarios
        /\ phase = "ready"
        /\ liveOwner = 0
        /\ snapshotOwner = 0
        /\ snapshotRegistered = (scenario # "registered")
        /\ liveRegistered = snapshotRegistered
        /\ snapshotReadable = (scenario \in {"allowed", "registered"})
        /\ liveReadable = snapshotReadable
        /\ returned = FALSE
        /\ observedOwner = 2
        /\ visible = FALSE
\* Another transaction changes ownership without changing our snapshot.
ChangeOwner == /\ liveOwner = 0
               /\ liveOwner' = 1
               /\ UNCHANGED <<scenario, phase, snapshotOwner, liveRegistered, snapshotRegistered, liveReadable, snapshotReadable, returned, observedOwner, visible>>
\* Outside registration commits must not alter the captured view.
ChangeRegistration == /\ liveRegistered' = ~liveRegistered
                      /\ UNCHANGED <<scenario, phase, liveOwner, snapshotOwner, snapshotRegistered, liveReadable, snapshotReadable, returned, observedOwner, visible>>
\* A concurrent grant, revocation or deletion changes live readability only.
ChangeReadability == /\ scenario \notin {"actor", "bound"}
                     /\ liveReadable' = ~liveReadable
                     /\ UNCHANGED <<scenario, phase, liveOwner, snapshotOwner, liveRegistered, snapshotRegistered, snapshotReadable, returned, observedOwner, visible>>
\* The ordinary read path uses authorization and membership from its snapshot.
Readable == snapshotReadable \/ scenario = Fault
\* Registration is read from the same view as the document and owner.
Registration == IF Fault = "registered" THEN TRUE
                ELSE IF Fault = "registration_snapshot" THEN liveRegistered
                ELSE snapshotRegistered
Eligible == Readable /\ Registration
\* The GREEN read uses the captured view; the snapshot fault uses live ACP.
Read == /\ phase = "ready"
        /\ phase' = "done"
        /\ returned' = Eligible
        \* Public reads remain visible even when ownership is not registered.
        /\ visible' = IF Fault = "visibility_registered" THEN Eligible
                       ELSE IF Fault = "visibility_snapshot" THEN liveReadable
                       ELSE Readable
        /\ observedOwner' = IF Eligible THEN
                (IF Fault = "snapshot" THEN liveOwner ELSE snapshotOwner) ELSE 2
        /\ UNCHANGED <<scenario, liveOwner, snapshotOwner, liveRegistered, snapshotRegistered, liveReadable, snapshotReadable>>
\* Explore the outside commit before or after the metadata read.
Next == ChangeOwner \/ ChangeRegistration \/ ChangeReadability \/ Read
\* The operation may finish or stutter; it creates no durable writes.
Spec == Init /\ [][Next]_vars
\* An inaccessible or unbound document never releases an owner.
NoUnauthorizedOwner == returned => scenario = "allowed"
\* Every released owner belongs to the same view as the document row.
SameSnapshot == returned => observedOwner = snapshotOwner
\* A concurrent registration or removal cannot add or hide a captured owner.
SameRegistrationSnapshot == (phase = "done" /\ Readable) => (returned = snapshotRegistered)
\* Read denial never exposes whether a hidden or missing document exists.
NoUnauthorizedVisibility == visible => scenario \in {"allowed", "registered"}
\* Unknown ownership must not be reported as failed read authorization.
VisibilityMatchesRead == phase = "done" => visible = Readable

\* A unified native observation uses the same owner but selects one explicit mode.
\* A gateway authenticates the actor; the transaction principal owns the handle.
\* Named access does not imply delegated read visibility, or vice versa.
UnifiedCases == {"read_allowed", "read_actor", "read_bound", "read_context",
                 "read_principal", "read_nacbound", "read_app", "read_document", "read_member",
                 "named_allowed", "named_actor", "named_bound", "named_context",
                 "named_member", "named_registered", "named_active", "named_supported", "named_node", "named_permission", "named_unsupported_and_node_denied"}
\* A guard fault is the one gate removed by a RED configuration.
UGuard(ok, name) == ok \/ Fault = name
\* These facts represent separately owned checks in the transaction snapshot.
UActor == scenario \notin {"read_actor", "named_actor"}
UBound == scenario \notin {"read_bound", "named_bound"}
UContext == scenario \notin {"read_context", "named_context"}
UNacBound == scenario # "read_nacbound"
UPrincipal == scenario # "read_principal"
UApp == scenario # "read_app"
URead == scenario # "read_document"
UMember == scenario \notin {"read_member", "named_member"}
URegistered == scenario # "named_registered"
UActive == scenario # "named_active"
USupported == scenario \notin {"named_supported", "named_unsupported_and_node_denied"}
UNode == scenario \notin {"named_node", "named_unsupported_and_node_denied"}
UNamed == scenario # "named_permission"
UIsRead == scenario \in {"read_allowed", "read_actor", "read_bound", "read_context",
                         "read_principal", "read_nacbound", "read_app", "read_document", "read_member"}
\* Results are 0 allowed, 1 denied, 2 unavailable, 3 invalid transaction.
\* Delegated hidden or missing rows are unavailable, never owner metadata.
UDecision ==
    IF ~UGuard(UActor, "actor") \/ ~UGuard(UContext, "context")
    THEN 3
    ELSE IF UIsRead THEN
        IF ~UGuard(UBound, "bound") \/ ~UGuard(UNacBound, "nacbound") \/ ~UGuard(UPrincipal, "principal") THEN 3
        ELSE IF UGuard(UApp, "app") /\ UGuard(URead, "read") /\ UGuard(UMember, "member") THEN 0
        ELSE 2
    ELSE IF ~UGuard(UBound, "bound") \/ ~UGuard(UMember, "member") \/ ~UGuard(URegistered, "registered") \/ ~UGuard(USupported, "supported") \/ ~UGuard(UActive, "active") THEN 2
    ELSE IF UGuard(UNode, "node") /\ UGuard(UNamed, "permission") THEN 0
    ELSE 1
\* The initial bit says the live authorization still matches the snapshot.
UInit == /\ scenario \in UnifiedCases
         /\ phase = "ready"
         /\ liveOwner = 0 /\ snapshotOwner = 0
         /\ liveRegistered = TRUE /\ snapshotRegistered = TRUE
         /\ liveReadable = TRUE /\ snapshotReadable = TRUE
         /\ returned = FALSE /\ observedOwner = 3 /\ visible = FALSE
\* Another transaction changes one relevant live fact; a second change can restore it.
UChange == /\ ~returned
           /\ liveReadable' = ~liveReadable
           /\ UNCHANGED <<scenario, phase, liveOwner, snapshotOwner, liveRegistered,
                           snapshotRegistered, snapshotReadable, returned, observedOwner, visible>>
\* The observation only uses the captured row and authorization snapshot.
UObserve == /\ phase = "ready"
            /\ phase' = "done"
            /\ observedOwner' = UDecision
            /\ visible' = (UDecision = 0)
            /\ UNCHANGED <<scenario, liveOwner, snapshotOwner, liveRegistered,
                            snapshotRegistered, liveReadable, snapshotReadable, returned>>
\* An allowed observation may guard a write only while the facts still match.
UCommit == /\ phase = "done" /\ observedOwner = 0 /\ ~returned
           /\ (liveReadable = snapshotReadable \/ Fault = "atomic")
           /\ returned' = TRUE
           /\ UNCHANGED <<scenario, phase, liveOwner, snapshotOwner, liveRegistered,
                           snapshotRegistered, liveReadable, snapshotReadable, observedOwner, visible>>
\* Interleave an observation, an outside revoke or restore, and the guarded commit.
USpec == UInit /\ [][UChange \/ UObserve \/ UCommit]_vars
\* An allowed observation satisfies the exact gates of its selected mode.
USound == (phase = "done" /\ observedOwner = 0) =>
          (UActor /\ UBound /\ UContext /\ UMember /\
           IF UIsRead THEN (UNacBound /\ UPrincipal /\ UApp /\ URead)
           ELSE (URegistered /\ USupported /\ UActive /\ UNode /\ UNamed))
\* A missing, unregistered or inactive named request is not an active denial.
UUnavailableNotDenied == (phase = "done" /\ ~UIsRead /\ (~UBound \/ ~UMember \/ ~URegistered \/ ~USupported \/ ~UActive)) =>
                         observedOwner # 1
\* A guarded write cannot commit while a previously read fact is revoked.
UCommitMatchesSnapshot == returned => liveReadable = snapshotReadable
\* The public observation only contains a decision, never an owner value.
UDecisionOnly == observedOwner \in {0, 1, 2, 3}
=============================================================================
