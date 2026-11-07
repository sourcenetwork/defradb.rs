------------------------ MODULE SourceScopedNac ------------------------
\* A publisher changes configuration and one source grant in one transaction.
\* A concurrent revocation must conflict; other sources and direct grants stay.
\* This models native NAC relationship staging and transaction validation.
\* A trusted embedder's stage also replays the root's descendant membership
\* edge as the same actor: a present edge needs nothing, a missing one needs
\* the actor's own authority, node authority is never borrowed, and a refused
\* descendant rolls the whole statement back. A request caller does not replay.
\* Rust: crates/query/src/txn/source_nac.rs source_permission_job_for;
\* crates/db/src/document_projection.rs change_source_permission.
EXTENDS Naturals

CONSTANT Bug
\* The bug selector removes one guard in a RED configuration.
VARIABLES config, grant, version, seen, staged, bad, leaked, revived, direct, other, owner, membership,
          desc, stagedDesc, stagedTrusted, unreconciled, requestReplayed
\* These variables describe durable state, a staged attempt, and observed failures.
\* desc: the durable descendant membership edge beneath the root.
\* stagedDesc: the descendant the staged attempt will commit.
\* stagedTrusted: the staged attempt came from the trusted embedder.
\* unreconciled: a trusted embedder commit left its descendant missing.
\* requestReplayed: a request caller's attempt created a descendant.
vars == <<config, grant, version, seen, staged, bad, leaked, revived, direct, other, owner, membership,
          desc, stagedDesc, stagedTrusted, unreconciled, requestReplayed>>

Init ==
  /\ config = TRUE /\ grant = TRUE /\ version = 0
  /\ seen = 0 /\ staged = FALSE
  /\ bad = FALSE /\ leaked = FALSE /\ revived = FALSE
  /\ direct = TRUE /\ other = TRUE /\ owner = TRUE /\ membership = TRUE
  \* The descendant may be present or missing (the gap being repaired).
  /\ desc \in BOOLEAN /\ stagedDesc = desc /\ stagedTrusted = FALSE
  /\ unreconciled = FALSE /\ requestReplayed = FALSE

\* A trusted embedder that restates an unchanged edge needs no authority.
\* An untrusted caller, or any changing edge, still needs it.
NeedsAuthority(trusted, changed) == changed \/ ~trusted

\* The Unchanged bug misreads a changing edge as unchanged.
SeenChanged(changed) == changed /\ Bug # "Unchanged"

