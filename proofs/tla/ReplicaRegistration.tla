------------------------- MODULE ReplicaRegistration -------------------------
\* The story, in plain words.
\*
\* A document is created by "c" on some node and replicated to receiving
\* nodes "r1" and "r2". The receivers share one remote ACP provider (one Vera
\* deployment); the provider decides who may change the document. Every
\* replicated change carries its signer, and a receiver merges a change only
\* if the provider says that signer may make it, at the moment it merges.
\*
\* Two cases, chosen at the start:
\*   * c is one of this deployment's own actors ("own"): c's own node owes
\*     the provider c's registration and pays it after its commit. The
\*     receivers wait until the provider holds the document. They never
\*     register it themselves: that would take the document from its creator.
\*   * c belongs to another deployment ("foreign"): c cannot write on this
\*     provider, so nobody would ever register the document here. A receiver
\*     of a collection the embedder listed registers it as itself, and owes,
\*     in the same unit, the grants the embedder asks for (write for c, read
\*     for the deployment's nodes). The unit is written in the transaction
\*     that merges the document's first block (its genesis), so the genesis
\*     never stands without it.
\*
\* While a receiver's unit is owed, the provider has not seen it: asking the
\* provider about c would say "no". So a change by c that arrives in that
\* window is put back to be tried again (a retryable skip), never rejected:
\* a rejection is final for that block on that node. While the receiver's
\* registration itself is owed, nobody's change can be decided (the node's
\* stand-in lets only the receiver in), so every change waits.
\*
\* Two receivers may both register the same foreign document. The provider
\* takes the first; the second is told "already registered". If the owner is
\* one of the deployment's own actors, the second receiver drops its own
\* grants (it cannot give them: it does not own the document) and keeps a
\* note that c's grants come from the owner; until the provider shows c may
\* write, c's changes stay retryable there too. A receiver that first sees
\* the genesis after another receiver registered it keeps the same note while
\* c does not yet write: the owner's unit is owed where it cannot see it.
\*
\* A "node" here is both the node's identity and the provider actor it signs
\* with: the merge gate counts the node as able to read when either can.
\*
\* Another identity "w" may be granted write by the owner and revoked again.
\* Its changes merge only while it holds write; a decision remembered from
\* before a revocation must not be used after it.
\*
\* A deleted document stays registered: deleting a row never removes the
\* object from the provider. A local delete on c's own node does not drop the
\* registration c's node still owes, so the provider does get the document
\* and later replays are not stuck as unregistered.
\*
\* What must never happen:
\*   - the provider's owner is anybody but c (own case) or a receiver
\*     (foreign case) - INV_RegistrantIsReceiver;
\*   - a receiver registers, or owes a registration of, a document whose
\*     creator is one of the deployment's own actors - INV_OwnCreatorNotPreempted;
\*   - a receiver merges a foreign genesis without its registration owed or
\*     the provider holding the document - INV_GenesisWithRegistration;
\*   - c's change is rejected (c's write is never revoked here, so any
\*     rejection of c happened while a unit was still owed)
\*     - INV_CreatorNotRejectedWhileOwed;
\*   - a change is rejected while the provider lets its signer write, on
\*     the word of the receiver's own owed registration
\*     - INV_DeniedOnlyByProvider;
\*   - a change by c merges while c does not hold write - INV_MergeAuthorized;
\*   - a change by w merges while w does not hold write, such as after its
\*     revocation - INV_RevokedWriterDenied;
\*   - a registered document becomes unregistered - INV_RegisteredStays;
\*   - with the provider and every node working, c's genesis, update and
\*     delete do not all end merged on every receiver - LIVE_RowMerges.
\*
\* Rust owners: crates/db/src/merge/replica.rs ReplicaRegistration,
\* ReplicaOwnership; crates/db/src/merge/merge_handler/composite.rs
\* prepare_composite_merge; crates/db/src/merge/merge_handler/
\* composite_persist.rs persist_merged_document; crates/db/src/merge/
\* acp_merge_handler.rs AcpCompositeMergeHook (on_protected_composite,
\* on_protected_update, plan_replica); crates/acp/src/replica.rs
\* ReplicaUnit, record_replica_unit; crates/acp/src/external.rs
\* ExternalDocumentAcp::replica_grant_owed; crates/acp/src/external/
\* registration.rs register_and_clear; crates/db/src/write/autocommit/
\* helpers.rs unregister_deleted_doc.
EXTENDS Naturals, FiniteSets

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

