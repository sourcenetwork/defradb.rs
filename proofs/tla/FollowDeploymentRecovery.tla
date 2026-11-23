---- MODULE FollowDeploymentRecovery ----
\* The story, in plain words.
\*
\* A DefraDB node keeps its access rules on a Vera deployment. Another device
\* of the account can take the deployment over, and what the old device
\* confirmed above the takeover's cut is gone. FollowDeployment.tla covers the
\* takeover whose loss the node's journal can name. This model covers the
\* other one: the journal is too short to say what was lost, so the node
\* closes ("fails closed": it decides nothing) and rebuilds from its own
\* records instead.
\*
\* The node keeps a ledger: one record for every registration, grant and
\* revocation the deployment confirmed to it. A recovery pass walks that
\* ledger in pages. For each page it asks the deployment, under the new term,
\* which records it still holds, and hands the ones it lost to the outbox to
\* be sent again. Only then does it write down how far it got (the cursor).
\* The ledger says when the last record has been handed over; only then does
\* the node reopen and decide again. The outbox then sends what it was handed.
\*
\* The pass can stop at any moment (a crash): the page in memory is gone, the
\* cursor, the outbox and the ledger stay. Another takeover can happen while
\* the pass runs: the cursor names the term it was written under, and a
\* cursor of an older term is not resumed, the pass starts again.
\*
\* What must never happen:
\*   * the node decides while a record the deployment lost is not owed;
\*   * a revocation the node committed stops denying: a decision is served
\*     while the deployment still grants and no owed revocation denies.
\* What must happen: the pass ends, the node reopens, and everything the
\* deployment lost is sent again.
\*
\* Code this models (defradb.rs):
\*   crates/engine/vera/src/vera_rs/provider/terms.rs   Follower::check_with, Follower::recover
\*   crates/engine/vera/src/vera_rs/follow/journal.rs   Journal::close_recoverable,
\*       Journal::recovery_needed, Journal::recovered
\*   crates/engine/vera/src/vera_rs/follow/mod.rs       Follow::admit, reconcile
\*   crates/engine/acp/src/recovery.rs                  RecoveryLedger::page, RecoveryCursor,
\*       RecoverySource, RecoveryStep
\*   crates/engine/acp/src/external/recovery.rs         RecoverySource::next and ::delivered
\*       for ExternalDocumentAcp
\*   crates/engine/acp/src/external/voided.rs           OweAgain::owe_again, OweAgain::settle_owed
\*
\* Correspondence table (model -> Rust):
\*   Records              the slots RecoveryLedger::page walks, in key order
\*   lost                 what the verifier finds: an owner or relationship the deployment
\*                        no longer holds as the ledger records it (RecoveryVerifier)
\*   TakeOver, recoverable  reconcile closing with Journal::close_recoverable;
\*                        Journal::recovery_needed
\*   Read, page           RecoverySource::next: one RecoveryPage of at most
\*                        RECOVERY_PAGE_BYTES, with the effects of what it found lost
\*   TermFence            next resumes a cursor only when RecoveryCursor::term is the
\*                        verifier's term
\*   EndFromLedger        RecoveryCursor::complete is LedgerPage::end
\*   Owe, HandOver        Follower::recover calling OweAgain::owe_again with the page
\*   Record, OweFirst     Follower::recover calling RecoverySource::delivered after it
\*   cursor, cursorTerm, complete   the RecoveryCursor stored by delivered
\*   Done, SaysDone       next returning RecoveryStep::Done with the term of a complete
\*                        cursor, and Follower::recover then calling Journal::recovered
\*                        with that term
\*   Gate, Open           Follow::admit: refuse while the journal is closed or a term
\*                        is unchecked
\*   Crash                the node stops: Follower::recover's page is dropped
\*   Settle               OweAgain::settle_owed and the drainer sending what is owed
\*   Decide               an access decision through ExternalDocumentAcp, where an owed
\*                        revocation denies
EXTENDS Naturals, FiniteSets

CONSTANTS
    Records,       \* how many records the node's ledger holds
    Revoked,       \* which of them are revocations (the rest are grants and registrations)
    PageSize,      \* how many records one page reads
    MaxTakeovers,  \* how many takeovers happen in a run (keeps the model finite)
    MaxCrashes,    \* how many times the node stops part way through a pass
    Gate,          \* TRUE: nothing is decided while the journal is closed
    HandOver,      \* TRUE: a page's lost records are handed to the outbox
    OweFirst,      \* TRUE: the cursor is written only after the page is handed over
    EndFromLedger, \* TRUE: the pass is complete only when a page reached the ledger's end
    SaysDone,      \* TRUE: the source says so once the pass is complete
    TermFence      \* TRUE: a cursor written under another term is not resumed

