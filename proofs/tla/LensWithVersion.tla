---------------------------- MODULE LensWithVersion ----------------------------
\* The story, in plain words.
\*
\* A patch (or a migration set in an explicit transaction) commits a new
\* collection version whose PreviousVersion names a lens. After the commit
\* the database publishes the version, so other requests start to see it,
\* and later registers the lens in the in-memory transform store (it compiles
\* the module, which takes a while).
\*
\* Who acts:
\*   - the committer: commits, publishes, then registers the lens;
\*   - a request: once it sees the version, it uses the lens to migrate a
\*     document. If the lens is owed (committed, not registered yet), the use
\*     registers it first. If the store knows nothing of the lens, the use
\*     fails with "transform not found" (issue #2917).
\*
\* What must never happen: a request that sees the version finds no
\* transform for it.
\*
\* The fix: the commit owes the lens in its ordered publication, before the
\* version itself is published, and registering clears the owed entry only
\* after the lens is in the store.
\*
\* Rust owners:
\*   crates/database/db/src/definition/migration/committed_lenses.rs
\*     (DB::owe_lens_on_commit, CommittedLensStore::owe, register)
\*   crates/database/db/src/definition/patch/store.rs (DB::store_new_version)
\*   crates/database/db/src/txn/registry/schemas.rs
\*     (DbTransactionRegistry::set_migration_in_txn_as)
EXTENDS Naturals

\* Mode picks which guard is in place: "safe" keeps them all; each other mode
\* removes one guard, for a RED configuration.
CONSTANT Mode
\* The modes the model knows.
ASSUME Mode \in {"safe", "noOwe", "oweAfterPublish", "clearFirst"}

\* committed: the version and its lens config are durable.
\* owed: the lens is in the owed map (known, registered on first use).
\* published: requests can see the version.
\* inStore: the lens is registered in the transform store.
\* pc: where the committer is in its steps.
\* failed: some request saw the version and found no transform.
VARIABLES committed, owed, published, inStore, pc, failed

\* Every variable, for stuttering steps.
vars == <<committed, owed, published, inStore, pc, failed>>

\* Each variable has the shape the story gives it.
TypeOK == /\ committed \in BOOLEAN
          /\ owed \in BOOLEAN
          /\ published \in BOOLEAN
          /\ inStore \in BOOLEAN
          /\ pc \in {"commit", "publish", "late", "register", "clear", "done"}
          /\ failed \in BOOLEAN

\* Nothing has happened yet.
Init == /\ committed = FALSE
        /\ owed = FALSE
        /\ published = FALSE
        /\ inStore = FALSE
        /\ pc = "commit"
        /\ failed = FALSE

\* The transaction commits: the version and the lens config are durable.
Commit == /\ pc = "commit"
          /\ committed' = TRUE
          /\ pc' = "publish"
          /\ UNCHANGED <<owed, published, inStore, failed>>

\* The ordered publication. In the safe design it owes the lens and publishes
\* the version in one step, the owe first. "noOwe" publishes without owing
\* (the bug). "oweAfterPublish" publishes now and owes in a later step.
Publish == /\ pc = "publish"
           /\ published' = TRUE
           \* Only the safe and clearFirst modes owe here.
           /\ owed' = (Mode \in {"safe", "clearFirst"})
           \* oweAfterPublish still has its late owe to do.
           /\ pc' = IF Mode = "oweAfterPublish" THEN "late" ELSE "register"
           /\ UNCHANGED <<committed, inStore, failed>>

\* The late owe of the "oweAfterPublish" mode.
LateOwe == /\ pc = "late"
           /\ owed' = TRUE
           /\ pc' = "register"
           /\ UNCHANGED <<committed, published, inStore, failed>>

\* The follow-up registration. Safe: the lens enters the store, then the
\* owed entry is cleared. "clearFirst" clears the entry first, then adds.
RegisterStep == /\ pc = "register"
                /\ IF Mode = "clearFirst"
                      \* The bug: the owed entry goes before the lens is in.
                      THEN /\ owed' = FALSE
                           /\ UNCHANGED inStore
                      \* Safe: the lens is in the store first.
                      ELSE /\ inStore' = TRUE
                           /\ UNCHANGED owed
                /\ pc' = "clear"
                /\ UNCHANGED <<committed, published, failed>>

\* The second half of the registration.
ClearStep == /\ pc = "clear"
             /\ IF Mode = "clearFirst"
                   \* The lens finally enters the store.
                   THEN /\ inStore' = TRUE
                        /\ UNCHANGED owed
                   \* A registered lens is owed no longer.
                   ELSE /\ owed' = FALSE
                        /\ UNCHANGED inStore
             /\ pc' = "done"
             /\ UNCHANGED <<committed, published, failed>>

\* A request that sees the version uses the lens. An owed lens is registered
\* by the use; a lens neither owed nor in the store fails the request.
Use == /\ published
       /\ ~failed
       /\ IF inStore \/ owed
             \* The use finds it, registering it if owed.
             THEN /\ inStore' = TRUE
                  /\ UNCHANGED failed
             \* transform not found.
             ELSE /\ failed' = TRUE
                  /\ UNCHANGED inStore
       /\ UNCHANGED <<committed, owed, published, pc>>

\* Any one of the steps may happen next.
Next == Commit \/ Publish \/ LateOwe \/ RegisterStep \/ ClearStep \/ Use

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* No request that sees the version ever fails to find its transform.
INV_NoMissingTransform == ~failed

\* A visible version's lens is always either registered or owed.
INV_VisibleLensKnown == published => (inStore \/ owed)
=============================================================================
