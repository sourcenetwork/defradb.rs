------------------------- MODULE ProjectionNoEscalation -------------------------
\* The story. A requester writes a document. The host's projector then
\* changes relationships, on that document or, through project_related and
\* startup reconciliation, on any other document, with the same-document
\* guard switched off. A relationship the projector changes is changed as the
\* requester (the node, for an actorless request), under the requester's own
\* node permission and document authority.
\*
\* What must never happen: the projector grants a relationship the requester
\* could not have granted directly with its own request; in particular it
\* never changes the owner relation, and it never borrows the document owner's
\* or the node's authority for a requester who lacks it.
\*
\* The code: DocumentMutationContext::change_projected_relationship in
\* crates/database/db/src/document_projection.rs (the `reconciling` flag lifts
\* only the same-document check), with the direct path it must not exceed,
\* add_dac_actor_relationship_as through DocumentACP::add_relationship.
EXTENDS Naturals

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
CONSTANT Bug

\* Docs: the document written and another one. Relations: an ordinary
\* relation and the owner relation.
Docs == {"written", "other"}
Relations == {"reader", "owner"}

\* nac: the requester holds the node permission to add relationships.
\* manages: the documents whose relation the requester may manage (owns or
\* holds a managing relation on). granted: the relationships the projector
\* published, as <<document, relation>> pairs.
VARIABLES nac, manages, granted

\* Every variable, for stuttering steps.
vars == <<nac, manages, granted>>

\* Any requester: with or without the node permission, managing any set of
\* documents. Nothing is published yet.
Init == /\ nac \in BOOLEAN
        /\ manages \in SUBSET Docs
        /\ granted = {}

\* What the requester could grant with a direct request: an ordinary
\* relation on a document it manages, holding the node permission.
Direct(d, r) == nac /\ d \in manages /\ r # "owner"

\* The projector's guard, as change_projected_relationship applies it with
\* the same-document check lifted. Each bug drops one conjunct: "nac" the
\* node permission, "authority" the document authority (the projector acts
\* with the owner's or the node's authority), "owner" the owner refusal.
Projected(d, r) == /\ (nac \/ Bug = "nac")
                   /\ (d \in manages \/ Bug = "authority")
                   /\ (r # "owner" \/ Bug = "owner")

\* The projector tries a relationship on either document.
Project(d, r) == /\ Projected(d, r)
                 /\ granted' = granted \cup {<<d, r>>}
                 /\ UNCHANGED <<nac, manages>>

\* Any attempt, in any order.
Next == \E d \in Docs, r \in Relations : Project(d, r)

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* Everything the projector published, the requester could have granted
\* directly.
INV_NoEscalation == \A g \in granted : Direct(g[1], g[2])

\* The projector never changes the owner relation.
INV_NoOwner == \A g \in granted : g[2] # "owner"
=============================================================================
