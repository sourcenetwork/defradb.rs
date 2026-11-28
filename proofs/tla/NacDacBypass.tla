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
\* A request reaches documents along one of several paths: a query, a
\* commits query, an aggregate, a subscription's event query, a version
\* read, an explain, or an update or delete that first decides which
\* documents it may change. The bypass the request's transaction decides
\* must reach every one of them, as Go's `canDACBypass` sits in front of
\* every document check.
\*
\* A write is prepared before its attempts run: preparation picks the
\* documents the write may change, and only those, outside any transaction.
\* It picks them under the bypass NAC grants at that moment, so a grant or a
\* revoke can land between preparation and the attempt. Each attempt decides
\* the bypass again in its own transaction; when it decides the other one,
\* the attempt ends and the write is prepared again under the attempt's
\* decision, so no attempt changes documents picked under a stale bypass.
\*
\* The bypass belongs to the identity that made the request. A request with
\* no identity (an anonymous one) never bypasses, even on a node whose
\* default identity, the one document access control acts as for an
\* anonymous request, holds `bypass-dac` (as the NAC owner does).
\*
\* A caller can also deny its own request the bypass: an embedder that signs
\* a read as an identity holding `bypass-dac` (a node administrator) says the
\* read must still go through DAC. Such a read never bypasses, whatever NAC
\* says.
\*
\* What must never happen:
\*   1. A read bypasses DAC when, at its snapshot, NAC was off or the caller
\*      did not hold `bypass-dac`, or when its caller denied it the bypass,
\*      or when it has no caller. (INV_BypassOnlyWhenGranted)
\*   2. A read does not bypass DAC when, at its snapshot, NAC was on and the
\*      caller held `bypass-dac` and did not deny it: Go's answer for the
\*      node owner. (INV_BypassWhenGranted)
\*   3. A read whose caller denied it the bypass bypasses DAC.
\*      (INV_DeniedNeverBypasses)
\*
\* The Rust code this models:
\*   - the decision, in the request's transaction  crates/engine/query/src/runner/dac_bypass.rs QueryRunner::with_decided_dac_bypass,
\*     called by a transaction's turn and by        crates/engine/query/src/runner/turn.rs QueryRunner::lent,
\*     each attempt of an autocommit write          crates/engine/query/src/runner/mutation.rs QueryRunner::execute_parsed_mutations
\*   - a write's preparation, which picks the       crates/engine/query/src/runner/prepare.rs QueryRunner::prepare_mutations,
\*     documents under the bypass it records         crates/database/db/src/read/lensed/autocommit/mod.rs LensedAutoCommitFetcher::dac_bypass,
\*                                                   crates/engine/query/src/prepared.rs PreparedDacBypass
\*   - an attempt that decided the other bypass     crates/engine/query/src/prepared.rs PreparedDacBypass::stale,
\*     ends, and the write is prepared again         crates/engine/query/src/runner/executor.rs QueryRunner::resolve_retry_input,
\*     under the attempt's decision                  crates/engine/query/src/runner/prepare.rs QueryRunner::prepare_mutations_again
\*   - the decision is for the request's own        crates/engine/query/src/prepared.rs PreparedDacBypass::caller
\*     caller, never the default identity
\*   - the caller's denial, kept by every copy      crates/foundation/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied, RequestContext::with_dac_bypass
\*   - NAC on and the grant, read in that view      crates/database/db/src/read/lensed/fetcher/mod.rs LensedDocFetcher::dac_bypass
\*   - where the bypass is honoured                 crates/database/db/src/collection/acp.rs check_doc_permission_as
\*   - every runner document check (aggregates,     crates/engine/query/src/runner/dac_bypass.rs QueryRunner::dac_bypassed,
\*     version and subscription reads, writes)        crates/engine/query/src/runner/mutation.rs QueryRunner::check_doc_access_for_read, QueryRunner::check_doc_access_prepared
\*   - plan reads (queries, explain)                crates/engine/query/src/plan/permission_filter.rs AcpReadCheck::allows
EXTENDS Naturals

CONSTANT Bug   \* "None" for the real code; otherwise the guard a RED config breaks

\* The paths a request reads or writes documents along.
Paths == {"query", "commits", "aggregate", "subscription", "version", "explain", "write"}