VARIABLES
    term,        \* the deployment's term, as the light client certified it
    handled,     \* the term the node last checked
    recoverable, \* TRUE while the journal is closed and waits for a pass
    lost,        \* the records the deployment does not hold as the ledger says
    owed,        \* the records the outbox owes the deployment
    cursor,      \* how many records the pass has handed over; durable
    cursorTerm,  \* the term the cursor was written under; durable
    complete,    \* TRUE once the cursor says the pass is over; durable
    page,        \* the page in memory: none, or what it read and whether it was handed over
    takeovers,   \* how many takeovers happened
    crashes,     \* how many times the node stopped
    servedWide   \* TRUE once a decision allowed what a committed revocation denies

\* Every variable, for "nothing else moves".
vars == <<term, handled, recoverable, lost, owed, cursor, cursorTerm, complete, page,
          takeovers, crashes, servedWide>>

\* Every record of the ledger, by its place in key order.
All == 1..Records

\* The value of `page` when no page is in memory.
None == [read |-> FALSE]

\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ term \in 0..MaxTakeovers /\ handled \in 0..MaxTakeovers  \* terms
    /\ cursorTerm \in 0..MaxTakeovers                           \* a term
    /\ recoverable \in BOOLEAN /\ complete \in BOOLEAN          \* flags
    /\ lost \subseteq All /\ owed \subseteq All                 \* sets of records
    /\ cursor \in 0..Records                                    \* a count of records
    /\ page.read \in BOOLEAN                                    \* a page or none
    /\ takeovers \in 0..MaxTakeovers /\ crashes \in 0..MaxCrashes  \* counts
    /\ servedWide \in BOOLEAN                                   \* a history flag

\* At the start: one term, checked; the deployment holds every record.
Init ==
    /\ term = 0 /\ handled = 0                  \* the first term, checked
    /\ recoverable = FALSE                      \* the journal is open
    /\ lost = {} /\ owed = {}                   \* nothing lost, nothing owed
    /\ cursor = 0 /\ cursorTerm = 0             \* no pass has run
    /\ complete = FALSE                         \* so none is complete
    /\ page = None                              \* no page in memory
    /\ takeovers = 0 /\ crashes = 0             \* nothing happened yet
    /\ servedWide = FALSE                       \* nothing served yet

\* Whether the node may decide and send now.
Open ==
    /\ ~recoverable                             \* the journal is not closed
    /\ handled = term                           \* and the term is checked

\* Another device takes over, and the journal cannot say what it voided: the
\* deployment may have lost any of the records. The node closes its journal
\* as recoverable.
TakeOver ==
    /\ takeovers < MaxTakeovers                 \* the run allows one more
    /\ takeovers' = takeovers + 1               \* count it
    /\ term' = term + 1                         \* a new term
    /\ lost' \in SUBSET All                     \* any records may be gone now
    /\ recoverable' = TRUE                      \* closed until a pass rebuilds
    /\ UNCHANGED <<handled, owed, cursor, cursorTerm, complete, page, crashes,
                   servedWide>>                 \* the pass has not looked yet

\* Whether the stored cursor belongs to the pass for this term.
Resumes == cursorTerm = term \/ ~TermFence

\* The pass reads its next page: the records after the cursor, at most
\* PageSize of them, and which of those the deployment lost. A cursor of
\* another term is not resumed: the pass starts at the first record.
Read ==
    /\ recoverable /\ ~page.read                \* a pass is needed, no page in memory
    /\ ~(Resumes /\ complete)                   \* and the pass is not over
    /\ LET from == IF Resumes THEN cursor ELSE 0       \* where this page starts
           to == Min(from + PageSize, Records)         \* where it ends
       IN page' = [read |-> TRUE,                      \* a page is in memory
                   term |-> term,                      \* read under this term
                   to |-> to,                          \* its last record
                   found |-> lost \cap ((from + 1)..to),  \* what it found lost
                   handed |-> FALSE]                   \* not handed over yet
    /\ UNCHANGED <<term, handled, recoverable, lost, owed, cursor, cursorTerm, complete,
                   takeovers, crashes, servedWide>>    \* nothing else moves

