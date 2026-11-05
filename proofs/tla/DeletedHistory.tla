---- MODULE DeletedHistory ----
\* A protected document's authorization covers its retained history, including
\* after a deletion commits or the process restarts. A reused document identity
\* cannot distinguish an old history or its grants from a new incarnation.
\* FenceReuse is an exploratory candidate, not a decision about native purge.
\* The proposed correction concerns unregister_deleted_doc in
\* crates/db/src/write/autocommit/helpers.rs and DirectChecker in
\* crates/acp/src/read_access.rs. It does not model external ACP or replication.
EXTENDS Naturals

\* The constants switch each candidate safeguard on or off.
CONSTANTS RetainOnDelete, FenceReuse, ClearOnPurge, RetainHistory, HonorRevocation
\* The state tracks document phase, authority, history, and observed reuse.
VARIABLES phase, pending, registered, reader, history, revoked, adopted, stale
\* All state variables participate in each transition.
vars == <<phase, pending, registered, reader, history, revoked, adopted, stale>>

\* A live document starts with a protected history and one reader.
Init == /\ phase = "live" /\ pending = FALSE
        /\ registered = TRUE /\ reader = TRUE /\ history = TRUE
        /\ revoked = FALSE /\ adopted = FALSE /\ stale = FALSE

\* The owner begins a deletion without changing durable authority.
Begin == /\ phase = "live" /\ ~pending /\ pending' = TRUE
         /\ UNCHANGED <<phase, registered, reader, history, revoked, adopted, stale>>
\* An aborted deletion leaves its authority intact.
Abort == /\ pending /\ pending' = FALSE
         /\ UNCHANGED <<phase, registered, reader, history, revoked, adopted, stale>>
\* A committed deletion either retains or removes authority.
Commit == /\ pending /\ phase = "live" /\ pending' = FALSE
          /\ phase' = "deleted"
          /\ registered' = IF RetainOnDelete THEN registered ELSE FALSE
          /\ reader' = IF RetainOnDelete THEN reader ELSE FALSE
          /\ UNCHANGED <<history, revoked, adopted, stale>>
\* An owner revokes the reader while registration exists.
Revoke == /\ registered /\ reader
          /\ reader' = IF HonorRevocation THEN FALSE ELSE reader
          /\ revoked' = TRUE
          /\ UNCHANGED <<phase, pending, registered, history, adopted, stale>>
\* A purge may retain history and must clean authority if history is removed.
Purge == /\ phase = "deleted" /\ phase' = "purged"
         /\ history' = RetainHistory
         /\ registered' = IF ~RetainHistory /\ ClearOnPurge THEN FALSE ELSE registered
         /\ reader' = IF ~RetainHistory /\ ClearOnPurge THEN FALSE ELSE reader
         /\ UNCHANGED <<pending, revoked, adopted, stale>>
\* Recreation either fences retained history or risks inheriting old grants.
Recreate == /\ phase = "purged" /\ (FenceReuse => ~history)
            /\ phase' = "reborn" /\ adopted' = history
            /\ stale' = reader /\ registered' = TRUE /\ history' = TRUE
            /\ UNCHANGED <<pending, reader, revoked>>
\* A restart discards an unfinished deletion.
Restart == /\ pending /\ pending' = FALSE
           /\ UNCHANGED <<phase, registered, reader, history, revoked, adopted, stale>>
\* Each step is one of the actions above.
Next == Begin \/ Abort \/ Commit \/ Revoke \/ Purge \/ Recreate \/ Restart
\* The initial state and repeated steps define all modeled executions.
Spec == Init /\ [][Next]_vars

\* Every retained deleted history remains registered.
INV_ProtectedHistory == (phase \in {"deleted", "purged"} /\ history) => registered
\* A revoked reader cannot retain a grant.
INV_Revocation == revoked => ~reader
\* A new document never adopts an old history.
INV_NoAdoptedHistory == ~adopted
\* A new document never inherits a former reader.
INV_NoStaleGrant == ~stale
====