\* The receiving nodes of the deployment.
Receivers == {"r1", "r2"}
\* The replicated changes: c's genesis, update and delete, and two updates
\* by w.
Commits == {"g", "u", "d", "x1", "x2"}
\* Who signed each change.
Signer(k) == IF k \in {"x1", "x2"} THEN "w" ELSE "c"
\* The changes that are not the genesis.
Later == Commits \ {"g"}

VARIABLES
  own,       \* TRUE when c is one of this deployment's own actors
  cReg,      \* c's node's own owed registration: "none", "owed", "done" or "cleared"
  owner,     \* the provider's owner of the document: "none", "c" or a receiver
  writers,   \* the identities the provider lets write, beside the owner
  nodesRead, \* TRUE once the provider lets the deployment's nodes read
  wState,    \* w's write: "none", "granted" or "revoked"
  reg,       \* per receiver: its owed registration, "none", "owed" or "done"
  grants,    \* per receiver: the grants it still owes, of "cw" (c writes) and "nr" (nodes read)
  marker,    \* per receiver: "none", "unit" (its own unit) or "adopted" (the owner's grants)
  memo,      \* per receiver: the signers a remembered decision lets write
  merged,    \* per receiver: the changes merged
  rejected   \* per receiver: the changes rejected for good

vars == <<own, cReg, owner, writers, nodesRead, wState, reg, grants, marker,
          memo, merged, rejected>>

\* At the start nothing is registered, merged or owed by a receiver; c's
\* node owes its own registration in the own case.
Init ==
  /\ own \in BOOLEAN                                \* either case
  /\ cReg = IF own THEN "owed" ELSE "none"          \* c's node owes it when c is own
  /\ owner = "none"                                 \* the provider holds nothing yet
  /\ writers = {}                                   \* nobody writes yet
  /\ nodesRead = FALSE                              \* the nodes read nothing yet
  /\ wState = "none"                                \* w was never granted
  /\ reg = [r \in Receivers |-> "none"]             \* no receiver owes a registration
  /\ grants = [r \in Receivers |-> {}]              \* nor grants
  /\ marker = [r \in Receivers |-> "none"]          \* nor keeps a note
  /\ memo = [r \in Receivers |-> {}]                \* nothing is remembered
  /\ merged = [r \in Receivers |-> {}]              \* nothing is merged
  /\ rejected = [r \in Receivers |-> {}]            \* nothing is rejected

\* Whether the provider lets s write: the owner may, and whoever was granted.
HoldsWrite(s) == s = owner \/ s \in writers

\* Whether receiver r counts the document as registered: the provider holds
\* it, or r itself still owes its registration.
Registered(r) == owner # "none" \/ reg[r] = "owed"

\* Whether receiver r may read the document: it owns it on the provider, the
\* provider lets the deployment's nodes read, or r owes its registration (the
\* owed creator, r, is the only one the node lets read meanwhile).
Readable(r) == owner = r \/ nodesRead \/ reg[r] = "owed"

