---- MODULE NacDacBypass ----
\* The story, in plain words.
\*
\* A caller reads documents that document access control (DAC) protects.
\* Node access control (NAC) can grant a caller `bypass-dac`; the node owner
\* holds it. Go lets such a caller read every document, but only while NAC is
\* on: with NAC off every NAC permission reads as held, and that must not
\* open DAC to everyone.
\*
\* A read takes one snapshot and reads its rows there. While it runs, NAC can
\* be turned off or back on and the grant can be revoked or given back.
\* defradb.rs decides the bypass in the request's own transaction, at that
\* same snapshot, so the bypass and the rows always agree.
\*
\* A caller can also deny its own request the bypass: an embedder that signs
\* a read as an identity holding `bypass-dac` (a node administrator) says the
\* read must still go through DAC. Such a read never bypasses, whatever NAC
\* says.
\*
\* What must never happen:
\*   1. A read bypasses DAC when, at its snapshot, NAC was off or the caller
\*      did not hold `bypass-dac`, or when its caller denied it the bypass.
\*      (INV_BypassOnlyWhenGranted)
\*   2. A read does not bypass DAC when, at its snapshot, NAC was on and the
\*      caller held `bypass-dac` and did not deny it: Go's answer for the
\*      node owner. (INV_BypassWhenGranted)
\*   3. A read whose caller denied it the bypass bypasses DAC.
\*      (INV_DeniedNeverBypasses)
\*
\* The Rust code this models:
\*   - the decision, in the request's transaction  crates/query/src/runner/turn.rs QueryRunner::lent
\*   - the caller's denial, kept by every copy      crates/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied, RequestContext::with_dac_bypass
\*   - NAC on and the grant, read in that view      crates/db/src/read/lensed/fetcher/mod.rs LensedDocFetcher::dac_bypass
\*   - where the bypass is honoured                 crates/db/src/collection/acp.rs check_doc_permission_as
EXTENDS Naturals

CONSTANT Bug   \* "None" for the real code; otherwise the guard a RED config breaks

VARIABLES
  nacOn,     \* NAC is on now
  held,      \* the caller holds `bypass-dac` now
  snap,      \* the running read's <<nacOn, held, denied>> at its snapshot, or <<>> for none
  changes,   \* how many changes happened, to keep the model finite
  bad,       \* TRUE once a read bypassed without a grant at its snapshot
  lost,      \* TRUE once a read did not bypass with a grant at its snapshot
  overrode   \* TRUE once a read its caller denied the bypass bypassed

vars == <<nacOn, held, snap, changes, bad, lost, overrode>>

\* The decision the code makes from `on` (NAC on), `has` (grant held) and
\* `no` (the caller denied the bypass). RED NacOff decides as a bare
\* permission check does, where NAC off reads as every permission held.
\* RED Never is the code before #2681: no bypass. RED IgnoreDenial is the code
\* before the denial: it asks NAC whatever the caller said.
Decide(on, has, no) ==
  CASE Bug = "NacOff"       -> (~no /\ (~on \/ has))
    [] Bug = "Never"        -> FALSE
    [] Bug = "IgnoreDenial" -> (on /\ has)
    [] OTHER                -> (~no /\ on /\ has)

TypeOK ==
  /\ nacOn \in BOOLEAN                                             \* on or off
  /\ held \in BOOLEAN                                              \* held or not
  /\ snap \in {<<>>} \cup (BOOLEAN \X BOOLEAN \X BOOLEAN)         \* no read, or its snapshot and denial
  /\ changes \in 0..2                                              \* at most two changes
  /\ bad \in BOOLEAN                                               \* the wrong-bypass flag
  /\ lost \in BOOLEAN                                              \* the missed-bypass flag
  /\ overrode \in BOOLEAN                                          \* the overridden-denial flag

Init ==
  /\ nacOn \in BOOLEAN                                             \* NAC starts either way
  /\ held \in BOOLEAN                                              \* the grant starts either way
  /\ snap = <<>>                                                   \* no read running
  /\ changes = 0                                                  \* nothing changed yet
  /\ bad = FALSE                                                   \* nothing wrong yet
  /\ lost = FALSE                                                  \* nothing missed yet
  /\ overrode = FALSE                                               \* no denial overridden yet

\* A read begins: it takes its snapshot, and its caller either denies it the
\* bypass or leaves the decision to NAC.
Begin ==
  /\ snap = <<>>                                                   \* one read at a time
  /\ \E no \in BOOLEAN :                                            \* the caller's choice
       snap' = <<nacOn, held, no>>                                  \* what it will see, and the denial
  /\ UNCHANGED <<nacOn, held, changes, bad, lost, overrode>>

\* The read decides the bypass and returns its rows. The code reads NAC and
\* the grant in the read's own transaction, so it sees the snapshot; RED Live
\* reads them as they are now, outside it.
Finish ==
  /\ snap # <<>>                                                   \* a read is running
  /\ LET on   == IF Bug = "Live" THEN nacOn ELSE snap[1]           \* NAC as the decision sees it
         has  == IF Bug = "Live" THEN held ELSE snap[2]            \* the grant as the decision sees it
         no   == snap[3]                                           \* the caller denied the bypass
         by   == Decide(on, has, no)                               \* the decision
         ok   == ~no /\ snap[1] /\ snap[2]                         \* the answer at the rows' snapshot
     IN /\ bad' = (bad \/ (by /\ ~ok))                             \* bypassed without a grant
        /\ lost' = (lost \/ (ok /\ ~by))                           \* missed a grant
        /\ overrode' = (overrode \/ (no /\ by))                    \* bypassed a denial
  /\ snap' = <<>>                                                  \* the read is over
  /\ UNCHANGED <<nacOn, held, changes>>

\* NAC is turned off or back on.
Toggle ==
  /\ changes < 2                                                   \* keep the model finite
  /\ nacOn' = ~nacOn                                               \* flip it
  /\ changes' = changes + 1                                        \* count it
  /\ UNCHANGED <<held, snap, bad, lost, overrode>>

\* The grant is revoked or given back.
Grant ==
  /\ changes < 2                                                   \* keep the model finite
  /\ held' = ~held                                                 \* flip it
  /\ changes' = changes + 1                                        \* count it
  /\ UNCHANGED <<nacOn, snap, bad, lost, overrode>>

Next == Begin \/ Finish \/ Toggle \/ Grant

Spec == Init /\ [][Next]_vars

\* Promise 1: no bypass without NAC on and the grant, at the read's snapshot.
INV_BypassOnlyWhenGranted == ~bad

\* Promise 2: a bypass whenever NAC was on and the grant held, at that
\* snapshot, and the caller did not deny it.
INV_BypassWhenGranted == ~lost

\* Promise 3: a read its caller denied the bypass never bypasses.
INV_DeniedNeverBypasses == ~overrode
====
