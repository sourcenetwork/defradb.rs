---- MODULE DeletedHistory ----
\* The story, in plain words.
\*
\* An owner adds a document to a collection that has an access policy. The
\* node registers the document's access-control object: the owner, and any
\* relationship the owner grants (a reader, say). Everyone else (a foreign
\* actor) is denied. An object the node does not hold reads as public: that
\* is how a document added before the collection had a policy stays open.
\*
\* When the owner deletes the document, its rows go, but its history stays
\* on the node: the commits `_commits` lists, the blocks served by CID and
\* over P2P. That history must stay under the same decision as before the
\* delete. If the delete removed the object, the history would read as
\* public and the foreign actor would read it. So a delete keeps the object,
\* as Go does: the reader still reads, the foreign actor still does not, and
\* the owner can still revoke the reader, because the owner is still there.
\* The delete commits in one transaction, or not at all, and nothing about
\* this changes when the node restarts: the object lives in the same store.
\*
\* A deleted document's DocID cannot be added again while its history is
\* there: the create is refused. A purge removes the
\* history, and with it the object, so the same content can be added again
\* under the same DocID, by the same creator or another; that new document
\* belongs to its creator alone, takes no grant from the one that was purged,
\* and denies the previous owner.
\*
\* A truncate is outside this model. It keeps the object and grants, as Go
\* does for `go.collection.truncate.acp`, while this model covers deletes,
\* purges and re-creation.
\*
\* What must never happen: anyone reads a document's history whom its
\* current owner has not let read it (INV_NoUnintendedRead).
\*
\* Code this models:
\*   crates/database/db/src/write/autocommit/helpers.rs     archive_deleted_doc (a local
\*       delete keeps the object; only vera.rs, which denies a missing object, archives)
\*   crates/database/db/src/write/autocommit/mod.rs         register_deleted_doc_with_acp_as
\*       (no unregistration on a provider that reads a missing object as public)
\*   crates/engine/query/src/runner/mutation.rs             execute_single_mutation (the
\*       same rule for a mutator that leaves ACP hooks to the runner)
\*   crates/database/db/src/merge/merge_handler/composite_persist.rs  handle_deletion (a
\*       delete merged from a peer keeps the object too)
\*   crates/database/db/src/collection/acp.rs               release_purged_doc_objects (a
\*       purge removes the object with the history; a truncate keeps it, as Go does)
\*   crates/database/db/src/write/autocommit/helpers.rs     register_created_doc (refuses
\*       "a document with the given ID has been deleted": the fence)
\*   crates/engine/acp/src/read_access.rs                   DirectChecker::object_access
\*       (a missing object reads as public)
EXTENDS Naturals

\* Each guard can be switched off by a RED configuration.
\* KeepOnDelete: a committed delete leaves the object and its grants alone.
\* ReleaseOnPurge: a purge removes the object together with the history.
\* FenceRecreate: a create of a DocID whose deleted history is still there is refused.
CONSTANTS KeepOnDelete, ReleaseOnPurge, FenceRecreate

\* The first owner and a different creator who may re-create after a purge.
Owner == "owner"
OtherOwner == "other-owner"
Reader == "reader"
Foreign == "foreign"
\* Everyone who might try to read.
Actors == {Owner, OtherOwner, Reader, Foreign}

\* phase: "live" (rows and history), "deleted" (history only) or "gone" (purged).
\* pending: a delete is staged in an open transaction.
\* owner: the creator of the document this DocID currently names.
\* registered: the node holds the document's access-control object.
\* grants: the actors the object lets read besides its owner.
\* intended: who the current owner means to let read, besides itself; the oracle.
\* leaked: someone outside the owner's intent read the history.
VARIABLES phase, pending, owner, registered, grants, intended, leaked

\* Every variable, for stuttering steps.
vars == <<phase, pending, owner, registered, grants, intended, leaked>>

\* The kinds of values each variable may hold.
TypeOK ==
  /\ phase \in {"live", "deleted", "gone"}
  /\ pending \in BOOLEAN
  /\ owner \in Actors
  /\ registered \in BOOLEAN
  /\ grants \subseteq {Reader}
  /\ intended \subseteq {Reader}
  /\ leaked \in BOOLEAN

