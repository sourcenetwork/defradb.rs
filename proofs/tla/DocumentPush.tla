---- MODULE DocumentPush ----
\* An explicit selection owns acknowledgement independently of any subscription.
\* One operation captures its owner once. Work admitted before removal can finish;
\* removal forbids another tracked registration and cannot create a new owner.
EXTENDS Naturals, FiniteSets
CONSTANTS AllowSelected, CheckExplicitOwner, CheckSelection, CheckGrant, CheckFence, CheckMarkerOwner, CheckAcknowledgements, CheckRetained
VARIABLES explicit, initiallyPresent, present, owner, grants, prepared, sent,
          acknowledged, refused, markers, stable, result, retained, crossedFence
vars == <<explicit, initiallyPresent, present, owner, grants, prepared, sent,
          acknowledged, refused, markers, stable, result, retained, crossedFence>>
Docs == {1, 2}
Wanted == IF explicit THEN {1} ELSE Docs
Active == owner \in {"selected", "replicator"} /\ result = "pending"
Init == /\ explicit \in BOOLEAN /\ present \in BOOLEAN
        /\ initiallyPresent = present /\ owner = "new"
        /\ grants \in SUBSET Docs
        /\ prepared = {} /\ sent = {} /\ acknowledged = {} /\ refused = {}
        /\ markers = {} /\ retained = {} /\ stable = TRUE /\ result = "pending" /\ crossedFence = FALSE
Choose == /\ owner = "new"
          \* The ownership mutant lets a transient subscription capture an explicit call.
          /\ owner' = IF explicit /\ AllowSelected /\ (CheckExplicitOwner \/ ~present)
                       THEN "selected" ELSE IF present THEN "replicator" ELSE "refused"
          /\ UNCHANGED <<explicit, initiallyPresent, present, grants, prepared, sent,
                         acknowledged, refused, markers, stable, result, retained, crossedFence>>
Remove == /\ Active /\ present /\ present' = FALSE /\ markers' = {} /\ retained' = {}
          /\ UNCHANGED <<explicit, initiallyPresent, owner, grants, prepared, sent,
                         acknowledged, refused, stable, result, crossedFence>>
Prepare(d) == /\ Active /\ d \in Docs \ prepared
              /\ (~CheckSelection \/ d \in Wanted)
              /\ (~CheckFence \/ owner = "selected" \/ present)
              /\ prepared' = prepared \cup {d}
              /\ markers' = IF owner = "replicator" \/ ~CheckMarkerOwner
                             THEN markers \cup {d} ELSE markers
              /\ crossedFence' = (crossedFence \/ (owner = "replicator" /\ ~present))
              /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, sent,
                             acknowledged, refused, stable, result, retained>>
Send(d) == /\ Active /\ d \in prepared \ sent /\ (~CheckGrant \/ d \in grants)
           /\ sent' = sent \cup {d}
           /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, prepared,
                          acknowledged, refused, markers, stable, result, retained, crossedFence>>
Acknowledge(d) == /\ Active /\ d \in sent \ (acknowledged \cup refused)
                  /\ acknowledged' = acknowledged \cup {d}
                  /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, prepared,
                                 sent, refused, markers, stable, result, retained, crossedFence>>
Refuse(d) == /\ Active /\ d \in sent \ (acknowledged \cup refused)
            /\ refused' = refused \cup {d}
            /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, prepared,
                           sent, acknowledged, markers, stable, result, retained, crossedFence>>
ChangeHeads == /\ Active /\ stable /\ stable' = FALSE
               /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, prepared,
                              sent, acknowledged, refused, markers, result, retained, crossedFence>>
Finish == /\ Active /\ sent \subseteq (acknowledged \cup refused)
          /\ LET complete == (Wanted \subseteq acknowledged) /\ stable
                  accepted == complete \/ ~CheckAcknowledgements
              IN /\ result' = IF accepted THEN "acknowledged"
                               ELSE IF owner = "replicator" /\ present /\ markers # {}
                                    THEN "retainedRetry" ELSE "failed"
                 /\ markers' = IF accepted THEN {} ELSE markers
                 /\ retained' = IF accepted /\ present /\ CheckRetained
                                  THEN retained \cup acknowledged ELSE retained
          /\ UNCHANGED <<explicit, initiallyPresent, present, owner, grants, prepared,
                         sent, acknowledged, refused, stable, crossedFence>>
Next == Choose \/ Remove \/ ChangeHeads \/ Finish
        \/ (\E d \in Docs : Prepare(d) \/ Send(d) \/ Acknowledge(d) \/ Refuse(d))
Spec == Init /\ [][Next]_vars
TypeOK == /\ explicit \in BOOLEAN /\ initiallyPresent \in BOOLEAN /\ present \in BOOLEAN
          /\ owner \in {"new", "refused", "selected", "replicator"}
          /\ grants \subseteq Docs /\ prepared \subseteq Docs /\ sent \subseteq Docs
          /\ acknowledged \subseteq sent /\ refused \subseteq sent /\ markers \subseteq Docs
          /\ retained \subseteq acknowledged
          /\ stable \in BOOLEAN /\ crossedFence \in BOOLEAN
          /\ result \in {"pending", "acknowledged", "retainedRetry", "failed"}
INV_SelectedAdmitted == (owner # "new" /\ explicit) => owner = "selected"
INV_SelectionScoped == prepared \subseteq Wanted
INV_GrantRequired == sent \subseteq grants
INV_ReplayFence == ~crossedFence
INV_SelectedHasNoMarker == owner = "selected" => markers = {}
INV_CompletionTruth == result = "acknowledged" => (Wanted \subseteq acknowledged /\ stable)
\* Successful completion exposes accepted heads through the installed peer observation owner.
INV_RetainedHeads == (result = "acknowledged" /\ present) => Wanted \subseteq retained
INV_RetryOwned == result = "retainedRetry" => (owner = "replicator" /\ present /\ markers # {})
====
