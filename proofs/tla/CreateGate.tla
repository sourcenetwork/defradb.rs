---------------------------- MODULE CreateGate ----------------------------
\* The story, in plain words.
\*
\* A collection's policy says who may add a document to it (the `create` of
\* its resource's `collection` section, such as `author->publish`). A
\* document's ID comes from its content, so two people who write the same book
\* get the same ID, and only the first one to commit holds it. Two people act:
\* "a" and "b". Someone else grants and revokes their right to create at any
\* time, on the provider that holds the policy.
\*
\* Two providers:
\*   * the local engine registers the document inside the create's own
\*     transaction, so it decides `create` there, on the rights as they are;
\*   * Vera (remote) cannot join the transaction. Before the transaction the
\*     node reads Vera's certified state, which may lag behind the rights, and
\*     decides `create` on it; inside the transaction a refused create writes
\*     nothing. After the commit the node owes Vera the registration, and Vera
\*     decides `create` again on the rights as they are then, which is final.
\*
\* While Vera does not hold the registration of a document in such a
\* collection, nobody reads it, its creator included. A refusal at
\* registration is final and never sent again. A takeover can void a
\* registration Vera held; it is owed again, and still hidden.
\*
\* A document that arrives from a peer (a replica) is owed as this node's own
\* registration with no relationships, which Vera refuses as a create; it
\* clears once the creator's own registration lands, and is sent a bounded
\* number of times before it is parked.
\*
\* What must never happen:
\*   1. someone holds the document's ID who was not admitted by the rights the
\*      node decided on when it committed (the certified state for Vera, the
\*      rights themselves for the local engine) - INV_NoSquat;
\*   2. a create without an identity commits - INV_NoAnonymous;
\*   3. anyone reads the document before Vera holds its registration
\*      - INV_HiddenUntilRegistered;
\*   4. a registration Vera refused as a create is sent again
\*      - INV_RefusalFinal;
\*   5. a replica's registration is sent more than MaxReplicaSends times
\*      - INV_ReplicaBounded.
\*
\* What the model leaves open on purpose: someone admitted when the node
\* decided, whose right was revoked before Vera checked, holds the ID of a
\* document that stays hidden. That case is the product's to decide.
\*
\* The code: the decision before the transaction is the CreateDecision input
\* resolved by resolve_create_decision in crates/database/db/src/write/
\* prepare_create.rs through authorize_create_through in crates/engine/acp/
\* src/create_decision.rs, and the create refuses on it in create_gate,
\* crates/database/db/src/write/create/gate.rs. The local engine decides in
\* the create's transaction in register_created_doc, crates/database/db/src/
\* write/autocommit/helpers.rs. Hidden while owed: PendingRegistration::hidden
\* in crates/engine/acp/src/pending.rs; kept on a void: owe_again in
\* crates/engine/acp/src/external/voided.rs; the final refusal:
\* PendingRegistration::create_refused and note_failure in crates/engine/acp/
\* src/external/registration.rs; the replica bound and its clearing:
\* MAX_REPLICA_CREATE_SENDS and parked_create there. The anonymous refusal:
\* create_gate in gate.rs. Lean proves the decision equal to acp-rs's
\* (proofs/lean/CreateDecision.lean).

EXTENDS Naturals

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
\* Remote is TRUE for Vera and FALSE for the local engine.
\* MaxReplicaSends bounds how often a replica's registration is sent.
CONSTANTS Bug, Remote, MaxReplicaSends

\* rights:    who may create right now, on the provider.
\* certified: who may create in the certified state the node last read.
\* holder:    who holds the document's ID on this node, "none" before.
\* admitted:  whether the holder was admitted by the rights the node decided
\*            on when it committed; kept only to state promise 1.
\* reg:       the registration: "none", "owed", "refused" or "held".
\* hidden:    whether the owed record says the document is hidden.
\* resent:    turns TRUE when a refused registration is sent again.
\* replica:   a replica's record: "none", "owed", "parked" or "cleared".
\* replicaSends: how often the replica's registration was sent.
\* origin:    the creator's own registration of the replica on its node:
\*            "pending", "held" or "refused".
VARIABLES rights, certified, holder, admitted, reg, hidden, resent,
          replica, replicaSends, origin

