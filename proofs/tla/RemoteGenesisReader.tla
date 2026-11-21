------------------------- MODULE RemoteGenesisReader -------------------------
\* The story. Two DefraDB nodes of one home, a and b, and a third node c, all
\* decide document access on one remote Vera deployment. The creator writes a
\* document on node a, in a collection the host opted into genesis read
\* access: either everyone may read it (public), or only the node its
\* immutable address field names, here b. A remote provider cannot join a's
\* transaction, so the create records what a owes the provider: the
\* document's registration and, in the same transaction, the reader its
\* genesis asks for. After the commit a sends the registration, and then the
\* grant, which waits behind the registration. The creator may later write
\* again, and may revoke the reader. Each receiver learns the provider's
\* state only through its light client, which lags behind the provider. A
\* receiver merges a frame only when what its light client last saw says
\* the document is registered and the receiver may read it.
\*
\* What must never happen: a node merges a document it was never granted; a
\* node merges on a grant the provider does not hold yet; a node merges more
\* of the document after it has seen the reader revoked; a node merges a
\* document whose registration its provider does not hold; a create its
\* registrations allow is refused for the grants it owes; an owed grant takes
\* an ordinary change's place in its actor's or the node's admission bound,
\* or settling it takes from a count it never added to. And what must
\* happen: unless the reader is revoked, the addressed node ends up merging
\* the genesis.
\*
\* The code: stage_genesis_reader in
\* crates/database/db/src/write/genesis_reader.rs, staging through
\* TransactionOutbox::stage_with_registration
\* (crates/engine/acp/src/outbox/staging.rs) and called by
\* register_created_doc (crates/database/db/src/write/autocommit/helpers.rs)
\* in the create's transaction; the settlement of an owed change behind its
\* object's registration in crates/engine/acp/src/external/settle.rs; the
\* receiver's gate AcpCompositeMergeHook::on_protected_composite in
\* crates/database/db/src/merge/acp_merge_handler.rs.
\* Only the standard operators on numbers and finite sets are used.
EXTENDS Integers, FiniteSets

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
CONSTANT Bug

\* The receiving nodes: b, the addressed one, and c, which nobody addresses.
Receivers == {"b", "c"}

\* The frames of the document: 1 is its genesis, 2 a later write by the
\* creator.
Frames == {1, 2}

\* access: what the host asked for the collection, "addressed" or "public".
\* created: a has committed the create.
\* owedReg, owedGrant: the registration and the grant a still owes the
\*   provider, recorded durably in its outbox.
\* postCommit: in the "separate" bug, the grant still to be staged by a step
\*   after the commit, which a crash loses.
\* crashed: a crashed once (bounded to keep the model small).
\* reg, grant: the provider holds the registration, the reader grant.
\* refused: the provider refused the grant for good.
\* revoked: the creator revoked the reader on the provider.
\* everGranted: the provider has held the grant at some point (a ghost).
\* seenReg, seenGrant, seenRevoked: what each receiver's light client last saw.
\* written: the frames a has written.
\* delivered, merged: the frames each receiver holds, and has merged.
\* early: a receiver merged while the provider had never held the grant.
\* afterRevoked: a receiver merged after its light client saw the revocation.
\* overBound: the create's transaction makes more documents than it may stage
\*   ordinary owed changes, or than their actor may hold unsettled.
\* createRefused: a create its registrations allow was refused for its grants.
\* ordinaryOwed: the creator owes an ordinary change of its own (a revocation).
\* ordinaryRefused: that ordinary change was refused at admission.
\* count: the creator's admission count of unsettled changes, against Limit.
VARIABLES ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit, crashed,
          reg, grant, refused, revoked, everGranted,
          seenReg, seenGrant, seenRevoked,
          written, delivered, merged, early, afterRevoked

\* Every variable, for stuttering steps and fairness.
vars == <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit, crashed,
          reg, grant, refused, revoked, everGranted,
          seenReg, seenGrant, seenRevoked,
          written, delivered, merged, early, afterRevoked>>

\* Either kind of opted-in collection; nothing written, owed or held yet.
Init == /\ access \in {"addressed", "public"}
        /\ overBound \in BOOLEAN /\ createRefused = FALSE
        /\ ordinaryOwed = FALSE /\ ordinaryRefused = FALSE /\ count = 0
        /\ created = FALSE /\ owedReg = FALSE /\ owedGrant = FALSE
        /\ postCommit = FALSE /\ crashed = FALSE
        /\ reg = FALSE /\ grant = FALSE /\ refused = FALSE
        /\ revoked = FALSE /\ everGranted = FALSE
        /\ seenReg = [n \in Receivers |-> FALSE]
        /\ seenGrant = [n \in Receivers |-> FALSE]
        /\ seenRevoked = [n \in Receivers |-> FALSE]
        /\ written = {} /\ delivered = [n \in Receivers |-> {}]
        /\ merged = [n \in Receivers |-> {}]
        /\ early = FALSE /\ afterRevoked = FALSE

\* Whom the grant names: the wildcard reads for every node, the address
\* only for b. The "address" bug grants every node whatever the address.
Named(n) == access = "public" \/ n = "b" \/ Bug = "address"

