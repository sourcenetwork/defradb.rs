------------------------- MODULE OwnerSelfObservation -------------------------
\* An authenticated HTTP caller asks whether it owns a document. Its bearer
\* and transaction identify the caller; the document must be visible through
\* the ordinary read path and the owner comes from that transaction's ACP.
\* The response reveals only whether the owner is this caller.
EXTENDS Naturals
CONSTANT Bug                           \* RED configs remove one admission guard.
VARIABLES answered, invalid             \* Whether a response was unsafe.
vars == <<answered, invalid>>
Init == /\ answered = FALSE            \* No request has answered yet.
        /\ invalid = FALSE             \* Nothing unsafe has been disclosed.

Observe(authenticated, sameTxn, bound, visible, selfOwner, subjectIsCaller) ==
  /\ ~answered                         \* Only one bounded request is needed.
  /\ LET allowed ==                    \* Each check applies to the bearer.
       (authenticated \/ Bug = "Auth") /\
       (sameTxn \/ Bug = "Owner") /\
       (bound \/ Bug = "Bound") /\
       (subjectIsCaller \/ Bug = "Subject")
     IN /\ answered' = allowed        \* A denied request has no observation.
        /\ invalid' =                 \* A true answer must establish every fact.
             (allowed /\
               (~authenticated \/ ~sameTxn \/ ~bound \/ ~subjectIsCaller \/
                ((visible \/ Bug = "Visible") /\ selfOwner /\ ~visible)))
Next == \E authenticated, sameTxn, bound, visible, selfOwner, subjectIsCaller \in BOOLEAN :
  Observe(authenticated, sameTxn, bound, visible, selfOwner, subjectIsCaller)
Spec == Init /\ [][Next]_vars          \* Explore every combination.
INV_NoFalseAuthority == ~invalid       \* No denied or hidden observation answers.
=============================================================================