\* The paths that honour the bypass the request decided. RED SomePaths is
\* the code before #3029: only a query and a commits query got it.
Honours(path) ==
  IF Bug = "SomePaths" THEN path \in {"query", "commits"}         \* only the plan reads honoured it
                       ELSE TRUE                                   \* every path honours it

VARIABLES
  nacOn,     \* NAC is on now
  held,      \* the caller holds `bypass-dac` now
  snap,      \* the running read's <<nacOn, held, denied, path, anonymous>> at its snapshot, or <<>> for none
  prepared,  \* a write is prepared and waiting for its attempt
  req,       \* the prepared write's <<denied, anonymous>>, chosen by its caller
  prepBy,    \* the bypass the prepared write's documents were picked under
  changes,   \* how many changes happened, to keep the model finite
  bad,       \* TRUE once a read bypassed without a grant at its snapshot
  lost,      \* TRUE once a read did not bypass with a grant at its snapshot
  overrode   \* TRUE once a read its caller denied the bypass bypassed

vars == <<nacOn, held, snap, prepared, req, prepBy, changes, bad, lost, overrode>>

\* The decision the code makes from `on` (NAC on), `has` (grant held), `no`
\* (the caller denied the bypass), `anon` (the request has no caller) and
\* `path`. RED NacOff decides as a bare permission check does, where NAC off
\* reads as every permission held. RED Never is the code before #2681: no
\* bypass. RED IgnoreDenial is the code before the denial: it asks NAC
\* whatever the caller said. RED DefaultIdentity decides a write's bypass
\* for the identity document access control acts as, so an anonymous write
\* asks for the node's default identity, which holds `bypass-dac`.
Decide(on, has, no, anon, path) ==
  CASE Bug = "NacOff"          -> (~no /\ ~anon /\ (~on \/ has))
    [] Bug = "Never"           -> FALSE
    [] Bug = "IgnoreDenial"    -> (~anon /\ on /\ has)
    [] Bug = "DefaultIdentity" -> (~no /\ on /\ (IF anon /\ path = "write" THEN TRUE ELSE (~anon /\ has)))
    [] OTHER                   -> (~no /\ ~anon /\ on /\ has)

TypeOK ==
  /\ nacOn \in BOOLEAN                                             \* on or off
  /\ held \in BOOLEAN                                              \* held or not
  /\ snap \in {<<>>} \cup (BOOLEAN \X BOOLEAN \X BOOLEAN \X Paths \X BOOLEAN) \* no read, or its snapshot, denial, path and anonymity
  /\ prepared \in BOOLEAN                                          \* a write is prepared or not
  /\ req \in BOOLEAN \X BOOLEAN                                    \* the prepared write's denial and anonymity
  /\ prepBy \in BOOLEAN                                            \* bypassed or not when picked
  /\ changes \in 0..2                                              \* at most two changes
  /\ bad \in BOOLEAN                                               \* the wrong-bypass flag
  /\ lost \in BOOLEAN                                              \* the missed-bypass flag
  /\ overrode \in BOOLEAN                                          \* the overridden-denial flag

Init ==
  /\ nacOn \in BOOLEAN                                             \* NAC starts either way
  /\ held \in BOOLEAN                                              \* the grant starts either way
  /\ snap = <<>>                                                   \* no read running
  /\ prepared = FALSE                                              \* no write prepared
  /\ req = <<FALSE, FALSE>>                                        \* a placeholder until a write is prepared
  /\ prepBy = FALSE                                                \* a placeholder until a write is prepared
  /\ changes = 0                                                  \* nothing changed yet
  /\ bad = FALSE                                                   \* nothing wrong yet
  /\ lost = FALSE                                                  \* nothing missed yet
  /\ overrode = FALSE                                               \* no denial overridden yet

\* A write is prepared: its caller denies it the bypass or not and has an
\* identity or not, and preparation picks its documents under the bypass
\* NAC grants right now, outside any transaction.
Prepare ==
  /\ snap = <<>> /\ ~prepared                                      \* nothing running, no write waiting
  /\ \E no, anon \in BOOLEAN :                                       \* the caller's choices
       /\ req' = <<no, anon>>                                      \* kept for the write's attempts
       /\ prepBy' = Decide(nacOn, held, no, anon, "write")          \* picked under the bypass as it is now
  /\ prepared' = TRUE                                              \* the write waits for its attempt
  /\ UNCHANGED <<nacOn, held, snap, changes, bad, lost, overrode>>

