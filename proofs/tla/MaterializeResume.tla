--------------------------- MODULE MaterializeResume ---------------------------
\* The story, in plain words.
\*
\* A collection has a few documents stored at an older version. A schema
\* change owes the collection a reindex, and its settlement walks the
\* documents a page at a time, saving its cursor with each page. A
\* settlement only fixes the index: it moves no document's version. A crash
\* (or an error) can stop it after any page, leaving the cursor behind.
\*
\* Then an operator asks to materialize the collection: walk every document
\* and save it at the active version. Materializing reuses the reindex pages,
\* so it could pick up the settlement's cursor and carry on from there.
\*
\* What must never happen: materializing reports it is done while a document
\* is still stored at the older version, because it resumed past documents
\* the settlement walked without moving them.
\*
\* How the code keeps the promise: a materialization never resumes the owed
\* reindex; it records a reindex of its own under a new token, and a cursor
\* under another token reads as the start.
\*
\* Rust code modelled:
\*   crates/database/db/src/definition/migration/reindex/mod.rs
\*     DB::reindex_in_pages (the owed reindex is resumed only by a rebuild
\*     that does not restamp; otherwise a new token is owed)
\*   crates/database/db/src/definition/migration/reindex/progress.rs read
\*     (a cursor under another token reads as the start)
\*   crates/database/db/src/definition/migration/reindex/page.rs
\*     ReindexPage::walk (a settlement page moves no version)
EXTENDS Naturals

CONSTANTS
  \* How many documents the collection holds.
  N,
  \* TRUE when a materialization resumes the settlement's cursor (the bug).
  Resume

\* The constants have the shapes they should.
ASSUME N \in Nat /\ N > 0 /\ Resume \in BOOLEAN

\* The documents, by short ID in walk order.
Docs == 1..N

VARIABLES
  \* The documents already saved at the active version.
  stamped,
  \* The cursor: the last document the reindex that owns it walked.
  cursor,
  \* Who owns the owed reindex and its cursor: "settle", "materialize", or
  \* "none" once the last page cleared it.
  owner,
  \* The materialization's progress: "idle", "walking", or "done".
  phase

\* All the variables, for stuttering.
vars == <<stamped, cursor, owner, phase>>

\* Every variable holds a value of its kind.
TypeOK == /\ stamped \subseteq Docs
          /\ cursor \in 0..N
          /\ owner \in {"settle", "materialize", "none"}
          /\ phase \in {"idle", "walking", "done"}

\* At the start a schema change owes a settlement that has walked nothing,
\* no document is at the active version, and no one asked to materialize.
Init == /\ stamped = {}
        /\ cursor = 0
        /\ owner = "settle"
        /\ phase = "idle"

\* A settlement page walks the next document. It fixes the index only, so
\* the document stays at its older version. It runs only while it still owns
\* the owed reindex; a newer owner supersedes it.
SettlePage == /\ owner = "settle"
              /\ cursor < N
              /\ cursor' = cursor + 1
              /\ UNCHANGED <<stamped, owner, phase>>

\* The operator asks to materialize. With the guard on it owes a reindex of
\* its own, whose cursor reads as the start; with it off it takes over the
\* settlement's cursor where it stopped.
MaterializeStart == /\ phase = "idle"
                    /\ phase' = "walking"
                    /\ owner' = "materialize"
                    /\ cursor' = IF Resume /\ owner = "settle" THEN cursor ELSE 0
                    /\ UNCHANGED stamped

\* A materialization page walks the next document and saves it at the
\* active version.
MaterializePage == /\ phase = "walking"
                   /\ cursor < N
                   /\ cursor' = cursor + 1
                   /\ stamped' = stamped \cup {cursor + 1}
                   /\ UNCHANGED <<owner, phase>>

\* The last page: the walk is past the last document, so the owed record is
\* cleared and the materialization reports it is done.
MaterializeDone == /\ phase = "walking"
                   /\ cursor = N
                   /\ phase' = "done"
                   /\ owner' = "none"
                   /\ UNCHANGED <<stamped, cursor>>

\* Any one of the steps above; a crash is a settlement that takes no more.
Next == SettlePage \/ MaterializeStart \/ MaterializePage \/ MaterializeDone

\* The whole behaviour: start in Init, take Next steps (or stutter).
Spec == Init /\ [][Next]_vars

\* A materialization that reports done has saved every document at the
\* active version.
INV_MaterializedAll == phase = "done" => stamped = Docs
================================================================================