\* The grant is counted with its registration, one per created document,
\* which no bound on new changes limits. The "counted" bug counts it against
\* the bounds on ordinary owed changes instead, so a transaction past them
\* cannot stage it.
Counted == Bug = "counted" /\ overBound

\* How many unsettled changes admission lets the creator hold, scaled down
\* from the code's bounds: one.
Limit == 1

\* Staging the grant adds to the admission count only under the "admitted"
\* bug; settling it takes from the count under that bug and under the
\* "unbalanced" bug, which takes away what staging never added.
AddsGrant == IF Bug = "admitted" THEN 1 ELSE 0
TakesGrant == IF Bug \in {"admitted", "unbalanced"} THEN 1 ELSE 0

\* The create commits: its genesis frame, its owed registration and, in the
\* same transaction, its owed grant. The "separate" bug leaves the grant to
\* a step after the commit instead.
Create == /\ ~created /\ ~Counted
          /\ created' = TRUE /\ written' = {1} /\ owedReg' = TRUE
          /\ owedGrant' = (Bug /= "separate")
          /\ postCommit' = (Bug = "separate")
          /\ count' = count + (IF Bug /= "separate" THEN AddsGrant ELSE 0)
          /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, overBound, createRefused, access, crashed, reg, grant, refused, revoked, everGranted,
                         seenReg, seenGrant, seenRevoked, delivered, merged,
                         early, afterRevoked>>

\* Under the "counted" bug, staging the grant fails and the whole create,
\* which its registrations allow, is refused.
RefuseCreate == /\ ~created /\ ~createRefused /\ Counted
                /\ createRefused' = TRUE
                /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, access, created, owedReg, owedGrant, postCommit,
                               crashed, reg, grant, refused, revoked, everGranted,
                               seenReg, seenGrant, seenRevoked, written, delivered,
                               merged, early, afterRevoked>>

\* The "separate" bug's step after the commit: it stages the grant then.
StageAfterCommit == /\ postCommit
                    /\ owedGrant' = TRUE /\ postCommit' = FALSE
                    /\ count' = count + AddsGrant
                    /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, overBound, createRefused, access, created, owedReg, crashed, reg, grant, refused,
                                   revoked, everGranted, seenReg, seenGrant,
                                   seenRevoked, written, delivered, merged,
                                   early, afterRevoked>>

\* a crashes once: what it had durably recorded survives, work it only
\* meant to do after the commit is lost.
Crash == /\ ~crashed
         /\ crashed' = TRUE /\ postCommit' = FALSE
         /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, reg, grant, refused,
                        revoked, everGranted, seenReg, seenGrant, seenRevoked,
                        written, delivered, merged, early, afterRevoked>>

\* The provider takes the owed registration.
SettleReg == /\ owedReg
             /\ reg' = TRUE /\ owedReg' = FALSE
             /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedGrant, postCommit, crashed, grant,
                            refused, revoked, everGranted, seenReg, seenGrant,
                            seenRevoked, written, delivered, merged, early,
                            afterRevoked>>

\* The owed grant is sent, behind its object's registration. Sent before
\* the provider holds the object (the "order" bug), the provider refuses it
\* for good and the outbox drops it.
SettleGrant == /\ owedGrant
               /\ (~owedReg \/ Bug = "order")
               /\ owedGrant' = FALSE
               /\ count' = count - TakesGrant
               /\ IF reg
                    THEN grant' = TRUE /\ everGranted' = TRUE /\ UNCHANGED refused
                    ELSE refused' = TRUE /\ UNCHANGED <<grant, everGranted>>
               /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, overBound, createRefused, access, created, owedReg, postCommit, crashed, reg,
                              revoked, seenReg, seenGrant, seenRevoked, written,
                              delivered, merged, early, afterRevoked>>

\* The creator stages an ordinary change of its own, such as a revocation:
\* admission refuses it when the creator's count has reached the bound.
StageOrdinary == /\ created /\ ~ordinaryOwed /\ ~ordinaryRefused
                 /\ IF count >= Limit
                      THEN ordinaryRefused' = TRUE /\ UNCHANGED <<ordinaryOwed, count>>
                      ELSE ordinaryOwed' = TRUE /\ count' = count + 1
                           /\ UNCHANGED ordinaryRefused
                 /\ UNCHANGED <<overBound, createRefused, access, created, owedReg,
                                owedGrant, postCommit, crashed, reg, grant, refused,
                                revoked, everGranted, seenReg, seenGrant, seenRevoked,
                                written, delivered, merged, early, afterRevoked>>

\* The provider takes the ordinary change; its count is settled.
SettleOrdinary == /\ ordinaryOwed /\ ~owedReg
                  /\ ordinaryOwed' = FALSE /\ count' = count - 1
                  /\ UNCHANGED <<ordinaryRefused, overBound, createRefused, access,
                                 created, owedReg, owedGrant, postCommit, crashed, reg,
                                 grant, refused, revoked, everGranted, seenReg,
                                 seenGrant, seenRevoked, written, delivered, merged,
                                 early, afterRevoked>>

