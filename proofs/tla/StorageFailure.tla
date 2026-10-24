------------------------ MODULE StorageFailure ------------------------
EXTENDS Naturals, TLC
\* An engine failure passes through storage, database and wire adapters.
\* They preserve its cause and never turn an uncertain commit into replay
\* permission. Models CoreKV, db error adapters and query/HTTP conversions.
CONSTANTS PreserveCause, TypedOnly, AllowRetry
\* The original cause stays fixed while its observations cross adapters.
VARIABLES original, cause, stage, code, retry
\* All facts in this bounded error-translation pipeline.
vars == <<original, cause, stage, code, retry>>
\* Other and opaque messages deliberately look like a disk-full error.
Init == /\ original \in {"full", "other", "opaque"}
        /\ cause = original /\ stage = 0 /\ code = "none" /\ retry = FALSE
\* Each native adapter carries typed facts rather than stringifying them.
\* The lost-cause red configuration replaces them with opaque text.
Adapt == /\ stage < 2 /\ stage' = stage + 1
         /\ cause' = IF PreserveCause THEN cause ELSE "opaque"
         /\ UNCHANGED <<original, code, retry>>
\* Only the typed full cause produces a code; text cannot supply authority.
\* The other red configurations classify text or authorize replay.
Wire == /\ stage = 2 /\ stage' = 3
        /\ code' = IF cause = "full" \/ ~TypedOnly THEN "STORAGE_FULL" ELSE "none"
        /\ retry' = AllowRetry /\ UNCHANGED <<original, cause>>
\* The adapters execute in order, with arbitrary pauses at each boundary.
Next == Adapt \/ Wire
\* Stuttering lets an adapter wait without losing the error it carries.
Spec == Init /\ [][Next]_vars
\* A returned code identifies exactly the engine's typed full cause.
ClassificationPreserved == stage = 3 =>
    (code = "STORAGE_FULL") = (original = "full")
\* An error never proves that replaying a transaction is safe.
NoReplayAuthority == ~retry
=============================================================================