\* Whether r's own unit (registration and grants) is still owed.
UnitOwed(r) == marker[r] = "unit" /\ (reg[r] = "owed" \/ grants[r] # {})

\* The merge gate's answer for c's change on r: put it back to be tried
\* again. While r's own unit is owed, or while r waits for the owner's grants
\* and the provider does not yet let c write. The planted bugs forget the
\* grants half of the unit, or the gate as a whole.
CreatorRetries(r) ==
  CASE Bug = "rejectWhileOwed" -> FALSE
    [] Bug = "grantsSeparate" -> marker[r] = "unit" /\ reg[r] = "owed"
    [] OTHER -> \/ UnitOwed(r)
                \/ marker[r] = "adopted" /\ ~HoldsWrite("c")

\* The gate's answer for anybody's change on r: put it back while r's own
\* registration is owed. The provider has not seen r's unit, so the node
\* cannot ask it about the document; its "no" would not be the provider's.
\* The planted bug lets the node's own placeholder answer stand.
Retries(r, s) ==
  \/ s = "c" /\ CreatorRetries(r)
  \/ Bug # "ownedDenies" /\ marker[r] = "unit" /\ reg[r] = "owed"

\* What the provider (through the node's gate) answers for signer s on r:
\* while r owes its registration only r is let in, which no change's signer
\* is; otherwise whether s holds write. The planted bug answers from what an
\* earlier answer let in.
Allowed(r, s) ==
  IF reg[r] = "owed" THEN FALSE
  ELSE IF Bug = "staleDecision" /\ s \in memo[r] THEN TRUE
  ELSE HoldsWrite(s)

\* c's node pays its own registration: the provider makes c the owner, and
\* c's own grants let the deployment's nodes read.
CreatorRegisters ==
  /\ cReg = "owed"
  /\ owner = "none"
  /\ owner' = "c"
  /\ nodesRead' = TRUE
  /\ cReg' = "done"
  /\ UNCHANGED <<own, writers, wState, reg, grants, marker, memo, merged, rejected>>

\* c deletes the document on its own node before its registration was paid.
\* The registration stays owed; the planted bug drops it.
CreatorDeletesLocally ==
  /\ cReg = "owed"
  /\ Bug = "deleteClears"
  /\ cReg' = "cleared"
  /\ UNCHANGED <<own, owner, writers, nodesRead, wState, reg, grants, marker,
                 memo, merged, rejected>>

\* Whether r, merging a foreign genesis the provider already holds for
\* another receiver, must note that c's grants come from that owner: c does
\* not write yet, so the owner's unit is still owed somewhere r cannot see.
Adopts(r) ==
  /\ ~own
  /\ owner \in Receivers \ {r}
  /\ ~HoldsWrite("c")
  /\ marker[r] = "none"
  /\ Bug # "noAdoptOnArrival"

\* Receiver r merges c's genesis. If the provider holds the document (or r
\* still owes its registration), the genesis merges once r can read it, with
\* the note above when it applies. If nobody holds it and c is foreign, r
\* merges it and records its unit in the same step. The planted bugs register
\* for an own creator too, or merge the genesis and leave the registration to
\* after the commit.
MergeGenesis(r) ==
  /\ "g" \notin merged[r]
  /\ \/ /\ Registered(r)
        /\ Readable(r)
        /\ marker' = [marker EXCEPT ![r] = IF Adopts(r) THEN "adopted" ELSE @]
        /\ UNCHANGED <<reg, grants>>
     \/ /\ ~Registered(r)
        /\ (~own \/ Bug = "registerOwn")
        /\ IF Bug = "registerAfterMerge"
             THEN UNCHANGED <<reg, grants, marker>>
             ELSE /\ reg' = [reg EXCEPT ![r] = "owed"]
                  /\ grants' = [grants EXCEPT ![r] = {"cw", "nr"}]
                  /\ marker' = [marker EXCEPT ![r] = "unit"]
  /\ merged' = [merged EXCEPT ![r] = @ \cup {"g"}]
  /\ UNCHANGED <<own, cReg, owner, writers, nodesRead, wState, memo, rejected>>

\* Receiver r decides a later change k, after its genesis merged. The
\* document must count as registered and readable; else k waits. A change
\* waits while the gate says so. Otherwise the provider decides: allowed
\* merges, refused is rejected. Merging the delete never unregisters; the
\* planted bug archives the object.
Decide(r, k) ==
  /\ k \in Later
  /\ "g" \in merged[r]
  /\ k \notin merged[r] \cup rejected[r]
  /\ Registered(r)
  /\ Readable(r)
  /\ ~Retries(r, Signer(k))
  /\ IF Allowed(r, Signer(k))
       THEN /\ merged' = [merged EXCEPT ![r] = @ \cup {k}]
            /\ memo' = [memo EXCEPT ![r] = @ \cup {Signer(k)}]
            /\ rejected' = rejected
            /\ IF k = "d" /\ Bug = "archiveOnDelete"
                 THEN /\ owner' = "none"
                      /\ writers' = {}
                 ELSE UNCHANGED <<owner, writers>>
       ELSE /\ rejected' = [rejected EXCEPT ![r] = @ \cup {k}]
            /\ UNCHANGED <<merged, memo, owner, writers>>
  /\ UNCHANGED <<own, cReg, nodesRead, wState, reg, grants, marker>>

\* r sends its owed registration. The provider takes it when it holds
\* nothing; otherwise r is told it is already registered. Then r's grants
\* cannot be given by r: they are discarded, and when the owner is one of the
\* deployment's own actors (another receiver) r notes that c's grants come
\* from the owner.
SettleRegistration(r) ==
  /\ reg[r] = "owed"
  /\ reg' = [reg EXCEPT ![r] = "done"]
  /\ IF owner = "none"
       THEN /\ owner' = r
            /\ UNCHANGED <<grants, marker>>
       ELSE /\ grants' = [grants EXCEPT ![r] = {}]
            /\ marker' = [marker EXCEPT ![r] =
                            IF owner \in Receivers /\ Bug # "adoptDrops"
                              THEN "adopted" ELSE "none"]
            /\ owner' = owner
  /\ UNCHANGED <<own, cReg, writers, nodesRead, wState, memo, merged, rejected>>

