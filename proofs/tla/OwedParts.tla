---- MODULE OwedParts ----
\* The story, in plain words.
\*
\* A node keeps a count of what it owes a remote ACP provider: how many
\* relationship deletes are still owed on one document. Three kinds of
\* transaction touch it:
\*   * a stager records one more owed delete (a revocation committed in a
\*     transaction);
\*   * a settler records that one owed delete is settled (the provider now
\*     holds it);
\*   * a decider reads the count to decide whether a reader is denied, and
\*     then commits a write of its own (the transaction the decision
\*     authorizes).
\* The count is one value in two parts: the staged part, which only stagers
\* add to, and the settled part, which only settlers add to. What is owed is
\* staged minus settled. Every write is a merge that adds to one part. A
\* stager's own read stands for a read of what only stagers write: in the
\* code, the tuple's head key (keys.rs head_key), which orders the stagers of
\* one tuple and which settling never touches.
\*
\* The store checks a transaction at commit (regolith's DefraLevel): a
\* transaction that read some parts of the value is refused when a write that
\* touches one of those parts committed after it read. A stager and a decider
\* read the staged part only.
\*
\* What must never happen:
\*   1. a decider commits a decision that nobody is denied while a delete
\*      it did not see was committed before it - INV_NoMissedRevoke;
\*   2. a settle makes a stager or a decider fail its commit
\*      - INV_SettleAbortsNobody;
\*   3. more is settled than was staged - INV_OwedNotNegative.
\*
\* The code this models: crates/engine/acp/src/outbox/ops.rs (stage, settle,
\* owed), crates/engine/acp/src/outbox/deny.rs (owed_denial), over the
\* operand family crates/engine/storage/src/backends/regolith/merge/owed.rs
\* (touches). The counters' algebra is proven in proofs/lean/OwedParts.lean.

EXTENDS Naturals, Sequences, FiniteSets  \* numbers, write logs, sets

CONSTANTS
  MaxWrites,       \* how many writes may commit, to keep the model finite
  SettleParts,     \* the parts a settle's write touches: {"settled"} in the design
  ReadParts,       \* the parts a stager and a decider read: {"staged"} in the design
  SettleGuarded    \* TRUE: a settle needs something owed (the record it settles)

ASSUME SettleParts \subseteq {"staged", "settled"}  \* known parts
ASSUME ReadParts \subseteq {"staged", "settled"}    \* known parts
ASSUME MaxWrites \in Nat /\ SettleGuarded \in BOOLEAN  \* a bound and a switch

Txns == {"stager", "decider"}   \* the transactions that read the count

VARIABLES
  staged,      \* the staged part: deletes ever staged
  settled,     \* the settled part: deletes ever settled
  log,         \* the committed writes, oldest first, each [by, parts, delete]
  phase,       \* per transaction: "idle", "open", "done", "refused"
  snap,        \* per transaction: how many writes it had seen when it read
  denies,      \* the decider's decision when it read: TRUE if denied
  missed,      \* TRUE once a decider committed past a delete it did not see
  bySettle     \* TRUE once a transaction was refused for a settle alone

vars == <<staged, settled, log, phase, snap, denies, missed, bySettle>>  \* all

TypeOK ==
  /\ staged \in Nat /\ settled \in Nat                \* counts
  /\ phase \in [Txns -> {"idle", "open", "done", "refused"}]  \* known phases
  /\ snap \in [Txns -> Nat]                            \* positions in the log
  /\ denies \in BOOLEAN /\ missed \in BOOLEAN /\ bySettle \in BOOLEAN  \* flags

Init ==
  /\ staged = 0 /\ settled = 0                         \* nothing owed yet
  /\ log = <<>>                                        \* nothing committed
  /\ phase = [t \in Txns |-> "idle"]                   \* nobody has read
  /\ snap = [t \in Txns |-> 0]                         \* nothing seen
  /\ denies = FALSE /\ missed = FALSE /\ bySettle = FALSE  \* nothing happened

\* What is owed: staged minus settled, never below zero.
Owed == IF staged >= settled THEN staged - settled ELSE 0

\* The writes committed after transaction t read that touch a part it read.
Conflicts(t) ==
  {i \in 1..Len(log) : i > snap[t] /\ log[i].parts \cap ReadParts # {}}

\* Transaction t reads the count: it remembers how much of the log it saw.
\* A decider decides here, on what it read.
Read(t) ==
  /\ phase[t] = "idle"                                 \* it has not read yet
  /\ phase' = [phase EXCEPT ![t] = "open"]             \* now it has
  /\ snap' = [snap EXCEPT ![t] = Len(log)]             \* at this point of the log
  /\ denies' = IF t = "decider" THEN Owed > 0 ELSE denies  \* the decision
  /\ UNCHANGED <<staged, settled, log, missed, bySettle>>

\* Transaction t commits. Refused when a write touching a part it read
\* committed after its read; a stager's commit is itself a staged write.
Commit(t) ==
  /\ phase[t] = "open"                                 \* it has read
  /\ Len(log) < MaxWrites                              \* the model stays finite
  /\ IF Conflicts(t) # {}
     THEN /\ phase' = [phase EXCEPT ![t] = "refused"]  \* refused at commit,
          /\ bySettle' = (bySettle \/                  \* for settles alone?
                \A i \in Conflicts(t) : log[i].by = "settle")
          /\ UNCHANGED <<staged, log, missed>>
     ELSE /\ phase' = [phase EXCEPT ![t] = "done"]     \* it commits
          /\ IF t = "stager"
             THEN /\ staged' = staged + 1              \* one more delete owed
                  /\ log' = Append(log, [by |-> "stage", parts |-> {"staged"}, delete |-> TRUE])
             ELSE UNCHANGED <<staged, log>>
          \* A decider that let a reader in commits past every delete that
          \* committed after its read: it missed one if there is such a delete.
          /\ missed' = (missed \/ (t = "decider" /\ ~denies /\
                \E i \in 1..Len(log) : i > snap[t] /\ log[i].by = "stage"))
          /\ UNCHANGED bySettle
  /\ UNCHANGED <<settled, snap, denies>>

\* A transaction that was refused runs again from its read.
Retry(t) ==
  /\ phase[t] = "refused"                              \* it was refused
  /\ phase' = [phase EXCEPT ![t] = "idle"]             \* and starts over
  /\ UNCHANGED <<staged, settled, log, snap, denies, missed, bySettle>>

\* Another stager commits a delete without reading anything here (its own
\* read was of the same part; what matters to the others is its write).
StageElsewhere ==
  /\ Len(log) < MaxWrites                              \* the model stays finite
  /\ staged' = staged + 1                              \* one more delete owed
  /\ log' = Append(log, [by |-> "stage", parts |-> {"staged"}, delete |-> TRUE])
  /\ UNCHANGED <<settled, phase, snap, denies, missed, bySettle>>

\* A settler settles one owed delete: a merge into the parts SettleParts names.
Settle ==
  /\ Len(log) < MaxWrites                              \* the model stays finite
  /\ SettleGuarded => Owed > 0                         \* something is owed (the design)
  /\ settled' = settled + 1                            \* one more settled
  /\ log' = Append(log, [by |-> "settle", parts |-> SettleParts, delete |-> TRUE])
  /\ UNCHANGED <<staged, phase, snap, denies, missed, bySettle>>

Next ==
  \/ \E t \in Txns : Read(t) \/ Commit(t) \/ Retry(t)  \* the readers
  \/ StageElsewhere \/ Settle                          \* the writers

Spec == Init /\ [][Next]_vars

\* A decider never commits past a delete it did not see.
INV_NoMissedRevoke == ~missed

\* A settle never makes a stager or a decider fail its commit.
INV_SettleAbortsNobody == ~bySettle

\* Nothing is settled that was not staged.
INV_OwedNotNegative == settled <= staged

====
