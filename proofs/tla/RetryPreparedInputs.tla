---- MODULE RetryPreparedInputs ----
\* The story. A client sends one autocommit write. Some of what the write
\* needs comes from a provider outside the database: an ACP decision, a key,
\* or the embedder's write validator's approval. Before any transaction
\* starts, request preparation asks the provider and keeps the answer in the
\* request's memo, keyed by the exact request. Then attempts run: each is a
\* transaction that reads the answer from the memo. An attempt that finds no
\* answer for its exact request ends asking for it; the answer is fetched
\* outside the attempt and the next attempt runs. A storage conflict ends an
\* attempt too, and the next one reuses the memo.
\*
\* What must never happen: a provider is asked while a transaction is open; a
\* provider is asked the same question twice; a write commits without its
\* answer; an attempt is spent asking for an answer preparation already got.
\*
\* Code it models: crates/engine/query/src/runner/prepare.rs
\* prepare_mutations (preparation, which calls authorize_mutation with
\* strict_attempt = false); crates/engine/query/src/runner/mutation.rs
\* authorize_mutation (the strict attempt that ends in NeedsInput) and
\* check_doc_access_prepared; crates/engine/query/src/runner/executor.rs
\* resolve_wave and resolve_write_validation_input (the answer fetched
\* outside the attempt); crates/engine/query/src/prepared.rs
\* PreparedInputMemo (insert keeps the first answer).
EXTENDS Naturals
\* KeepMemoOnConflict: a conflict leaves the memo alone (the GREEN rule).
\* ResolveOutside: a provider is asked only while no attempt is open.
\* StorePreparedAnswer: preparation keeps the answer it got in the memo.
CONSTANT KeepMemoOnConflict, ResolveOutside, StorePreparedAnswer
\* Each switch is a plain yes or no.
ASSUME KeepMemoOnConflict \in BOOLEAN
\* The same for the second switch.
ASSUME ResolveOutside \in BOOLEAN
\* The same for the third switch.
ASSUME StorePreparedAnswer \in BOOLEAN
\* active: an attempt (a transaction) is open.
\* context: what the question depends on now (schema, heads); "c0" or "c1".
\* memoContext: which context the memo holds an answer for, or "none".
\* providerCalls: how many times the provider was asked.
\* networkInAttempt: provider calls made while an attempt was open.
\* commits, attempts: writes committed, attempts begun.
\* prepared: request preparation has run.
\* preparedContext: the context preparation asked about.
\* wastedAttempts: attempts that ended asking for the answer preparation got.
VARIABLES active, context, memoContext, providerCalls, networkInAttempt,
          commits, attempts, prepared, preparedContext, wastedAttempts
\* Every variable, so a step that changes nothing can be named.
vars == <<active, context, memoContext, providerCalls, networkInAttempt,
          commits, attempts, prepared, preparedContext, wastedAttempts>>
\* At the start nothing has run and the memo is empty.
Init == /\ active = FALSE /\ context = "c0" /\ memoContext = "none"
        /\ providerCalls = 0 /\ networkInAttempt = 0 /\ commits = 0 /\ attempts = 0
        /\ prepared = FALSE /\ preparedContext = "none" /\ wastedAttempts = 0
\* Preparation runs once, before any attempt and outside any transaction.
\* It asks the provider; under the GREEN rule it keeps the answer in the memo,
\* under the planted bug it drops it (the approval checked and thrown away).
Prepare == /\ ~prepared /\ ~active /\ attempts = 0
           /\ prepared' = TRUE /\ preparedContext' = context
           /\ providerCalls' = providerCalls + 1
           /\ memoContext' = IF StorePreparedAnswer THEN context ELSE memoContext
           /\ UNCHANGED <<active, context, networkInAttempt, commits, attempts, wastedAttempts>>
\* An attempt begins only after preparation, and only a few times in this
\* bounded run.
Begin == /\ prepared /\ ~active /\ attempts < 3 /\ active' = TRUE /\ attempts' = attempts + 1
         /\ UNCHANGED <<context, memoContext, providerCalls, networkInAttempt, commits,
                        prepared, preparedContext, wastedAttempts>>
\* An attempt that finds no answer for its exact question ends asking for it.
\* If the question is still the one preparation asked, that attempt was wasted.
NeedInput == /\ active /\ memoContext # context /\ active' = FALSE
             /\ wastedAttempts' = wastedAttempts + IF context = preparedContext THEN 1 ELSE 0
             /\ UNCHANGED <<context, memoContext, providerCalls, networkInAttempt, commits,
                            attempts, prepared, preparedContext>>
\* The provider is asked for an answer the memo lacks. The GREEN rule allows
\* this only with no attempt open; a call made inside one is counted.
Resolve == /\ prepared /\ memoContext # context
           /\ providerCalls < 3 /\ (ResolveOutside => ~active)
           /\ memoContext' = context /\ providerCalls' = providerCalls + 1
           /\ networkInAttempt' = networkInAttempt + IF active THEN 1 ELSE 0
           /\ UNCHANGED <<active, context, commits, attempts, prepared, preparedContext,
                          wastedAttempts>>
\* An attempt commits only when the memo answers its exact question.
Commit == /\ active /\ memoContext = context /\ commits < 2 /\ active' = FALSE /\ commits' = commits + 1
          /\ UNCHANGED <<context, memoContext, providerCalls, networkInAttempt, attempts,
                         prepared, preparedContext, wastedAttempts>>
\* A storage conflict ends the attempt; only the GREEN rule keeps the memo.
Conflict == /\ active /\ active' = FALSE
            /\ memoContext' = IF KeepMemoOnConflict THEN memoContext ELSE "none"
            /\ UNCHANGED <<context, providerCalls, networkInAttempt, commits, attempts,
                           prepared, preparedContext, wastedAttempts>>
\* A schema, head or identity change between attempts changes the exact
\* question, so the old answer no longer matches it.
ChangeContext == /\ ~active /\ context = "c0" /\ context' = "c1"
                /\ UNCHANGED <<active, memoContext, providerCalls, networkInAttempt, commits,
                               attempts, prepared, preparedContext, wastedAttempts>>
\* Any of the steps above may happen next.
Next == Prepare \/ Begin \/ NeedInput \/ Resolve \/ Commit \/ Conflict \/ ChangeContext
\* The whole behaviour: start in Init, then take steps.
Spec == Init /\ [][Next]_vars
\* Every variable holds a value of the kind it should.
TypeOK == /\ active \in BOOLEAN /\ context \in {"c0", "c1"}
          /\ memoContext \in {"none", "c0", "c1"} /\ providerCalls \in Nat
          /\ networkInAttempt \in Nat
          /\ commits \in Nat /\ attempts \in Nat
          /\ prepared \in BOOLEAN /\ preparedContext \in {"none", "c0", "c1"}
          /\ wastedAttempts \in Nat
\* The provider answers each question once: once while the context is the
\* first one, and once more at most after it changed.
INV_ProviderOncePerContext == providerCalls <= IF context = "c0" THEN 1 ELSE 2
\* No provider is ever asked while a transaction attempt is open.
INV_NoProviderInsideAttempt == networkInAttempt = 0
\* A write commits only through an attempt.
INV_CommitHasExactPreparedInput == commits <= attempts
\* No attempt ends asking for the answer preparation already got.
INV_PreparedAnswerReused == wastedAttempts = 0
====