\* r sends one of its owed grants, after its registration and as the owner,
\* in either order: c may write ("cw"), or the deployment's nodes may read
\* ("nr"). Each is its own change on the provider. Once none is left the
\* unit is paid and the note goes.
SettleGrant(r, gr) ==
  /\ reg[r] = "done"
  /\ gr \in grants[r]
  /\ owner = r
  /\ writers' = IF gr = "cw" THEN writers \cup {"c"} ELSE writers
  /\ nodesRead' = (nodesRead \/ gr = "nr")
  /\ grants' = [grants EXCEPT ![r] = @ \ {gr}]
  /\ marker' = [marker EXCEPT ![r] = IF grants'[r] = {} THEN "none" ELSE @]
  /\ UNCHANGED <<own, cReg, owner, wState, reg, memo, merged, rejected>>

\* r's note that c's grants come from the owner goes once the provider lets
\* c write.
ClearAdopted(r) ==
  /\ marker[r] = "adopted"
  /\ HoldsWrite("c")
  /\ marker' = [marker EXCEPT ![r] = "none"]
  /\ UNCHANGED <<own, cReg, owner, writers, nodesRead, wState, reg, grants,
                 memo, merged, rejected>>

\* The owner grants w write, once.
GrantW ==
  /\ owner # "none"
  /\ wState = "none"
  /\ writers' = writers \cup {"w"}
  /\ wState' = "granted"
  /\ UNCHANGED <<own, cReg, owner, nodesRead, reg, grants, marker, memo,
                 merged, rejected>>

\* The owner revokes w's write, once.
RevokeW ==
  /\ wState = "granted"
  /\ writers' = writers \ {"w"}
  /\ wState' = "revoked"
  /\ UNCHANGED <<own, cReg, owner, nodesRead, reg, grants, marker, memo,
                 merged, rejected>>

