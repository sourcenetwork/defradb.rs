---------------------------- MODULE LensWithVersion ----------------------------
\* The story, in plain words.
\*
\* A patch (or a migration set in an explicit transaction) commits a new
\* collection version whose PreviousVersion names a lens. The transaction has
\* already compiled the lens into a store of its own. The moment the commit
\* lands, a request that reads the version from storage can see it (a fresh
\* read, a P2P merge); the commit's ordered publication runs a little later,
\* possibly behind an earlier commit that is still publishing, and the lens is
\* registered in the database's transform store later still (compiling the
\* module takes a while).
\*
\* Who acts:
\*   - two transactions, a and b, that both carry the same lens (a lens ID is
\*     the CID of its content, so two commits can carry one lens). Each one
\*     either commits or fails;
\*   - for a commit that landed: its ordered publication, and its follow-up
\*     registration, which run independently of each other;
\*   - a request: once any commit landed, it uses the lens to migrate a
\*     document. It finds the lens if the database's store holds it, if it is
\*     owed (the use registers it first), or if a commit in flight holds its
\*     own compiled copy. Otherwise it fails with "transform not found"
\*     (issue #2917).
\*
\* What must never happen: a request that sees the version finds no
\* transform for it; a failed commit leaves its lens behind.
\*
\* The fix: just before the commit is attempted, the transaction holds its
\* compiled copy in flight, under a ticket of its own. The ordered publication
\* owes the lens and only then lets go of that ticket. A failed commit lets go
\* of its own ticket and of no other. The registration clears the owed entry
\* only after the lens is in the store. A use reads the in-flight hold before
\* the owed entry, so a use between the owe and the let-go still finds it.
\*
\* Rust owners:
\*   crates/database/db/src/definition/migration/committed_lenses.rs
\*     (DB::owe_lens_on_commit; CommittedLensStore::hold, let_go, holder,
\*      owe; register)
\*   crates/database/db/src/definition/patch/store.rs (DB::store_new_version)
\*   crates/database/db/src/txn/registry/schemas.rs
\*     (DbTransactionRegistry::set_migration_in_txn_as)
EXTENDS Naturals, Sequences

\* Mode picks which guard is in place: "safe" keeps them all; each other mode
\* removes or breaks one guard, for a RED configuration.
CONSTANT Mode
\* The modes the model knows.
ASSUME Mode \in {"safe", "noHold", "sharedHold", "letGoFirst", "keepOnAbort",
                 "clearFirst", "owedFirst"}

\* The two transactions that carry the lens.
Txns == {"a", "b"}

\* The steps of the ordered publication, in order. The safe order owes the
\* lens before it lets go of the hold; "letGoFirst" swaps them.
Steps == IF Mode = "letGoFirst"
           \* The bug: the hold goes before the lens is owed.
           THEN <<"letGo", "oweCheck", "oweInsert", "done">>
           \* Safe: owe first, then let go.
           ELSE <<"oweCheck", "oweInsert", "letGo", "done">>

\* The maps a use reads. The safe order checks the in-flight hold first;
\* "owedFirst" reverses it and plants the read-order race.
UseSteps == IF Mode = "owedFirst"
              THEN <<"owed", "held">>
              ELSE <<"held", "owed">>

\* pc[t]: where transaction t is: not yet committing, holding its copy just
\* before the commit, committed, or failed.
\* held: the tickets whose compiled copy is held in flight.
\* opc[t]: which step of Steps t's ordered publication is at (0: not started).
\* saw[t]: whether t's owe found the lens in the store already.
\* rpc[t]: where t's follow-up registration is.
\* owed: the lens is in the owed map (known, registered on first use).
\* inStore: the lens is registered in the database's transform store.
\* upc: where the request is in UseSteps: 0 is idle, 1..Len(UseSteps) are
\*   reads, Len(UseSteps)+1 is the store read, and Len(UseSteps)+2 is done.
\* failed: some request saw a committed version and found no transform.
VARIABLES pc, held, opc, saw, rpc, owed, inStore, upc, failed

\* Every variable, for stuttering steps.
vars == <<pc, held, opc, saw, rpc, owed, inStore, upc, failed>>

\* Each variable has the shape the story gives it.
TypeOK == /\ pc \in [Txns -> {"idle", "held", "committed", "aborted"}]
          /\ held \subseteq Txns
          /\ opc \in [Txns -> 0..Len(Steps)]
          /\ saw \in [Txns -> BOOLEAN]
          /\ rpc \in [Txns -> {"wait", "add", "clear", "done"}]
          /\ owed \in BOOLEAN
          /\ inStore \in BOOLEAN
          /\ upc \in 0..Len(UseSteps)+2
          /\ failed \in BOOLEAN

\* Nothing has happened yet.
Init == /\ pc = [t \in Txns |-> "idle"]
        /\ held = {}
        /\ opc = [t \in Txns |-> 0]
        /\ saw = [t \in Txns |-> FALSE]
        /\ rpc = [t \in Txns |-> "wait"]
        /\ owed = FALSE
        /\ inStore = FALSE
        /\ upc = 0
        /\ failed = FALSE

\* What letting go of t's ticket leaves held. "sharedHold" keys the hold by
\* the lens, so letting go drops every commit's hold of it.
LetGo(t) == IF Mode = "sharedHold" THEN {} ELSE held \ {t}

\* Just before the commit is attempted, t holds its compiled copy under its
\* own ticket. "noHold" skips this, as before the fix.
Hold(t) == /\ pc[t] = "idle"
           /\ pc' = [pc EXCEPT ![t] = "held"]
           \* Only "noHold" takes no hold.
           /\ held' = IF Mode = "noHold" THEN held ELSE held \cup {t}
           /\ UNCHANGED <<opc, saw, rpc, owed, inStore, upc, failed>>

\* The commit lands: readers of storage see the version from now on, and
\* t's ordered publication and registration are due.
Commit(t) == /\ pc[t] = "held"
             /\ pc' = [pc EXCEPT ![t] = "committed"]
             \* The ordered publication starts at its first step.
             /\ opc' = [opc EXCEPT ![t] = 1]
             \* The registration is due.
             /\ rpc' = [rpc EXCEPT ![t] = "add"]
             /\ UNCHANGED <<held, saw, owed, inStore, upc, failed>>

\* The commit fails (a conflict, a failed check) or the transaction is
\* dropped: t lets go of its ticket. "keepOnAbort" forgets to.
Abort(t) == /\ pc[t] \in {"idle", "held"}
            /\ pc' = [pc EXCEPT ![t] = "aborted"]
            \* Only "keepOnAbort" keeps the failed commit's hold.
            /\ held' = IF Mode = "keepOnAbort" THEN held ELSE LetGo(t)
            /\ UNCHANGED <<opc, saw, rpc, owed, inStore, upc, failed>>

\* One step of t's ordered publication, as Steps orders them.
Ordered(t) ==
  /\ opc[t] \in 1..(Len(Steps) - 1)
  /\ LET step == Steps[opc[t]] IN
       \* owe, first half: does the store hold the lens already?
       /\ IF step = "oweCheck"
             THEN /\ saw' = [saw EXCEPT ![t] = inStore]
                  /\ UNCHANGED <<held, owed>>
          \* owe, second half: a lens the store lacked is owed. Another
          \* task may have registered it in between, which leaves a stale
          \* owed entry that the next use clears.
          ELSE IF step = "oweInsert"
             THEN /\ owed' = (owed \/ ~saw[t])
                  /\ UNCHANGED <<held, saw>>
          \* let go of t's ticket.
          ELSE /\ held' = LetGo(t)
               /\ UNCHANGED <<saw, owed>>
  \* On to the next step.
  /\ opc' = [opc EXCEPT ![t] = opc[t] + 1]
  /\ UNCHANGED <<pc, rpc, inStore, upc, failed>>

\* t's follow-up registration, which may run before, during or after its
\* ordered publication. Safe: the lens enters the store, then the owed entry
\* is cleared. "clearFirst" clears the entry first.
Register(t) ==
  /\ rpc[t] \in {"add", "clear"}
  /\ IF (rpc[t] = "add") = (Mode # "clearFirst")
        \* The lens enters the store.
        THEN /\ inStore' = TRUE
             /\ UNCHANGED owed
        \* A registered lens is owed no longer.
        ELSE /\ owed' = FALSE
             /\ UNCHANGED inStore
  \* add is followed by clear, clear by done.
  /\ rpc' = [rpc EXCEPT ![t] = IF rpc[t] = "add" THEN "clear" ELSE "done"]
  /\ UNCHANGED <<pc, held, opc, saw, upc, failed>>

\* Some commit carrying the lens has landed, so a request can see its
\* version in storage (and, later, in the collection cache).
Visible == \E t \in Txns : pc[t] = "committed"

\* A request that sees the version starts using the lens.
UseStart ==
  /\ Visible /\ ~failed /\ upc = 0
  /\ upc' = 1
  /\ UNCHANGED <<pc, held, opc, saw, rpc, owed, inStore, failed>>

\* The next read in UseSteps. A held copy or an owed lens ends the use
\* successfully; neither found moves to the next read.
UseRead ==
  /\ upc \in 1..Len(UseSteps)
  /\ LET step == UseSteps[upc] IN
       IF step = "held"
         THEN /\ upc' = IF held # {} THEN Len(UseSteps)+2 ELSE upc+1
              /\ UNCHANGED <<inStore, owed, failed>>
         ELSE /\ upc' = IF owed THEN Len(UseSteps)+2 ELSE upc+1
              \* A use registers an owed lens, so it is in the store too.
              /\ inStore' = (inStore \/ owed)
              /\ owed' = IF owed THEN FALSE ELSE owed
              /\ UNCHANGED failed
  /\ UNCHANGED <<pc, held, opc, saw, rpc>>

\* The final read is the database's own store. Missing it is the issue #2917
\* failure; finding it completes the use.
UseStore ==
  /\ upc = Len(UseSteps)+1
  /\ upc' = Len(UseSteps)+2
  /\ failed' = ~inStore
  /\ UNCHANGED <<pc, held, opc, saw, rpc, owed, inStore>>

\* Any one of the steps may happen next.
Next == \/ \E t \in Txns : Hold(t) \/ Commit(t) \/ Abort(t) \/ Ordered(t)
                           \/ Register(t)
        \/ UseStart \/ UseRead \/ UseStore

\* The whole behaviour.
Spec == Init /\ [][Next]_vars

\* No request that sees the version ever fails to find its transform.
INV_NoMissingTransform == ~failed

\* While a version is visible, its lens is registered, owed or held.
INV_VisibleLensKnown == Visible => (inStore \/ owed \/ held # {})

\* A failed commit holds nothing.
INV_FailedCommitLetsGo == \A t \in Txns : pc[t] = "aborted" => t \notin held

\* The lens is owed only once some commit carrying it has landed.
INV_OwedOnlyCommitted == owed => Visible
=============================================================================
