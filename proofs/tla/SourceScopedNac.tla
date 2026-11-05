------------------------ MODULE SourceScopedNac ------------------------
\* A publisher changes configuration and one source grant in one transaction.
\* A concurrent revocation must conflict; other sources and direct grants stay.
\* This models native NAC relationship staging and transaction validation.
EXTENDS Naturals

CONSTANT Bug
\* The bug selector removes one guard in a RED configuration.
VARIABLES config, grant, version, seen, staged, bad, leaked, revived, direct, other, owner, membership
\* These variables describe durable state, a staged attempt, and observed failures.
vars == <<config, grant, version, seen, staged, bad, leaked, revived, direct, other, owner, membership>>

Init ==
  /\ config = TRUE /\ grant = TRUE /\ version = 0
  /\ seen = 0 /\ staged = FALSE
  /\ bad = FALSE /\ leaked = FALSE /\ revived = FALSE
  /\ direct = TRUE /\ other = TRUE /\ owner = TRUE /\ membership = TRUE

Stage(ownerMatch, authority, writable, bound, fresh, validSource) ==
  /\ ~staged /\ config
  /\ LET allowed == (ownerMatch \/ Bug = "Owner") /\
       (authority \/ Bug = "Authority") /\ (writable \/ Bug = "Readonly") /\
       (bound \/ Bug = "Store") /\ (fresh \/ Bug = "Expired") /\
       (validSource \/ Bug = "InvalidSource")
     IN /\ staged' = allowed
        /\ bad' = (bad \/ (allowed /\ ~(ownerMatch /\ authority /\ writable /\ bound /\ fresh /\ validSource)))
  /\ seen' = version
  /\ IF Bug = "Autocommit" /\ staged' THEN
       /\ grant' = FALSE /\ leaked' = TRUE
     ELSE /\ UNCHANGED <<grant, leaked>>
  /\ UNCHANGED <<config, version, revived, direct, other, owner, membership>>

OtherRevoke ==
  /\ config /\ grant
  /\ config' = FALSE /\ grant' = FALSE /\ version' = 1 - version
  /\ UNCHANGED <<seen, staged, bad, leaked, revived, direct, other, owner, membership>>

Commit ==
  /\ staged
  /\ IF seen = version \/ Bug = "Conflict" THEN
       /\ config' = TRUE /\ grant' = TRUE /\ version' = 1 - version
       /\ revived' = (revived \/ ~grant)
     ELSE /\ UNCHANGED <<config, grant, version, revived>>
  /\ staged' = FALSE
  /\ UNCHANGED <<seen, bad, leaked, direct, other, owner, membership>>

Disable ==
  /\ grant
  /\ config' = FALSE /\ grant' = FALSE /\ version' = 1 - version
  /\ direct' = IF Bug = "Sources" THEN FALSE ELSE direct
  /\ membership' = IF Bug = "Membership" THEN FALSE ELSE membership
  /\ UNCHANGED <<seen, staged, bad, leaked, revived, other, owner>>

Discard ==
  /\ staged /\ staged' = FALSE
  /\ UNCHANGED <<config, grant, version, seen, bad, leaked, revived, direct, other, owner, membership>>

Next ==
  (\E ownerMatch, authority, writable, bound, fresh, validSource \in BOOLEAN :
    Stage(ownerMatch, authority, writable, bound, fresh, validSource))
  \/ OtherRevoke \/ Commit \/ Disable \/ Discard
Spec == Init /\ [][Next]_vars

\* Denied actors cannot stage native permission changes.
INV_Authority == ~bad
\* An uncommitted transaction is never visible.
INV_Private == ~leaked
\* A stale transaction cannot restore a revoked source.
INV_NoRevival == ~revived
\* Removing one source does not revoke independent native paths.
INV_Independent == direct /\ other /\ owner /\ membership
=============================================================================