\* The history is on the node while the document is live or deleted.
HasHistory == phase \in {"live", "deleted"}

\* The read rule (DirectChecker): no object reads as public; an object lets
\* the document's current owner and its grants read.
Allows(a) == ~registered \/ a = owner \/ a \in grants

\* The owner has just added the document: it is registered, nobody is granted.
Init ==
  /\ phase = "live"
  /\ pending = FALSE
  /\ owner = Owner
  /\ registered = TRUE
  /\ grants = {}
  /\ intended = {}
  /\ leaked = FALSE

\* The owner grants the reader; this needs the object (the owner relation).
Grant ==
  /\ HasHistory /\ registered
  /\ grants' = grants \cup {Reader}
  \* The owner now means the reader to read.
  /\ intended' = intended \cup {Reader}
  /\ UNCHANGED <<phase, pending, owner, registered, leaked>>

\* The owner revokes the reader. The owner's intent changes whatever the
\* node holds; the object only changes if the node still has it.
Revoke ==
  /\ HasHistory
  /\ intended' = intended \ {Reader}
  /\ grants' = IF registered THEN grants \ {Reader} ELSE grants
  /\ UNCHANGED <<phase, pending, owner, registered, leaked>>

\* The owner stages a delete of the live document in a transaction.
BeginDelete ==
  /\ phase = "live" /\ ~pending
  /\ pending' = TRUE
  /\ UNCHANGED <<phase, owner, registered, grants, intended, leaked>>

\* The transaction commits: the rows go, the history stays. A delete merged
\* from a peer commits the same way. Without KeepOnDelete the delete also
\* removes the object (the bug this issue fixed).
CommitDelete ==
  /\ pending
  /\ pending' = FALSE
  /\ phase' = "deleted"
  /\ registered' = IF KeepOnDelete THEN registered ELSE FALSE
  /\ grants' = IF KeepOnDelete THEN grants ELSE {}
  /\ UNCHANGED <<owner, intended, leaked>>

\* The transaction is discarded, or the node restarts before it commits:
\* nothing it staged lands.
AbortDelete ==
  /\ pending
  /\ pending' = FALSE
  /\ UNCHANGED <<phase, owner, registered, grants, intended, leaked>>

\* A purge removes the history and the DocID mappings. With ReleaseOnPurge
\* it removes the object too.
Purge ==
  /\ HasHistory /\ ~pending
  /\ phase' = "gone"
  /\ registered' = IF ReleaseOnPurge THEN FALSE ELSE registered
  /\ grants' = IF ReleaseOnPurge THEN {} ELSE grants
  /\ UNCHANGED <<pending, owner, intended, leaked>>

\* A creator adds the same content again, under the same DocID. Its create
\* registers the object for that creator (a registration already there is kept,
\* grants and all). The owner of the new document has granted nobody. With
\* FenceRecreate a deleted document's DocID is refused.
Recreate ==
  \E creator \in Actors :
    /\ phase = "gone" \/ (phase = "deleted" /\ ~FenceRecreate)
    /\ ~pending
    /\ phase' = "live"
    /\ owner' = creator
    /\ registered' = TRUE
    /\ intended' = {}
    /\ UNCHANGED <<pending, grants, leaked>>

\* Actor a reads the history (`_commits`, a block, a P2P fetch). A read the
\* rule allows to someone outside the owner's intent is a leak.
Read(a) ==
  /\ HasHistory /\ Allows(a)
  /\ leaked' = (leaked \/ (a # owner /\ a \notin intended))
  /\ UNCHANGED <<phase, pending, owner, registered, grants, intended>>

\* One step is any of the above.
Next ==
  \/ Grant \/ Revoke \/ BeginDelete \/ CommitDelete \/ AbortDelete
  \/ Purge \/ Recreate
  \/ \E a \in Actors : Read(a)

\* Every behaviour starts at Init and takes steps of Next.
Spec == Init /\ [][Next]_vars

\* Nobody ever reads a document's history whom its current owner has not let
\* read it: not the foreign actor after a delete, not a revoked reader, and
\* not the previous owner when a different creator adds the DocID again after
\* a purge.
INV_NoUnintendedRead == ~leaked
====