\* The creator revokes the reader the provider holds.
Revoke == /\ grant
          /\ grant' = FALSE /\ revoked' = TRUE
          /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit, crashed,
                         reg, refused, everGranted, seenReg, seenGrant,
                         seenRevoked, written, delivered, merged, early,
                         afterRevoked>>

\* The creator writes again on a.
Write == /\ created /\ 2 \notin written
         /\ written' = written \cup {2}
         /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit, crashed,
                        reg, grant, refused, revoked, everGranted, seenReg,
                        seenGrant, seenRevoked, delivered, merged, early,
                        afterRevoked>>

\* A receiver's light client catches up with the provider.
Observe(n) == /\ seenReg' = [seenReg EXCEPT ![n] = reg]
              /\ seenGrant' = [seenGrant EXCEPT ![n] = grant]
              /\ seenRevoked' = [seenRevoked EXCEPT ![n] = revoked]
              /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit, crashed,
                             reg, grant, refused, revoked, everGranted, written,
                             delivered, merged, early, afterRevoked>>

\* Replication brings a written frame to a receiver.
Deliver(n, f) == /\ f \in written /\ f \notin delivered[n]
                 /\ delivered' = [delivered EXCEPT ![n] = @ \cup {f}]
                 /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit,
                                crashed, reg, grant, refused, revoked, everGranted,
                                seenReg, seenGrant, seenRevoked, written, merged,
                                early, afterRevoked>>

\* The receiver's gate, from what its light client saw: the document is
\* registered, and the receiver may read it. Bugs: "unregistered" treats an
\* unregistered document as public, as local ACP does; "ungated" lets every
\* registered document in; "staged" counts a grant still owed as held;
\* "memo" lets a later frame in on the genesis's earlier answer.
Readable(n) == seenGrant[n] /\ Named(n)
Gate(n, f) == IF ~seenReg[n]
                THEN Bug = "unregistered"
                ELSE \/ Readable(n)
                     \/ Bug = "ungated"
                     \/ Bug = "staged" /\ owedGrant /\ Named(n)
                     \/ Bug = "memo" /\ f = 2 /\ 1 \in merged[n]

\* A receiver merges a delivered frame, its history first, when its gate
\* lets it in; otherwise the frame waits to be tried again.
Merge(n, f) == /\ f \in delivered[n] /\ f \notin merged[n]
               /\ (f = 1 \/ 1 \in merged[n])
               /\ Gate(n, f)
               /\ merged' = [merged EXCEPT ![n] = @ \cup {f}]
               /\ early' = (early \/ ~everGranted)
               /\ afterRevoked' = (afterRevoked \/ seenRevoked[n])
               /\ UNCHANGED <<ordinaryOwed, ordinaryRefused, count, overBound, createRefused, access, created, owedReg, owedGrant, postCommit,
                              crashed, reg, grant, refused, revoked, everGranted,
                              seenReg, seenGrant, seenRevoked, written, delivered>>

\* Every step any actor can take.
Next == \/ Create \/ RefuseCreate \/ StageAfterCommit \/ Crash \/ SettleReg \/ SettleGrant
        \/ Revoke \/ Write \/ StageOrdinary \/ SettleOrdinary
        \/ \E n \in Receivers : Observe(n)
        \/ \E n \in Receivers, f \in Frames : Deliver(n, f) \/ Merge(n, f)

\* Safety: any interleaving.
Spec == Init /\ [][Next]_vars

\* Liveness: the creator, the provider, the light clients and replication
\* keep working. A crash and a revocation may happen, but need not.
LiveSpec == /\ Spec
            /\ WF_vars(Create) /\ WF_vars(StageAfterCommit)
            /\ WF_vars(SettleReg) /\ WF_vars(SettleGrant)
            /\ \A r \in Receivers : WF_vars(Observe(r))
            /\ \A r \in Receivers, f \in Frames :
                  WF_vars(Deliver(r, f)) /\ WF_vars(Merge(r, f))

\* A node merges the document only if the collection's access names it.
INV_OnlyGranted == \A n \in Receivers :
                      merged[n] /= {} => (access = "public" \/ n = "b")

\* No node merges on a grant the provider has never held.
INV_SettledFirst == ~early

\* No node merges more of the document once it has seen the reader revoked.
INV_RevokedStops == ~afterRevoked

\* No node merges a document whose registration the provider never held.
INV_RegisteredFirst == \A n \in Receivers : merged[n] /= {} => reg

\* The owed grant never takes an ordinary change's place: the creator's
\* ordinary change is admitted.
INV_OrdinaryAdmitted == ~ordinaryRefused

\* The admission count is exactly the ordinary changes owed: the grant is
\* neither added nor taken away.
INV_CountBalanced == count = IF ordinaryOwed THEN 1 ELSE 0

\* A create its registrations allow is never refused for the grants it owes.
INV_ValidBatchCommits == ~createRefused

\* Unless the reader is revoked, the addressed node merges the genesis.
LIVE_AddressedMerges == <>(revoked \/ 1 \in merged["b"])
=============================================================================
