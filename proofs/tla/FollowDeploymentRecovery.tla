---- MODULE FollowDeploymentRecovery ----
\* The own-state recovery pass added to FollowDeployment: the node may rebuild
\* what an uncovered takeover voided, but it serves nothing until the pass has
\* handed every page over and the checked term is published.
EXTENDS FollowDeployment

CONSTANT
    Pages      \* how many pages the own-state pass takes
    DeliverPage \* TRUE: each page is handed to the outbox before the next

VARIABLES
    page       \* the page the pass is handing over
    pagesDone  \* how many pages have reached the outbox

vars2 == <<page, pagesDone>>

TypeOK2 ==
    /\ page \in 0..Pages
    /\ pagesDone \in 0..Pages
    /\ pagesDone <= page

Init2 ==
    /\ page = 0
    /\ pagesDone = 0

\* The node can rebuild the voided state only after every page reaches the
\* outbox and the checked term is published.
CanDecide ==
    /\ ~recoverable
    /\ page = Pages
    /\ pagesDone = Pages

RecoverPage ==
    /\ recoverable
    /\ page < Pages
    /\ page' = page + 1
    /\ UNCHANGED <<pagesDone, vars>>

PublishPage ==
    /\ DeliverPage
    /\ page > pagesDone
    /\ pagesDone' = page
    /\ UNCHANGED <<page, vars>>

FinishRecovery ==
    /\ recoverable
    /\ page = Pages
    /\ pagesDone = Pages
    /\ recoverable' = FALSE
    /\ handled' = term
    /\ UNCHANGED <<page, pagesDone, vars>>

Next2 == RecoverPage \/ PublishPage \/ FinishRecovery

PublishPage2 ==
    /\ ~DeliverPage
    /\ page' = Pages
    /\ UNCHANGED <<pagesDone, vars>>

Spec2 == Init /\ Init2 /\ [][Next \/ Next2 \/ PublishPage2]_(vars \o vars2)
        /\ WF_vars2(RecoverPage) /\ WF_vars2(PublishPage) /\ WF_vars2(FinishRecovery)

INV_NoDecisionBeforeHandover ==
    [](~CanDecide => cursor = 0)

LIVE_RecoveryHandsOver ==
    [](recoverable => <>((page = Pages /\ pagesDone = Pages) \/ ~recoverable))
====