\* Every variable, for stuttering steps.
vars == <<rights, certified, holder, admitted, reg, hidden, resent,
          replica, replicaSends, origin>>

\* The two people who may try to create the same document.
People == {"a", "b"}

\* Any rights to start with; the node has read them; nothing exists yet.
Init == /\ rights \in SUBSET People
        /\ certified = rights
        /\ holder = "none"
        /\ admitted = TRUE
        /\ reg = "none"
        /\ hidden = TRUE
        /\ resent = FALSE
        /\ replica = "none"
        /\ replicaSends = 0
        /\ origin = "pending"

\* The local engine's state never lags: it decides on the rights themselves.
\* So only Vera's certified state can differ from them.
Rights(view) == IF Remote THEN view ELSE rights

\* Someone grants or revokes create rights on the provider.
ChangeRights == /\ rights' \in SUBSET People
                \* The local engine reads the rights themselves.
                /\ certified' = IF Remote THEN certified ELSE rights'
                /\ UNCHANGED <<holder, admitted, reg, hidden, resent,
                               replica, replicaSends, origin>>

\* The node reads a newer certified state from Vera.
Certify == /\ Remote
           /\ certified' = rights
           /\ UNCHANGED <<rights, holder, admitted, reg, hidden, resent,
                          replica, replicaSends, origin>>

\* Person p creates the document. The guard: p is admitted by the rights the
\* node decides on (the certified state for Vera, decided before the
\* transaction; the rights for the local engine, decided inside it). A refused
\* create writes nothing. Bug "NoPreCheck" skips Vera's decision before the
\* transaction, and "LocalUngated" the local engine's.
Create(p) ==
    \* The ID is free; otherwise the create fails with "already exists".
    /\ holder = "none"
    \* The decision, or the planted bug on its path.
    /\ \/ p \in Rights(certified)
       \/ Remote /\ Bug = "NoPreCheck"
       \/ ~Remote /\ Bug = "LocalUngated"
    \* The ID is taken, and we remember whether that was admitted.
    /\ holder' = p
    /\ admitted' = (p \in Rights(certified))
    \* Vera is owed the registration; the local engine registers at once.
    /\ reg' = IF Remote THEN "owed" ELSE "held"
    \* The owed record says the document is hidden.
    /\ hidden' = TRUE
    /\ UNCHANGED <<rights, certified, resent, replica, replicaSends, origin>>

\* A create without an identity: nobody can be checked, so it is refused in
\* the transaction. Bug "AnonymousAdmitted" lets it commit.
CreateAnonymous ==
    /\ holder = "none"
    /\ Bug = "AnonymousAdmitted"
    /\ holder' = "anonymous"
    /\ admitted' = FALSE
    /\ reg' = IF Remote THEN "owed" ELSE "held"
    /\ hidden' = TRUE
    /\ UNCHANGED <<rights, certified, resent, replica, replicaSends, origin>>

\* The node sends the owed registration; Vera decides `create` on the rights
\* as they are now. Admitted: Vera holds it. Refused: the refusal is final.
Settle == /\ reg = "owed"
          /\ reg' = IF holder \in rights THEN "held" ELSE "refused"
          /\ UNCHANGED <<rights, certified, holder, admitted, hidden, resent,
                         replica, replicaSends, origin>>

\* A refused registration is sent again. Only Bug "ResendRefused" does it.
Resend == /\ reg = "refused"
          /\ Bug = "ResendRefused"
          /\ resent' = TRUE
          /\ reg' = IF holder \in rights THEN "held" ELSE "refused"
          /\ UNCHANGED <<rights, certified, holder, admitted, hidden,
                         replica, replicaSends, origin>>