\* The page's lost records are handed to the outbox, which owes them now.
Owe ==
    /\ page.read /\ ~page.handed                \* a page not handed over yet
    /\ owed' = IF HandOver THEN owed \cup page.found ELSE owed  \* owed again
    /\ page' = [page EXCEPT !.handed = TRUE]    \* handed over
    /\ UNCHANGED <<term, handled, recoverable, lost, cursor, cursorTerm, complete,
                   takeovers, crashes, servedWide>>    \* nothing else moves

\* The node writes down how far the pass got, under the term the page was
\* read. The pass is complete when the page reached the ledger's last record.
Record ==
    /\ page.read                                \* a page is in memory
    /\ page.handed \/ ~OweFirst                 \* handed over first
    /\ cursor' = page.to                        \* the pass got this far
    /\ cursorTerm' = page.term                  \* under this term
    /\ complete' = (page.to = Records \/ ~EndFromLedger)  \* the ledger's end, or not yet
    /\ page' = None                             \* the page is done with
    /\ UNCHANGED <<term, handled, recoverable, lost, owed, takeovers, crashes,
                   servedWide>>                 \* nothing else moves

\* The source says the pass for this term is over: the node records the term
\* as checked and reopens.
Done ==
    /\ SaysDone                                 \* the source can say so
    /\ recoverable /\ ~page.read                \* a pass was needed, no page in memory
    /\ Resumes /\ complete                      \* and this term's pass is complete
    /\ recoverable' = FALSE                     \* the journal reopens
    /\ handled' = term                          \* the term is checked
    /\ UNCHANGED <<term, lost, owed, cursor, cursorTerm, complete, page, takeovers,
                   crashes, servedWide>>        \* nothing else moves

\* A source that never says it is over hands back one more empty page.
Again ==
    /\ ~SaysDone                                \* the source cannot say it is over
    /\ recoverable /\ ~page.read                \* a pass is needed, no page in memory
    /\ Resumes /\ complete                      \* though this term's pass is complete
    /\ page' = [read |-> TRUE, term |-> term, to |-> cursor,
                found |-> {}, handed |-> FALSE] \* an empty page
    /\ UNCHANGED <<term, handled, recoverable, lost, owed, cursor, cursorTerm, complete,
                   takeovers, crashes, servedWide>>    \* nothing else moves

\* The node stops part way: the page in memory is gone, the rest is on disk.
Crash ==
    /\ crashes < MaxCrashes                     \* the run allows one more
    /\ page.read                                \* a page is in memory
    /\ crashes' = crashes + 1                   \* count it
    /\ page' = None                             \* the page is gone
    /\ UNCHANGED <<term, handled, recoverable, lost, owed, cursor, cursorTerm, complete,
                   takeovers, servedWide>>      \* nothing else moves

\* The outbox sends one owed record and the deployment confirms it.
Settle ==
    /\ Open                                     \* the node sends only when open
    /\ \E r \in owed :                          \* some owed record
         /\ owed' = owed \ {r}                  \* is owed no more
         /\ lost' = lost \ {r}                  \* and the deployment holds it again
    /\ UNCHANGED <<term, handled, recoverable, cursor, cursorTerm, complete, page,
                   takeovers, crashes, servedWide>>    \* nothing else moves

\* The node decides. A revocation the deployment lost still denies while it
\* is owed; one that is lost and not owed lets the deployment's grant through.
Decide ==
    /\ Open \/ ~Gate                            \* the node may decide
    /\ servedWide' = (servedWide \/ ((lost \cap Revoked) \ owed # {}))  \* wider than intended?
    /\ UNCHANGED <<term, handled, recoverable, lost, owed, cursor, cursorTerm, complete,
                   page, takeovers, crashes>>   \* nothing else moves

\* One of these happens at each step.
Next == TakeOver \/ Read \/ Owe \/ Record \/ Done \/ Again \/ Crash \/ Settle \/ Decide

\* The pass keeps reading, handing over, recording and finishing, and the
\* outbox keeps sending.
Spec == Init /\ [][Next]_vars
        /\ WF_vars(Read) /\ WF_vars(Owe) /\ WF_vars(Record) /\ WF_vars(Done)
        /\ WF_vars(Again) /\ WF_vars(Settle)

\* The node never decides while a record the deployment lost is not owed.
INV_NothingLost == Open => lost \subseteq owed

\* A revocation the node committed denies throughout: no decision is wider.
INV_RevocationDenies == ~servedWide

\* The pass ends: the node reopens and everything lost is sent again.
LIVE_RecoveryEnds == <>[](Open /\ lost = {} /\ owed = {})
====
