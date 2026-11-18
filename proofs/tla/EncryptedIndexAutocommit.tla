---- MODULE EncryptedIndexAutocommit ----
\* The story: several clients add an encrypted index to the same collection
\* at the same time, each on a different field, and none of them uses an
\* explicit transaction. Each request reads the collection's definition,
\* adds its field to the definition's list of encrypted indexes, and writes
\* the whole definition back.
\*
\* What can happen: two requests read the same definition, and one commits
\* its new definition before the other commits. The second one's copy of the
\* definition is then out of date.
\*
\* What must never happen: a client is told its index was added and the
\* stored definition does not have it (the later writer overwrote it with an
\* out-of-date copy), or a client's valid request is refused only because
\* another client's change committed first.
\*
\* The two guards that prevent it:
\*   - the definition is read in the request's own transaction with a
\*     validated read, so a commit that changed it in between makes this
\*     commit conflict instead of overwriting it;
\*   - the request runs in an engine-retried transaction, so the conflict
\*     runs the request again against the new definition instead of failing.
\* A delete takes the same path (one read, one rewrite of the definition), so
\* this model of adds stands for both.
\*
\* Code it models:
\*   crates/db/src/collection/encrypted_index.rs  DB::add_encrypted_index,
\*                                                 DB::delete_encrypted_index
\*                                                 (one DB::transact_db attempt
\*                                                 each), rewrite_definition_in,
\*                                                 definition_in (the validated read)
\*   crates/db/src/txn/attempt.rs                 DB::transact_db (the re-run)
\*   crates/defra-node/src/ops/encrypted_index.rs NodeOps::add_encrypted_index,
\*                                                 NodeOps::delete_encrypted_index
\*   crates/cli/src/encrypted_index_adapter.rs    EncryptedIndexAdapter (the
\*                                                 `defradb start` HTTP path)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Clients,       \* the requests; each adds the encrypted index on its own field
  ValidatedRead, \* the definition is read in the transaction, validated at commit
  Retry          \* a commit conflict runs the request again

\* There is at least one client, and each switch is on or off.
ASSUME Clients # {} /\ ValidatedRead \in BOOLEAN /\ Retry \in BOOLEAN

VARIABLES
  def,     \* the fields the stored definition has an encrypted index on
  version, \* how many definition changes have committed
  pc,      \* where each client is: "start", "read", "done" or "refused"
  seen,    \* the definition each client read in its current attempt
  seenVer  \* the version each client read in its current attempt

vars == <<def, version, pc, seen, seenVer>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* the definition lists some of the clients' fields
  /\ def \subseteq Clients
  \* the version counts commits
  /\ version \in Nat
  \* each client is at one of four places
  /\ pc \in [Clients -> {"start", "read", "done", "refused"}]
  \* each client remembers a definition and its version
  /\ seen \in [Clients -> SUBSET Clients]
  /\ seenVer \in [Clients -> Nat]

\* Nothing indexed yet, no commits, every client about to start.
Init ==
  \* the stored definition has no encrypted index
  /\ def = {} /\ version = 0
  \* every client is at the start and has read nothing
  /\ pc = [c \in Clients |-> "start"]
  /\ seen = [c \in Clients |-> {}]
  /\ seenVer = [c \in Clients |-> 0]

\* Client c reads the stored definition and the version it is at.
Read(c) ==
  \* a client reads at the start of an attempt
  /\ pc[c] = "start"
  \* it remembers the definition and its version
  /\ seen' = [seen EXCEPT ![c] = def]
  /\ seenVer' = [seenVer EXCEPT ![c] = version]
  \* and moves on to commit
  /\ pc' = [pc EXCEPT ![c] = "read"]
  /\ UNCHANGED <<def, version>>

\* Client c commits the definition it read with its own field added.
\* With a validated read, a definition that changed since the read makes the
\* commit conflict; without one (the RED), the commit always lands.
Commit(c) ==
  \* a client commits after it read
  /\ pc[c] = "read"
  \* the commit lands when nothing changed since the read, or nothing checks
  /\ IF ~ValidatedRead \/ version = seenVer[c]
       \* it writes its copy of the definition, plus its own field
       THEN /\ def' = seen[c] \cup {c}
            \* one more change committed
            /\ version' = version + 1
            \* and the client is told it succeeded
            /\ pc' = [pc EXCEPT ![c] = "done"]
       \* a conflict: run again from the read, or (the RED) refuse the client
       ELSE /\ pc' = [pc EXCEPT ![c] = IF Retry THEN "start" ELSE "refused"]
            \* nothing is written
            /\ UNCHANGED <<def, version>>
  /\ UNCHANGED <<seen, seenVer>>

\* Some client takes its next step.
Next == \E c \in Clients : Read(c) \/ Commit(c)

\* Once every client has an answer, nothing more happens.
Done == (\A c \in Clients : pc[c] \in {"done", "refused"}) /\ UNCHANGED vars

\* The clients run their steps in any interleaving, then rest.
Spec == Init /\ [][Next \/ Done]_vars

\* A client told its index was added finds it on the stored definition.
INV_NoLostIndex == \A c \in Clients : pc[c] = "done" => c \in def

\* No client's valid request is refused because another one committed first.
INV_NoValidRefused == \A c \in Clients : pc[c] # "refused"
====
