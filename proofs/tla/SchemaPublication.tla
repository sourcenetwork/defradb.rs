------------------------ MODULE SchemaPublication ------------------------
EXTENDS TLC
\* A schema commits while an older commit's hook holds back ordered events.
\* The schema operation replies only after successful cache publication.
\* Callback panics remain reported by the existing post-commit owner.
\* Models OnCommit::OrderedAndWait and the existing receipt-ordered dispatcher.
CONSTANT WaitForPublication
VARIABLES durable, earlierDone, visible, returned
\* Every flag belongs to this one schema operation or its earlier blocker.
vars == <<durable, earlierDone, visible, returned>>
\* No durable schema or completed callback exists initially.
Init == /\ durable = FALSE /\ earlierDone = FALSE
        /\ visible = FALSE /\ returned = FALSE
\* The storage commit can finish before the earlier hook releases events.
Commit == /\ ~durable /\ durable' = TRUE
          /\ UNCHANGED <<earlierDone, visible, returned>>
\* The older hook eventually lets the ordered dispatcher continue.
ReleaseEarlier == /\ ~earlierDone /\ earlierDone' = TRUE
                  /\ UNCHANGED <<durable, visible, returned>>
\* Only a durable schema whose predecessor finished can publish its cache.
Publish == /\ durable /\ earlierDone /\ ~visible /\ visible' = TRUE
           /\ UNCHANGED <<durable, earlierDone, returned>>
\* The completion signal prevents a reply between commit and publication.
\* The red configuration removes exactly this wait.
Reply == /\ durable /\ ~returned /\ (WaitForPublication => visible)
         /\ returned' = TRUE /\ UNCHANGED <<durable, earlierDone, visible>>
\* Each actor may make its next step; stuttering changes nothing.
Next == Commit \/ ReleaseEarlier \/ Publish \/ Reply
Spec == Init /\ [][Next]_vars
\* A successful schema operation is immediately visible to its next caller.
VisibleBeforeReply == returned => visible
=============================================================================
