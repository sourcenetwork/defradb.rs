---------------------------- MODULE ProjectionActor ----------------------------
\* The story. A trusted host projector changes a relationship while the node
\* runs a projection. Three things can start it: a request that names no
\* identity (startup reconciliation, replication), a request signed by the node
\* itself, or a request signed by a foreign actor. The projection acts as one
\* actor: the request's own identity, or the node when there is none. A
\* relationship becomes visible only when that actor holds the authority
\* (here: owns the document).
\*
\* What must never happen: the projection acts as anyone other than the
\* request's actor (or the node, for an actorless request), in particular as
\* the document's owner when the actor is not the owner; or a relationship is
\* published by an actor without authority.
\*
\* The code: DocumentMutationContext::change_projected_relationship and
\* projected_owner in crates/database/db/src/document_projection.rs (the actor
\* is NamespaceView::projection_actor, else the node DID), and
\* DB::reconcile_document_projector in crates/database/db/src/database/mod.rs,
\* which runs actorless with reads bound to the node's identity.
EXTENDS Naturals

\* Bug lets a projection publish without the actor's authority.
CONSTANT Bug

\* source: who started the projection. node: the node's DID. owner: the
\* document's owner. actor: whom the projection acts as. visible: whether the
\* relationship was published.
VARIABLES source, node, owner, actor, visible

\* Every variable, for stuttering steps.
vars == <<source, node, owner, actor, visible>>

\* Any source, either owner; no actor chosen and nothing published yet.
Init == /\ source \in {"actorless", "node", "foreign"}
        /\ node = "node" /\ owner \in {"node", "foreign"}
        /\ actor = "unset" /\ visible = FALSE

\* The projection picks its actor: the node for an actorless request, the
\* request's identity otherwise. It never borrows the owner.
Select == /\ actor = "unset"
          /\ actor' = IF source = "actorless" THEN node ELSE source
          /\ UNCHANGED <<source, node, owner, visible>>

\* The relationship is published only when the actor holds the authority;
\* the bug publishes regardless.
Grant == /\ actor # "unset" /\ ~visible
         /\ visible' = (actor = owner \/ Bug)
         /\ UNCHANGED <<source, node, owner, actor>>

\* Selection, then the grant attempt.
Next == Select \/ Grant

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* The projection acts as the request's identity, or as the node when the
\* request has none.
INV_Identity == actor # "unset" => actor = IF source = "actorless" THEN node ELSE source

\* A published relationship was published by an actor with authority.
INV_Authority == visible => actor = owner
=============================================================================