\* Who replays: the trusted embedder (unless NoReplay drops it), and a
\* request caller only under the UntrustedReplay bug.
Replays(trusted) ==
  (trusted /\ Bug # "NoReplay") \/ (~trusted /\ Bug = "UntrustedReplay")

\* A descendant replays if present or if the actor holds authority for it.
\* BorrowNode plants the bug of falling back to the node's authority.
DescAllowed(descAuthority, nodeAuthority) ==
  desc \/ descAuthority \/ (Bug = "BorrowNode" /\ nodeAuthority)

Stage(ownerMatch, authority, writable, bound, fresh, validSource, trusted, changed,
      descAuthority, nodeAuthority) ==
  /\ ~staged /\ config
  /\ LET root == (ownerMatch \/ Bug = "Owner") /\
       (~NeedsAuthority(trusted, SeenChanged(changed)) \/ authority \/ Bug = "Authority") /\
       (writable \/ Bug = "Readonly") /\
       (bound \/ Bug = "Store") /\ (fresh \/ Bug = "Expired") /\
       (validSource \/ Bug = "InvalidSource")
         \* Does this attempt replay, and may its descendant replay?
         replay == Replays(trusted)
         descOk == DescAllowed(descAuthority, nodeAuthority)
         \* A refused descendant refuses the root too; Partial keeps the root.
         allowed == root /\ (~replay \/ descOk \/ Bug = "Partial")
         \* The descendant is created only when the replay itself was allowed.
         replayed == root /\ replay /\ descOk
         \* What the unbroken guards permit, to judge the staged result.
         permitted == ownerMatch /\ writable /\ bound /\ fresh /\ validSource /\
           (~NeedsAuthority(trusted, changed) \/ authority) /\
           (~trusted \/ desc \/ descAuthority)
     IN /\ staged' = allowed
        /\ bad' = (bad \/ (allowed /\ ~permitted))
        /\ stagedDesc' = (IF allowed /\ replayed THEN TRUE ELSE desc)
        /\ stagedTrusted' = (allowed /\ trusted)
        /\ requestReplayed' = (requestReplayed \/ (allowed /\ ~trusted /\ replayed /\ ~desc))
  /\ seen' = version
  /\ IF Bug = "Autocommit" /\ staged' THEN
       /\ grant' = FALSE /\ leaked' = TRUE
     ELSE /\ UNCHANGED <<grant, leaked>>
  /\ UNCHANGED <<config, version, revived, direct, other, owner, membership, desc, unreconciled>>

OtherRevoke ==
  /\ config /\ grant
  /\ config' = FALSE /\ grant' = FALSE /\ version' = 1 - version
  /\ UNCHANGED <<seen, staged, bad, leaked, revived, direct, other, owner, membership>>
  /\ UNCHANGED <<desc, stagedDesc, stagedTrusted, unreconciled, requestReplayed>>

Commit ==
  /\ staged
  /\ IF seen = version \/ Bug = "Conflict" THEN
       /\ config' = TRUE /\ grant' = TRUE /\ version' = 1 - version
       /\ revived' = (revived \/ ~grant)
       \* The descendant commits with its root, in the same transaction.
       /\ desc' = stagedDesc
       /\ unreconciled' = (unreconciled \/ (stagedTrusted /\ ~stagedDesc))
     ELSE /\ UNCHANGED <<config, grant, version, revived, desc, unreconciled>>
  /\ staged' = FALSE
  /\ UNCHANGED <<seen, bad, leaked, direct, other, owner, membership>>
  /\ UNCHANGED <<stagedDesc, stagedTrusted, requestReplayed>>

Disable ==
  /\ grant
  /\ config' = FALSE /\ grant' = FALSE /\ version' = 1 - version
  /\ direct' = IF Bug = "Sources" THEN FALSE ELSE direct
  /\ membership' = IF Bug = "Membership" THEN FALSE ELSE membership
  /\ UNCHANGED <<seen, staged, bad, leaked, revived, other, owner>>
  /\ UNCHANGED <<desc, stagedDesc, stagedTrusted, unreconciled, requestReplayed>>

Discard ==
  /\ staged /\ staged' = FALSE
  /\ UNCHANGED <<config, grant, version, seen, bad, leaked, revived, direct, other, owner, membership>>
  /\ UNCHANGED <<desc, stagedDesc, stagedTrusted, unreconciled, requestReplayed>>

Next ==
  (\E ownerMatch, authority, writable, bound, fresh, validSource, trusted, changed,
      descAuthority, nodeAuthority \in BOOLEAN :
    Stage(ownerMatch, authority, writable, bound, fresh, validSource, trusted, changed,
          descAuthority, nodeAuthority))
  \/ OtherRevoke \/ Commit \/ Disable \/ Discard
Spec == Init /\ [][Next]_vars

\* Denied actors cannot stage native permission changes or probe untrusted no-ops.
INV_Authority == ~bad
\* An uncommitted transaction is never visible.
INV_Private == ~leaked
\* A stale transaction cannot restore a revoked source.
INV_NoRevival == ~revived
\* Removing one source does not revoke independent native paths.
INV_Independent == direct /\ other /\ owner /\ membership
\* A committed trusted embedder stage leaves its descendant present.
INV_Reconciled == ~unreconciled
\* A request caller's statement never creates a descendant by replay.
INV_RequestUnchanged == ~requestReplayed
=============================================================================
