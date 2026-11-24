------------------------ MODULE ObservationSettlement ------------------------
\* The story. A caller observes whether an actor holds a relation on a
\* document through the external gate. The gate asks the provider, then looks
\* whether a change of that tuple is still owed to the provider. Meanwhile the
\* node may stage a revocation of the tuple, and a settlement pass may send an
\* owed revocation to the provider and clear its record.
\*
\* What must never happen: the gate answers that the actor holds the relation
\* when a revocation of it was already owed when the provider was asked. The
\* provider's answer then predates the revocation, and a settlement pass that
\* ran between the question and the check would have cleared the record that
\* gave it away.
\*
\* The code: ExternalDocumentAcp::observe_actor_relationship in
\* crates/engine/acp/src/external.rs holds the settling lock, the one every
\* settlement pass holds, from the provider's answer to the check.
\* Only the standard operators on numbers are used.
EXTENDS Naturals

\* Bug lets a settlement pass run while an observation holds no lock.
CONSTANT Bug

\* held: the provider holds the relationship. owed: a revocation of it is
\* owed to the provider. phase: where the observation is. seen: what the
\* provider answered. owedAtRead: a revocation was owed when it answered.
\* locked: the observation holds the settling lock. answer: what the gate
\* answers.
VARIABLES held, owed, phase, seen, owedAtRead, locked, answer

\* Every variable, for stuttering steps.
vars == <<held, owed, phase, seen, owedAtRead, locked, answer>>

\* The relationship is held, nothing is owed, nothing is observed yet.
Init == /\ held = TRUE /\ owed = FALSE /\ phase = "idle" /\ seen = FALSE
        /\ owedAtRead = FALSE /\ locked = FALSE /\ answer = "none"

\* A transaction commits a revocation, owed to the provider.
Stage == /\ held /\ ~owed /\ owed' = TRUE
         /\ UNCHANGED <<held, phase, seen, owedAtRead, locked, answer>>

\* A settlement pass sends the owed revocation and clears its record. It
\* waits for the settling lock; the bug lets it run past an observation.
Settle == /\ owed /\ (~locked \/ Bug)
          /\ held' = FALSE /\ owed' = FALSE
          /\ UNCHANGED <<phase, seen, owedAtRead, locked, answer>>

\* The observation takes the settling lock and asks the provider.
Ask == /\ phase = "idle" /\ phase' = "asked" /\ locked' = TRUE
       /\ seen' = held /\ owedAtRead' = owed
       /\ UNCHANGED <<held, owed, answer>>

\* The observation checks what is owed, answers, and lets the lock go.
Check == /\ phase = "asked" /\ phase' = "done" /\ locked' = FALSE
         /\ answer' = IF owed THEN "unsettled" ELSE IF seen THEN "held" ELSE "absent"
         /\ UNCHANGED <<held, owed, seen, owedAtRead>>

\* Any of these, in any order.
Next == Stage \/ Settle \/ Ask \/ Check

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* The gate never answers "held" for a tuple whose revocation was owed when
\* the provider was asked.
INV_NoStaleHeld == answer = "held" => ~owedAtRead
=============================================================================
