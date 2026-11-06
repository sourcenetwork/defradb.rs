---- MODULE ClientSignedCreate ----
\* Two sessions of one client each start one create; the server asks for a field wave, then a
\* composite wave. Replies can be forged, delayed, or belong to the other
\* client. A schema change, ACP revocation, expiry or cancellation can race a
\* reply. The server must not use an invalid reply or commit after revocation.
\* The block bytes and cryptographic checks are abstract atoms and Booleans;
\* this does not prove their encoding or signature algorithm. Signatures are
\* ordinary DefraDB block signatures, not cryptographically bound to sessions.
\* Rust owners: HTTP signature continuation, query prepared inputs, db create,
\* and crates/db/src/block/builder/client_waves.rs exact-wave validation.
EXTENDS Naturals, FiniteSets
\* Each switch controls one guard; RED configurations remove one guard.
CONSTANT CheckActor, CheckContext, CheckSignature, CheckClient, CheckLive, CheckAcp, ResolveOutside, Reclaim
\* Two sessions share one actor but own distinct operation and transaction handles.
Clients == {"s1", "s2"}
\* Another actor can try to answer either session.
Actors == {"alice", "mallory"}
\* Every request advances through two waves and at most one commit.
Phases == {"field", "composite", "ready", "committing", "done", "closed"}
\* These flags retain any historical violation even after the request advances.
VARIABLES phase, live, version, acp, active, badActor, badContext, badSignature,
          badClient, badLive, badAcp, networkInside, replayed, finishedExpired
\* The complete state TLC explores.
vars == <<phase, live, version, acp, active, badActor, badContext, badSignature,
          badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* Each original request starts live, authorized and waiting for its field wave.
Init == /\ phase = [c \in Clients |-> "field"]
        /\ live = [c \in Clients |-> TRUE]
        /\ version = 0 /\ acp = TRUE /\ active = [c \in Clients |-> FALSE]
        /\ badActor = FALSE /\ badContext = FALSE /\ badSignature = FALSE
        /\ badClient = FALSE /\ badLive = FALSE /\ badAcp = FALSE
        /\ networkInside = FALSE /\ replayed = FALSE /\ finishedExpired = FALSE
\* A committed definition change, including immutable flags under the same
\* version ID, invalidates the complete prepared schema context.
ChangeSchema == /\ version = 0 /\ version' = 1
                /\ UNCHANGED <<phase, live, acp, active, badActor, badContext,
                                badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* Revocation may happen after preparation but before the deciding commit.
Revoke == /\ acp /\ acp' = FALSE
          /\ UNCHANGED <<phase, live, version, active, badActor, badContext,
                          badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* A client can cancel or let its operation expire while a reply is in flight.
End(c) == /\ live[c] /\ phase[c] # "done"
          /\ live' = [live EXCEPT ![c] = FALSE]
          /\ UNCHANGED <<phase, version, acp, active, badActor, badContext,
                          badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* Cleanup releases the capsule after cancellation or expiry.
Reap(c) == /\ Reclaim /\ ~live[c] /\ phase[c] \in {"field", "composite", "ready"}
           /\ phase' = [phase EXCEPT ![c] = "closed"]
           /\ UNCHANGED <<live, version, acp, active, badActor, badContext,
                           badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* A transaction attempt can be active only until it yields for an input.
BeginAttempt(c) == /\ ~active[c] /\ active' = [active EXCEPT ![c] = TRUE]
                /\ UNCHANGED <<phase, live, version, acp, badActor, badContext,
                                badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* Ending the attempt releases its transaction turn before external work.
EndAttempt(c) == /\ active[c] /\ active' = [active EXCEPT ![c] = FALSE]
              /\ UNCHANGED <<phase, live, version, acp, badActor, badContext,
                              badSignature, badClient, badLive, badAcp, networkInside, replayed, finishedExpired>>
