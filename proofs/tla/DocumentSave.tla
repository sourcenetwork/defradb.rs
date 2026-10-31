---------------------------- MODULE DocumentSave ----------------------------
\* The story, in plain words. A caller saves a document under an ID it
\* names: Go's SaveDocument. The ID may name nothing yet, a live document,
\* or a deleted one. While the save runs, another writer may create the
\* document under that ID, and another may delete it.
\*
\* The save first looks the ID up, deleted documents included, then writes.
\* Its write is an upsert keyed by the ID: a live document is updated where
\* it is, an absent one is added under the ID. But the upsert cannot see a
\* deleted document, so writing over one adds the content as a brand-new
\* document under another ID: a fork, the caller's save landing somewhere
\* it never named.
\*
\* What must never happen: a save forks. A deleted document must be refused
\* with Go's "a document with the given ID has been deleted", and the
\* lookup and the write must decide on the same state, so nothing deletes
\* the document between them.
\*
\* Rust: crates/query/src/documents/mod.rs `save` (the lookup, the refusal,
\* the upsert) and crates/defra-node/src/ops/document.rs `Node::save_doc`
\* (one transaction around them: the caller's, else its own). The regression
\* is mutation::update::with_delete::update_save_deleted_doc_does_nothing.
\*
\* Assumed boundary: inside one transaction regolith validates the lookup's
\* read of the document at commit, so a delete or create that commits
\* between the lookup and the write makes the save's commit fail with a
\* conflict instead of landing.

EXTENDS Naturals

\* CheckDeleted: the save refuses a document its lookup found deleted.
\* OneTransaction: the lookup and the write run in one transaction.
CONSTANTS CheckDeleted, OneTransaction

\* slot: the state of the document under the saved ID.
\* version: how many commits changed the slot, what validation compares.
\* forks: documents a save added under some other ID.
\* pc: where the save is: before its lookup, after it, or finished.
\* seen, seenVersion: what the lookup found, and the version it found.
\* outcome: how the save ended.
\* everDeleted: the document was deleted at some point.
VARIABLES slot, version, forks, pc, seen, seenVersion, outcome, everDeleted

\* Every variable, for the steps that leave them alone.
vars == <<slot, version, forks, pc, seen, seenVersion, outcome, everDeleted>>

\* The three states a document's ID can be in.
States == {"absent", "live", "deleted"}

\* Start from every state the ID can be in, with nothing done yet.
Init == /\ slot \in States
        \* No commit has changed the slot yet.
        /\ version = 0
        \* No save has forked yet.
        /\ forks = 0
        \* The save has not looked the ID up yet.
        /\ pc = "start"
        \* Nothing seen yet.
        /\ seen = "none"
        \* The version the lookup saw, zero until it looks.
        /\ seenVersion = 0
        \* The save has not ended.
        /\ outcome = "none"
        \* A document that starts deleted was deleted.
        /\ everDeleted = (slot = "deleted")

\* The save looks the ID up, deleted documents included.
Lookup == /\ pc = "start"
          \* It records the state it found and the version it found it at.
          /\ seen' = slot
          /\ seenVersion' = version
          \* Next it decides.
          /\ pc' = "looked"
          /\ UNCHANGED <<slot, version, forks, outcome, everDeleted>>

\* The save refuses a document its lookup found deleted, in Go's words.
Refuse == /\ pc = "looked"
          \* Only when the save checks, and only for a deleted document.
          /\ CheckDeleted
          /\ seen = "deleted"
          \* It ends without writing.
          /\ pc' = "done"
          /\ outcome' = "refused"
          /\ UNCHANGED <<slot, version, forks, seen, seenVersion, everDeleted>>

\* The upsert's write, on the slot as it is when the write runs.
\* A live document is updated in place; an absent one is added under the
\* ID; a deleted one is invisible to the upsert, so the content is added
\* under another ID.
Write == \/ /\ slot = "live"
            \* The update changes the document: one more commit.
            /\ version' = version + 1
            /\ outcome' = "updated"
            /\ UNCHANGED <<slot, forks>>
         \/ /\ slot = "absent"
            \* The add makes the document live under the ID.
            /\ slot' = "live"
            /\ version' = version + 1
            /\ outcome' = "added"
            /\ UNCHANGED forks
         \/ /\ slot = "deleted"
            \* The fork: a new document somewhere else.
            /\ forks' = forks + 1
            /\ outcome' = "forked"
            /\ UNCHANGED <<slot, version>>

\* The save commits its write when it did not refuse.
Commit == /\ pc = "looked"
          \* A save that refuses does not get here.
          /\ ~(CheckDeleted /\ seen = "deleted")
          /\ pc' = "done"
          \* In one transaction, a slot changed since the lookup fails the
          \* commit with a conflict and nothing is written. Split across two
          \* transactions, nothing ties the write to what the lookup saw.
          /\ IF OneTransaction /\ version # seenVersion
                THEN /\ outcome' = "conflict"
                     /\ UNCHANGED <<slot, version, forks>>
                ELSE Write
          /\ UNCHANGED <<seen, seenVersion, everDeleted>>

\* Another writer deletes the live document.
Delete == /\ slot = "live"
          /\ slot' = "deleted"
          \* A delete is a commit that changes the slot.
          /\ version' = version + 1
          /\ everDeleted' = TRUE
          /\ UNCHANGED <<forks, pc, seen, seenVersion, outcome>>

\* Another writer creates the document under the ID.
Create == /\ slot = "absent"
          /\ slot' = "live"
          \* A create is a commit that changes the slot.
          /\ version' = version + 1
          /\ UNCHANGED <<forks, pc, seen, seenVersion, outcome, everDeleted>>

\* Any of the save's steps, or the other writers', in any order. Create and
\* Delete each fire at most once (absent, live, deleted only moves forward),
\* so the state space is finite.
Next == Lookup \/ Refuse \/ Commit \/ Delete \/ Create

\* Promise: every variable keeps to its range.
TypeOK == /\ slot \in States
          /\ version \in 0..3
          /\ forks \in 0..1
          /\ pc \in {"start", "looked", "done"}
          /\ seen \in States \cup {"none"}
          /\ outcome \in {"none", "refused", "conflict", "updated", "added", "forked"}
          /\ everDeleted \in BOOLEAN

\* Promise: a save never lands its content under an ID the caller did not
\* name.
INV_NoFork == forks = 0

\* Promise: a deleted document stays deleted; no save brings it back.
INV_DeletedStaysDeleted == everDeleted => slot = "deleted"

\* Promise: a save is refused only for a document its lookup found deleted.
INV_RefusedOnlyDeleted == outcome = "refused" => seen = "deleted"

=============================================================================