\* One step of anyone.
Next ==
  \/ CreatorRegisters
  \/ CreatorDeletesLocally
  \/ \E r \in Receivers :
       \/ MergeGenesis(r)
       \/ SettleRegistration(r)
       \/ \E gr \in {"cw", "nr"} : SettleGrant(r, gr)
       \/ ClearAdopted(r)
       \/ \E k \in Later : Decide(r, k)
  \/ GrantW
  \/ RevokeW

\* Every node and the provider keep working: each step that stays possible
\* is taken. w's grant and revocation are the owner's choice, not owed.
Fairness ==
  /\ WF_vars(CreatorRegisters)
  /\ \A r \in Receivers :
       /\ WF_vars(MergeGenesis(r))
       /\ WF_vars(SettleRegistration(r))
       /\ \A gr \in {"cw", "nr"} : WF_vars(SettleGrant(r, gr))
       /\ WF_vars(ClearAdopted(r))
       /\ \A k \in {"u", "d"} : WF_vars(Decide(r, k))

Spec == Init /\ [][Next]_vars
LiveSpec == Spec /\ Fairness

\* The variables hold values of the expected kinds.
TypeOK ==
  /\ own \in BOOLEAN
  /\ cReg \in {"none", "owed", "done", "cleared"}
  /\ owner \in {"none", "c"} \cup Receivers
  /\ writers \subseteq {"c", "w"}
  /\ nodesRead \in BOOLEAN
  /\ wState \in {"none", "granted", "revoked"}
  /\ reg \in [Receivers -> {"none", "owed", "done"}]
  /\ grants \in [Receivers -> SUBSET {"cw", "nr"}]
  /\ marker \in [Receivers -> {"none", "unit", "adopted"}]
  /\ memo \in [Receivers -> SUBSET {"c", "w"}]
  /\ merged \in [Receivers -> SUBSET Commits]
  /\ rejected \in [Receivers -> SUBSET Later]

\* Only c (own case) or a receiver (foreign case) ever owns the document.
INV_RegistrantIsReceiver ==
  owner \in {"none"} \cup (IF own THEN {"c"} ELSE Receivers)

\* No receiver registers, or owes a registration of, an own creator's document.
INV_OwnCreatorNotPreempted ==
  own => \A r \in Receivers : reg[r] = "none" /\ marker[r] = "none"

\* A merged genesis always has its registration owed by that receiver or
\* held by the provider.
INV_GenesisWithRegistration ==
  \A r \in Receivers : "g" \in merged[r] => Registered(r) \/ reg[r] = "done"

\* c's write is never revoked here, so a rejected change by c was rejected
\* while a unit was owed: that must never happen.
INV_CreatorNotRejectedWhileOwed ==
  \A r \in Receivers : \A k \in rejected[r] : Signer(k) # "c"

\* A change is rejected only when the provider does not let its signer
\* write: the node's own placeholder never rejects anybody.
INV_DeniedOnlyByProvider ==
  [][\A r \in Receivers : \A k \in Later :
       (k \notin rejected[r] /\ k \in rejected'[r]) => ~HoldsWrite(Signer(k))]_vars

\* A change by c merges only while c holds write.
INV_MergeAuthorized ==
  [][\A r \in Receivers : \A k \in Later :
       (Signer(k) = "c" /\ k \notin merged[r] /\ k \in merged'[r])
         => HoldsWrite("c")]_vars

\* A change by w merges only while w holds write: after its revocation, never.
INV_RevokedWriterDenied ==
  [][\A r \in Receivers : \A k \in Later :
       (Signer(k) = "w" /\ k \notin merged[r] /\ k \in merged'[r])
         => HoldsWrite("w")]_vars

\* Once the provider holds the document, it keeps holding it.
INV_RegisteredStays == [][owner # "none" => owner' # "none"]_vars

\* With everyone working, c's genesis, update and delete end merged on every
\* receiver.
LIVE_RowMerges == <>(\A r \in Receivers : {"g", "u", "d"} \subseteq merged[r])
==============================================================================
