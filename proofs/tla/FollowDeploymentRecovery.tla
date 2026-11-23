---- MODULE FollowDeploymentRecovery ----
\* The own-state pass added to FollowDeployment.
\*
\* An uncovered takeover makes the node recoverable. The source walks its
\* ledger one bounded page at a time, and each page is handed to the outbox.
\* `recoverCursor` counts the pages handed over. A page source reports the
\* ledger's end only on the final page, and the node publishes the checked
\* term only after the source has said the pass is done.
EXTENDS FollowDeployment

CONSTANTS
    Pages,                \* how many bounded pages the ledger takes
    DeliverPage,          \* TRUE: each page reaches the outbox before the next
    PageEndsAtBoundary,   \* TRUE: only the final page reports the ledger's end
    SourceCompletion      \* TRUE: the source reports that the whole pass is done

\* Every base variable except the term and recovery state recovery may change.
BaseIdle == <<staged, owed, pending, sent, chain, journal, evicted,
              failClosed, mark, cursor, checking, lost, tainted, crashes,
              cache, takeovers, servedVoid, servedWide>>

\* Recovery has produced or is producing pages.
RecoveryOpen == recoverable \/ recoverDone \/ recoverCursor > 0

\* A recovery pass may close only with every page handed over and done.
INV_NoDecisionBeforeHandover ==
    ~(RecoveryOpen /\ ~recoverable /\ handled = term
      /\ (recoverCursor < Pages \/ ~recoverDone))

\* The source returns a page and hands it to the outbox before the next.
SourcePage ==
    /\ recoverable                    \* the pass is active
    /\ recoverCursor < Pages          \* another ledger page remains
    /\ recoverCursor' = recoverCursor + 1  \* this page reaches the outbox
    /\ recoverDone' = IF recoverCursor + 1 = Pages
                      THEN SourceCompletion /\ PageEndsAtBoundary  \* the final page says done
                      ELSE SourceCompletion /\ ~PageEndsAtBoundary \* the bug says done early
    /\ UNCHANGED <<BaseIdle, term, handled, recoverable>>

\* The whole recovery pass has been handed over and done.
FinishRecovery ==
    /\ recoverable                    \* the pass is still active
    /\ recoverDone                    \* the source says every page is done
    /\ recoverable' = FALSE           \* recovery ends
    /\ handled' = term                \* the term is published
    /\ UNCHANGED <<BaseIdle, term, recoverCursor, recoverDone>>

\* The no-page-gate bug publishes a checked term without handing over.
SkipHandover ==
    /\ ~DeliverPage                   \* the page gate is absent
    /\ recoverable                    \* a pass is active
    /\ recoverCursor > 0              \* at least one page was read
    /\ recoverable' = FALSE           \* the bug ends recovery anyway
    /\ handled' = term                \* and publishes the term
    /\ UNCHANGED <<BaseIdle, term, recoverCursor, recoverDone>>

\* The base actions leave the recovery page fields alone.
BaseNext2 == Next /\ UNCHANGED <<recoverCursor, recoverDone>>  \* base work leaves recovery alone

Next2 == SourcePage \/ FinishRecovery \/ SkipHandover  \* the recovery pass itself

NextAll == BaseNext2 \/ Next2  \* one step is base work or recovery work

Spec2 == Init /\ [][NextAll]_vars
        /\ WF_vars(SourcePage)      \* the source keeps returning pages
        /\ WF_vars(FinishRecovery)  \* a done pass is closed and published

LIVE_RecoveryFinishes ==
    [](recoverable => <>(~recoverable /\ recoverDone /\ recoverCursor = Pages))  \* recovery must end
====
