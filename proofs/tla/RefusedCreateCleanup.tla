---- MODULE RefusedCreateCleanup ----
\* A creator reserves a document ID and a unique value. If Vera refuses,
\* the node releases both reservations and consumes the refusal together.
\* Correspondence: ExternalDocumentAcp::note_failure in external/registration.rs
\* and DB::purge_refused_create in access/refused_create.rs.
EXTENDS Naturals
\* Split omits the index delete. Replay and WrongOwner omit separate checks.
CONSTANTS Split, Replay, WrongOwner
\* The row and index name their owner, debt names the refused row, and
\* replaced remembers whether a second create claimed the document ID.
VARIABLES row, idx, debt, replaced
\* Begin just after the first local gated create has committed.
Init == /\ row = 1 /\ idx = 1 /\ debt = 0 /\ replaced = FALSE
\* Vera refuses the first create, and the node records its obligation.
Refuse == /\ row = 1 /\ debt = 0 /\ debt' = 1 /\ UNCHANGED <<row, idx, replaced>>
\* A cleanup usually requires the stored refusal. Replay allows an old copy.
Purge == /\ (debt = 1 \/ (Replay /\ replaced))
         \* The stored short ID determines whether the current row goes.
         /\ LET remove == row = 1 \/ WrongOwner \/ Replay
            IN /\ row' = IF remove THEN 0 ELSE row
               \* When a row goes, its unique value must go with it.
               /\ idx' = IF remove /\ ~Split THEN 0 ELSE idx
         \* The refusal is consumed, including when its row was replaced.
         /\ debt' = 0 /\ UNCHANGED replaced
\* A later create can claim both reservations once they are free.
Replace == /\ row = 0 /\ idx = 0
           /\ row' = 2 /\ idx' = 2 /\ replaced' = TRUE /\ UNCHANGED debt
\* An independent purge and recreate can leave an old refusal for the same
\* public ID, for instance when the new create uses another policy.
ReplaceBeforeCleanup == /\ row = 1 /\ debt = 1
                        /\ row' = 2 /\ idx' = 2 /\ replaced' = TRUE /\ UNCHANGED debt
\* A crash keeps precisely the committed state.
Crash == UNCHANGED <<row, idx, debt, replaced>>
\* A provider, creator, cleanup task, or crash can take the next step.
Next == Refuse \/ Purge \/ Replace \/ ReplaceBeforeCleanup \/ Crash
\* The durable row and the unique reservation always agree.
ReservationsAgree == row = idx
\* A later create always retains its reservations.
ReplacementSafe == replaced => row = 2
\* A durable refusal is eventually processed when cleanup can keep running.
EventuallyReleased == (debt = 1) ~> (debt = 0)
\* The cleanup task eventually runs when it remains enabled.
Spec == Init /\ [][Next]_<<row, idx, debt, replaced>> /\ WF_<<row, idx, debt, replaced>>(Purge)
====