\* A request begins: it takes its snapshot. A read's caller either denies it
\* the bypass or leaves the decision to NAC, has an identity or not, and
\* goes along one path; a prepared write's attempt keeps its caller's
\* choices.
Begin ==
  /\ snap = <<>>                                                   \* one request at a time
  /\ \/ /\ ~prepared                                               \* a read
        /\ \E no, anon \in BOOLEAN, path \in Paths \ {"write"} :      \* the caller's choices, and the path
             snap' = <<nacOn, held, no, path, anon>>               \* what it will see, the denial, the path, the caller
     \/ /\ prepared                                                \* the prepared write's attempt
        /\ snap' = <<nacOn, held, req[1], "write", req[2]>>         \* what it will see, with its caller's choices
  /\ UNCHANGED <<nacOn, held, prepared, req, prepBy, changes, bad, lost, overrode>>

\* The request decides the bypass and returns its rows or changes its
\* documents. The code reads NAC and the grant in the request's own
\* transaction, so it sees the snapshot; RED Live reads them as they are now,
\* outside it. A write's attempt that decides the other bypass than its
\* documents were picked under ends without changing anything, and the
\* write is prepared again under the attempt's decision; RED StalePrep goes
\* on instead, reaching only the documents picked under the stale bypass.
Finish ==
  /\ snap # <<>>                                                   \* a request is running
  /\ LET on    == IF Bug = "Live" THEN nacOn ELSE snap[1]          \* NAC as the decision sees it
         has   == IF Bug = "Live" THEN held ELSE snap[2]           \* the grant as the decision sees it
         no    == snap[3]                                          \* the caller denied the bypass
         path  == snap[4]                                          \* the path it goes along
         anon  == snap[5]                                          \* it has no caller
         by    == Decide(on, has, no, anon, path) /\ Honours(path) \* the decision, as its path applies it
         write == path = "write"                                   \* it is a write's attempt
         stale == write /\ prepBy # by                              \* its documents were picked under the other bypass
         reach == IF write THEN by /\ prepBy ELSE by               \* a write reaches only the documents it was given
         ok    == ~no /\ ~anon /\ snap[1] /\ snap[2]                \* the answer at the rows' snapshot
     IN IF stale /\ Bug # "StalePrep"
        THEN /\ prepBy' = by                                       \* prepared again under the attempt's decision
             /\ snap' = <<>>                                       \* the attempt ends, changing nothing
             /\ UNCHANGED <<prepared, req, bad, lost, overrode>>   \* the write still waits, nothing was answered
        ELSE /\ bad' = (bad \/ (reach /\ ~ok))                     \* bypassed without a grant
             /\ lost' = (lost \/ (ok /\ ~reach))                   \* missed a grant
             /\ overrode' = (overrode \/ (no /\ reach))            \* bypassed a denial
             /\ snap' = <<>>                                       \* the request is over
             /\ prepared' = FALSE                                  \* a write is done
             /\ UNCHANGED <<req, prepBy>>                          \* kept as they were
  /\ UNCHANGED <<nacOn, held, changes>>

\* NAC is turned off or back on.
Toggle ==
  /\ changes < 2                                                   \* keep the model finite
  /\ nacOn' = ~nacOn                                               \* flip it
  /\ changes' = changes + 1                                        \* count it
  /\ UNCHANGED <<held, snap, prepared, req, prepBy, bad, lost, overrode>>

\* The grant is revoked or given back.
Grant ==
  /\ changes < 2                                                   \* keep the model finite
  /\ held' = ~held                                                 \* flip it
  /\ changes' = changes + 1                                        \* count it
  /\ UNCHANGED <<nacOn, snap, prepared, req, prepBy, bad, lost, overrode>>

Next == Prepare \/ Begin \/ Finish \/ Toggle \/ Grant

Spec == Init /\ [][Next]_vars

\* Promise 1: no bypass without NAC on and the grant, at the read's snapshot,
\* nor for a request without a caller.
INV_BypassOnlyWhenGranted == ~bad

\* Promise 2: a bypass whenever NAC was on and the grant held, at that
\* snapshot, and the caller did not deny it.
INV_BypassWhenGranted == ~lost

\* Promise 3: a read its caller denied the bypass never bypasses.
INV_DeniedNeverBypasses == ~overrode
====