\* A takeover voids a registration Vera held: it is owed again. The record
\* keeps the document hidden; Bug "VoidDropsGated" loses that.
Void == /\ Remote
        /\ reg = "held"
        /\ reg' = "owed"
        /\ hidden' = (Bug # "VoidDropsGated")
        /\ UNCHANGED <<rights, certified, holder, admitted, resent,
                       replica, replicaSends, origin>>

\* Who reads the document. Vera holding the registration lets the policy
\* decide (anyone here, the policy's readers being beside the point). Owed,
\* a record that is not hidden lets its creator read (the ungated overlay).
Reads(x) == \/ reg = "held"
            \/ reg = "owed" /\ ~hidden /\ x = holder
            \* Bug "CreatorReadsOwed": the creator reads while it is owed.
            \/ Bug = "CreatorReadsOwed" /\ reg = "owed" /\ x = holder

\* A replica arrives from a peer and is owed as this node's registration.
ReceiveReplica == /\ replica = "none"
                  /\ replica' = "owed"
                  /\ UNCHANGED <<rights, certified, holder, admitted, reg,
                                 hidden, resent, replicaSends, origin>>

\* The creator's own registration of the replica, on its node, settles.
OriginSettles == /\ origin = "pending"
                 /\ origin' \in {"held", "refused"}
                 /\ UNCHANGED <<rights, certified, holder, admitted, reg,
                                hidden, resent, replica, replicaSends>>

\* The node settles the replica's record. Held by its creator: cleared,
\* nothing sent. Otherwise it is sent and refused as a create; once it was
\* sent MaxReplicaSends times it is parked. Bug "ReplicaUnbounded" never
\* parks it.
SettleReplica ==
    /\ replica = "owed"
    /\ IF origin = "held"
       THEN /\ replica' = "cleared"
            /\ UNCHANGED replicaSends
       ELSE /\ replicaSends' = replicaSends + 1
            /\ replica' = IF replicaSends + 1 >= MaxReplicaSends
                             /\ Bug # "ReplicaUnbounded"
                          THEN "parked" ELSE "owed"
    /\ UNCHANGED <<rights, certified, holder, admitted, reg, hidden, resent,
                   origin>>

\* A parked replica is cleared, unsent, once its creator's registration is
\* held.
ClearParked == /\ replica = "parked"
               /\ origin = "held"
               /\ replica' = "cleared"
               /\ UNCHANGED <<rights, certified, holder, admitted, reg, hidden,
                              resent, replicaSends, origin>>

\* Every step the system can take.
Next == \/ ChangeRights
        \/ Certify
        \/ \E p \in People : Create(p)
        \/ CreateAnonymous
        \/ Settle
        \/ Resend
        \/ Void
        \/ ReceiveReplica
        \/ OriginSettles
        \/ SettleReplica
        \/ ClearParked

\* The whole behaviour: start, then any steps.
Spec == Init /\ [][Next]_vars

\* Promise 1: a person who holds the ID was admitted by what the node decided
\* on. (A create without an identity is promise 2's.)
INV_NoSquat == holder \in People => admitted

\* Promise 2: no create without an identity commits.
INV_NoAnonymous == holder # "anonymous"

\* Promise 3: under Vera nobody reads the document before Vera holds it.
INV_HiddenUntilRegistered ==
    Remote => \A x \in People : Reads(x) => reg = "held"

\* Promise 4: a refused registration is never sent again.
INV_RefusalFinal == ~resent

\* Promise 5: a replica's registration is sent a bounded number of times.
INV_ReplicaBounded == replicaSends <= MaxReplicaSends

\* The replica counter cannot pass the bound plus one before a check fails,
\* so the state space stays finite in every configuration.
StateBound == replicaSends <= MaxReplicaSends + 1
=============================================================================
