------------------------ MODULE ParentRegistration ------------------------
\* The story. A policy can say that a document of one resource (a job) is
\* registered only under a parent object (a node), and only by an identity
\* that holds a named permission on that parent. The document carries the
\* parent's ID in one field. One identity acts here; others grant it and
\* revoke from it the permission on each parent at any time. It may create a
\* document locally, receive one as a replica, add a further parent link to a
\* registered document, delete a parent link, rewrite the parent field, and
\* delete the document.
\*
\* What must never happen: a registration or a parent link is written while
\* the identity lacks the permission on that parent; a registered document is
\* left with no parent link; the field of a registered document names another
\* parent than the one it was admitted under.
\*
\* The code: register_created_doc in
\* crates/database/db/src/write/autocommit/helpers.rs and
\* persist_merged_document in
\* crates/database/db/src/merge/merge_handler/composite_persist.rs read the
\* parent from the document; register_under_parent, check_parent_link and
\* check_parent_unlink in crates/engine/acp/src/zanzibar/acp/document_acp.rs
\* decide in the transaction that writes; validate_immutable_fields in
\* crates/database/db/src/collection/validation.rs guards the field.
\* No library module is needed: sets and functions are built in.

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
CONSTANT Bug

\* held: the parents the identity holds the rule's permission on right now.
\* registered: the documents that have an owner.
\* links: the parent links, as pairs of a document and a parent.
\* field: the parent each document's field names, "none" before it exists.
\* admitted: the parent each document was registered under; kept only to
\*           state the promise about the field, the code has no such record.
\* unauthorized: turns TRUE when a write happens without the permission.
VARIABLES held, registered, links, field, admitted, unauthorized

\* Every variable, for stuttering steps.
vars == <<held, registered, links, field, admitted, unauthorized>>

\* Two documents, so one can change while the other stays.
Docs == {"d1", "d2"}

\* Two parents, so a document can be moved or linked to a second one.
Parents == {"a", "b"}

\* Any permissions to start with; nothing is registered or linked yet.
Init == /\ held \in SUBSET Parents
        /\ registered = {}
        /\ links = {}
        /\ field = [d \in Docs |-> "none"]
        /\ admitted = [d \in Docs |-> "none"]
        /\ unauthorized = FALSE

\* Someone grants or revokes the identity's permission on any parents.
ChangeHeld == /\ held' \in SUBSET Parents
              /\ UNCHANGED <<registered, links, field, admitted, unauthorized>>

\* A document is registered under the parent its field names. The guard: the
\* identity holds the permission on that parent, read in the transaction that
\* writes the owner and the link. `source` is "local" for a create on this
\* node and "replica" for a signed genesis that arrived; the bug of the same
\* name skips the guard on that path.
Register(d, p, source) ==
    \* Only a document without an owner is registered.
    /\ d \notin registered
    \* The guard, or the planted bug on this path.
    /\ \/ p \in held
       \/ Bug = source
    \* The owner and the parent link are written together.
    /\ registered' = registered \cup {d}
    /\ links' = links \cup {<<d, p>>}
    \* The document's field names that parent, and it is the admitted one.
    /\ field' = [field EXCEPT ![d] = p]
    /\ admitted' = [admitted EXCEPT ![d] = p]
    \* Remember a write that happened without the permission.
    /\ unauthorized' = (unauthorized \/ p \notin held)
    /\ UNCHANGED held

\* A further parent link is added to a registered document. The guard: the
\* permission on the new parent. The "link" bug skips it.
AddLink(d, p) ==
    \* Only a registered document takes relationships.
    /\ d \in registered
    \* The link is new.
    /\ <<d, p>> \notin links
    \* The guard, or the planted bug.
    /\ (p \in held \/ Bug = "link")
    /\ links' = links \cup {<<d, p>>}
    \* Remember a write that happened without the permission.
    /\ unauthorized' = (unauthorized \/ p \notin held)
    /\ UNCHANGED <<held, registered, field, admitted>>

\* A parent link is deleted. The guard: another parent link of the document
\* remains. The "unlink" bug deletes the last one too.
RemoveLink(d, p) ==
    \* The link exists.
    /\ <<d, p>> \in links
    \* The guard, or the planted bug.
    /\ \/ \E q \in Parents : q # p /\ <<d, q>> \in links
       \/ Bug = "unlink"
    /\ links' = links \ {<<d, p>>}
    /\ UNCHANGED <<held, registered, field, admitted, unauthorized>>

\* An update writes the parent field of a registered document. The guard: it
\* writes the value the field already holds. The "update" bug writes any.
Update(d, p) ==
    \* Only an existing document is updated.
    /\ d \in registered
    \* The guard, or the planted bug.
    /\ (p = field[d] \/ Bug = "update")
    /\ field' = [field EXCEPT ![d] = p]
    /\ UNCHANGED <<held, registered, links, admitted, unauthorized>>

\* The document is deleted: its owner and all its links go together, and the
\* same document ID can be registered again later.
Unregister(d) ==
    \* Only a registered document is unregistered.
    /\ d \in registered
    \* Its owner goes.
    /\ registered' = registered \ {d}
    \* Every link of this document is removed, links of others stay.
    /\ links' = {l \in links : l[1] # d}
    \* Nothing is left of the field or of the admitted parent.
    /\ field' = [field EXCEPT ![d] = "none"]
    /\ admitted' = [admitted EXCEPT ![d] = "none"]
    /\ UNCHANGED <<held, unauthorized>>

\* Permission changes and every operation interleave in any order.
Next == \/ ChangeHeld
        \/ \E d \in Docs, p \in Parents :
            \/ Register(d, p, "local")
            \/ Register(d, p, "replica")
            \/ AddLink(d, p)
            \/ RemoveLink(d, p)
            \/ Update(d, p)
        \/ \E d \in Docs : Unregister(d)

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* No registration and no parent link was written without the permission on
\* its parent at that moment.
AuthorizedAtWrite == ~unauthorized

\* Every registered document has at least one parent link.
RegistrationHasParent == \A d \in registered : \E p \in Parents : <<d, p>> \in links

\* The field of a registered document names the parent it was admitted under.
FieldNamesAdmittedParent == \A d \in registered : field[d] = admitted[d]
=============================================================================