\* A correct client signs only its independently reconstructed wave; clientOk
\* is an abstract history fact, not a flag a server can trust from a caller.
\* A verified signature can also be an older valid signature on the same
\* block; ordinary DefraDB signatures do not identify an HTTP session.
\* The server accepts only its owner, original context and verified wave.
Reply(c, sender, session, contextOk, signatureOk, clientOk, replay) ==
  /\ phase[c] \in {"field", "composite"}
  /\ (CheckActor => sender = "alice")
  /\ (CheckContext => session = c /\ contextOk /\ version = 0)
  /\ (CheckSignature => signatureOk)
  /\ (CheckClient => clientOk \/ replay)
  /\ (CheckLive => live[c])
  /\ (ResolveOutside => ~active[c])
  /\ phase' = [phase EXCEPT ![c] = IF @ = "field" THEN "composite" ELSE "ready"]
  /\ badActor' = (badActor \/ sender # "alice")
  /\ badContext' = (badContext \/ session # c \/ ~contextOk \/ version # 0)
  /\ badSignature' = (badSignature \/ ~signatureOk)
  /\ badClient' = (badClient \/ (~clientOk /\ ~replay))
  /\ replayed' = (replayed \/ (replay /\ ~clientOk))
  /\ badLive' = (badLive \/ ~live[c])
  /\ networkInside' = (networkInside \/ active[c])
  /\ UNCHANGED <<live, version, acp, active, badAcp, finishedExpired>>
\* Dispatch checks the live bearer, schema and ACP before starting a commit.
Dispatch(c) == /\ phase[c] = "ready"
               /\ (CheckLive => live[c]) /\ (CheckAcp => acp)
               /\ (CheckContext => version = 0)
               /\ phase' = [phase EXCEPT ![c] = "committing"]
               /\ badLive' = (badLive \/ ~live[c])
               /\ badAcp' = (badAcp \/ ~acp)
               /\ badContext' = (badContext \/ version # 0)
               /\ UNCHANGED <<live, version, acp, active, badActor,
                               badSignature, badClient, networkInside, replayed, finishedExpired>>
\* A commit that already began finishes even if its caller or token expired.
Commit(c) == /\ phase[c] = "committing"
             /\ phase' = [phase EXCEPT ![c] = "done"]
             /\ finishedExpired' = (finishedExpired \/ ~live[c])
             /\ UNCHANGED <<live, version, acp, active, badActor, badContext,
                             badSignature, badClient, badLive, badAcp, networkInside, replayed>>
\* The environment can interleave each client and all of these changes.
Next == ChangeSchema \/ Revoke
        \/ (\E c \in Clients : BeginAttempt(c) \/ EndAttempt(c) \/ End(c) \/ Reap(c) \/ Dispatch(c) \/ Commit(c))
        \/ (\E c, session \in Clients : \E sender \in Actors :
              \E contextOk, signatureOk, clientOk, replay \in BOOLEAN :
                Reply(c, sender, session, contextOk, signatureOk, clientOk, replay))
\* Stuttering is permitted while clients or storage are unavailable.
Spec == Init /\ [][Next]_vars /\ (\A c \in Clients : WF_vars(Reap(c)))
\* Every request stays in a defined protocol phase.
TypeOK == /\ phase \in [Clients -> Phases] /\ live \in [Clients -> BOOLEAN]
          /\ version \in {0, 1} /\ acp \in BOOLEAN /\ active \in [Clients -> BOOLEAN]
          /\ badActor \in BOOLEAN /\ badContext \in BOOLEAN
          /\ badSignature \in BOOLEAN /\ badClient \in BOOLEAN
          /\ badLive \in BOOLEAN /\ badAcp \in BOOLEAN /\ networkInside \in BOOLEAN /\ replayed \in BOOLEAN /\ finishedExpired \in BOOLEAN
\* Another caller cannot answer a request.
INV_Actor == ~badActor
\* The server does not consume a mismatched operation, wave, byte sequence or schema.
INV_Context == ~badContext
\* A wrong key or invalid signature cannot enter the prepared memo.
INV_Signature == ~badSignature
\* A client does not sign a wave it could not reconstruct from its intent.
INV_Client == ~badClient
\* An expired or cancelled session cannot consume a reply.
INV_Live == ~badLive
\* A revoked caller cannot start a commit.
INV_Acp == ~badAcp
\* Client interaction never holds a database attempt.
INV_Outside == ~networkInside
\* This false claim records native finality when expiry follows dispatch.
INV_NoCompletionAfterExpiry == ~finishedExpired
\* This intentionally false claim documents cross-session signature replay.
INV_NoReplay == ~replayed
\* A cancelled or expired operation eventually releases its capsule.
LIVE_Reaped == \A c \in Clients :
  (phase[c] \in {"field", "composite", "ready"} /\ ~live[c]) ~> (phase[c] = "closed")
====
