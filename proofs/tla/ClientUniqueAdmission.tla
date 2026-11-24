---- MODULE ClientUniqueAdmission ----
\* The story: two clients of one node each add a document. Each document
\* carries a value in a field that may have a unique index. A client first
\* runs a filtered read for a value (usually the one it writes, but it may be
\* another), then the write path checks the unique index entry for the value
\* it writes, then the document and its entry are written, then the client
\* commits.
\*
\* A client runs in one of two commit modes:
\*   Explicit    it began a transaction itself and commits it itself. If the
\*               commit loses a conflict, the client is told
\*               "transaction conflict. Please retry" and nothing it wrote stays.
\*   Autocommit  the node runs the write in its own transaction. If the commit
\*               loses a conflict, the node runs the whole write again from a
\*               fresh snapshot, so a lost race ends as the write's own answer.
\*
\* What can happen: the two clients interleave in every order. A client may
\* start after the other committed, or both may read before either writes.
\* Their values may be equal or different, and may be nil, and the value a
\* client's filter reads may differ from the value it writes.
\*
\* What must never happen:
\*   - with the index, two committed documents hold the same non-nil value;
\*   - a refused client leaves a document or an index entry behind;
\*   - an autocommit write is answered with a transaction conflict;
\*   - a unique violation is reported when no other committed document holds
\*     the writer's value, or a conflict when no other committed document
\*     holds a value the client read or wrote;
\*   - without the index, anyone is refused;
\*   - a client writing nil is refused over nil: it may lose its commit only
\*     over a non-nil value its filter read (the known defect below).
\*
\* What the code does today and the house rules forbid: a query read is never
\* a decision (AGENTS.md, Reads and writes), yet the unique-index equality plan
\* reads the entry with a validated get
\* (crates/engine/storage/src/index/eq_iterator.rs ExactMatchIterator::new_unique,
\* its `txn.get`). So an explicit transaction that only filtered for a value
\* it does not write loses its commit when another writer adds that value,
\* and that holds for a client writing nil too. A filter for nil is a prefix
\* scan of the doc-suffixed nil entries, which decides nothing.
\* INV_RefusedOnlyOverItsOwnValue states the rule; the RED configurations
\* MC_ClientUniqueAdmission_Red_FilterReadDecides and
\* MC_ClientUniqueAdmission_Red_FilterReadDecidesNil show the code breaks it,
\* and testo txn::unique_isolation
\* known_defect_a_unique_filter_read_aborts_the_commit and
\* known_defect_a_nil_writers_filter_read_aborts_the_commit pin the same answers.
\*
\* Code it models:
\*   crates/engine/storage/src/index/unique.rs UniqueIndex::save, the `has`
\*     check of the unique entry (a validated Ordinary read), and its nil skip
\*     (`UniqueIndex::has_nil_field`, the doc-suffixed key)
\*   crates/database/db/src/error/mod.rs index_write_query_error, the typed
\*     refusal (QueryError::UniqueViolation)
\*   crates/database/db/src/write/autocommit/helpers.rs the autocommit create
\*     and its index write, run in transact_async, which re-runs a lost attempt
\*   crates/engine/query/src/executor.rs TXN_CONFLICT_MESSAGE, the explicit
\*     commit's refusal, and QueryResponseError::from_query_error, which adds
\*     UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE
\* The engine boundary: regolith v0.2.5 (055589006356738a31ee3b0e965d8e2a9c5bc057)
\* validates an Ordinary read at commit and aborts it when a newer commit
\* wrote its key (proofs/tla/RepeatableRead.tla, MC_DefraLevel_* configs);
\* commit_nowait validates exactly as commit does (src/transaction/nowait.rs:1-6).
\* That is assumed here, not modeled.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Clients,          \* the clients adding a document
  Values,           \* the values a client may write, Nil among them or not
  Mode,             \* "Explicit" or "Autocommit"
  Indexed,          \* the field has a unique index
  EntryValidated,   \* the unique entry check is validated at commit (the real code); FALSE plants a bug
  ScanDecides,      \* without an index, the filtered read is validated like a decision (a planted bug)
  LoserLeaves,      \* a commit that lost still leaves its index entry (a planted bug)
  RerunReuses,      \* an autocommit re-run keeps the first attempt's check instead of checking again (a planted bug)
  NilChecked,       \* nil is checked like any other value (a planted bug)
  MaxAttempts       \* how many attempts one write may take, to keep the model finite

\* The nil value: the field is unset.
Nil == "nil"

\* Two clients are enough to race; the modes and flags are what they say.
ASSUME Cardinality(Clients) = 2
ASSUME Mode \in {"Explicit", "Autocommit"}
ASSUME {Indexed, EntryValidated, ScanDecides, LoserLeaves, RerunReuses, NilChecked} \subseteq BOOLEAN
ASSUME MaxAttempts \in Nat \ {0}

VARIABLES
  input,      \* input[c]: the value client c writes, fixed for the run
  wanted,     \* wanted[c]: the value client c's filter reads, fixed for the run
  pc,         \* pc[c]: where client c is (begin, filter, check, write, commit, done, refused)
  snap,       \* snap[c]: the commit number client c's snapshot reads at
  attempts,   \* attempts[c]: how many attempts client c's write has begun
  checked,    \* checked[c]: client c's unique entry read is validated at its commit
  scanned,    \* scanned[c]: client c's filtered read is validated at its commit
  reason,     \* reason[c]: why client c was refused, "none" while it was not
  docs,       \* the clients whose documents are committed
  entries,    \* the committed unique entries, each with its value, owner and commit number
  lastWrite,  \* lastWrite[v]: the commit number of the newest commit that wrote value v
  seq         \* the number of the newest commit

vars == <<input, wanted, pc, snap, attempts, checked, scanned, reason, docs, entries, lastWrite, seq>>

\* The places a client can be.
Places == {"begin", "filter", "check", "write", "commit", "done", "refused"}

\* The answers a refused client can get.
Reasons == {"none", "TxnConflict", "UniqueViolation"}

\* A value the unique index checks: every non-nil value, and nil too only under
\* the NilChecked bug.
Checks(v) == Indexed /\ (v # Nil \/ NilChecked)

\* Value v already has a committed entry that client c's snapshot sees.
SeesEntry(c, v) == \E e \in entries : e.v = v /\ e.at <= snap[c]

\* Client c's commit must abort: a read it validates names a value, and a
\* commit after its snapshot wrote that value. The entry check names the value
\* c writes; the filter names the value c read.
Conflicts(c) ==
  \/ checked[c] /\ lastWrite[input[c]] > snap[c]
  \/ scanned[c] /\ lastWrite[wanted[c]] > snap[c]

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ input \in [Clients -> Values]
  /\ wanted \in [Clients -> Values]
  /\ pc \in [Clients -> Places]
  /\ snap \in [Clients -> Nat]
  /\ attempts \in [Clients -> 0..MaxAttempts]
  /\ checked \in [Clients -> BOOLEAN]
  /\ scanned \in [Clients -> BOOLEAN]
  /\ reason \in [Clients -> Reasons]
  /\ docs \subseteq Clients
  /\ \A e \in entries : e.v \in Values /\ e.owner \in Clients /\ e.at \in Nat
  /\ lastWrite \in [Values -> Nat]
  /\ seq \in Nat

\* Each client picks any value to write and any value to filter for;
\* nothing is committed and nobody has begun.
Init ==
  /\ input \in [Clients -> Values]
  /\ wanted \in [Clients -> Values]
  /\ pc = [c \in Clients |-> "begin"]
  /\ snap = [c \in Clients |-> 0]
  /\ attempts = [c \in Clients |-> 0]
  /\ checked = [c \in Clients |-> FALSE]
  /\ scanned = [c \in Clients |-> FALSE]
  /\ reason = [c \in Clients |-> "none"]
  /\ docs = {}
  /\ entries = {}
  /\ lastWrite = [v \in Values |-> 0]
  /\ seq = 0

\* An attempt begins: it takes a snapshot at the newest commit and has
\* validated nothing yet. Every client, nil writers included, runs its
\* filtered read next.
Begin(c) ==
  /\ pc[c] = "begin"
  /\ attempts[c] < MaxAttempts
  /\ snap' = [snap EXCEPT ![c] = seq]
  /\ attempts' = [attempts EXCEPT ![c] = @ + 1]
  /\ checked' = [checked EXCEPT ![c] = FALSE]
  /\ scanned' = [scanned EXCEPT ![c] = FALSE]
  /\ pc' = [pc EXCEPT ![c] = "filter"]
  /\ UNCHANGED <<input, wanted, reason, docs, entries, lastWrite, seq>>

\* The filtered read, for value wanted[c]. For a non-nil value served by the
\* unique index, whether it is validated depends on the plan, so both are
\* explored: today the equality plan validates it with a `get` (a known
\* defect) and a full scan does not. A filter for nil is a prefix scan
\* (ExactMatchIterator::new_unique, its `has_nil` branch), and without an
\* index the read is a plain scan; both are validated only under the
\* ScanDecides bug.
Filter(c) ==
  /\ pc[c] = "filter"
  \* the plans this read may take: validated or not under the index's
  \* equality lookup, otherwise a scan validated only under the bug
  /\ \E validated \in (IF Indexed /\ wanted[c] # Nil THEN BOOLEAN ELSE {ScanDecides}) :
       scanned' = [scanned EXCEPT ![c] = validated]
  /\ pc' = [pc EXCEPT ![c] = "check"]
  /\ UNCHANGED <<input, wanted,snap, attempts, checked, reason, docs, entries, lastWrite, seq>>

\* The write path checks the unique entry. Found at the snapshot, the write is
\* refused with the unique violation and writes nothing. Not found, the read is
\* kept for validation at commit (unless the EntryValidated bug drops it).
\* A value the index does not check goes straight on.
Check(c) ==
  /\ pc[c] = "check"
  /\ IF Checks(input[c]) /\ SeesEntry(c, input[c])
       THEN /\ pc' = [pc EXCEPT ![c] = "refused"]
            /\ reason' = [reason EXCEPT ![c] = "UniqueViolation"]
            /\ UNCHANGED checked
       ELSE /\ pc' = [pc EXCEPT ![c] = "write"]
            /\ checked' = [checked EXCEPT ![c] = Checks(input[c]) /\ EntryValidated]
            /\ UNCHANGED reason
  /\ UNCHANGED <<input, wanted,snap, attempts, scanned, docs, entries, lastWrite, seq>>

\* The document and its entry are written in the transaction; nothing is
\* visible to the other client until the commit.
Write(c) ==
  /\ pc[c] = "write"
  /\ pc' = [pc EXCEPT ![c] = "commit"]
  /\ UNCHANGED <<input, wanted,snap, attempts, checked, scanned, reason, docs, entries, lastWrite, seq>>

\* The entry a commit of client c lands: with the index, its value, owner and
\* commit number.
Landed(c) ==
  IF Indexed THEN {[v |-> input[c], owner |-> c, at |-> seq + 1]} ELSE {}

\* The commit wins: its document and entry are committed under a new number.
CommitWins(c) ==
  /\ ~Conflicts(c)
  /\ seq' = seq + 1
  /\ docs' = docs \cup {c}
  /\ entries' = entries \cup Landed(c)
  /\ lastWrite' = [lastWrite EXCEPT ![input[c]] = seq + 1]
  /\ pc' = [pc EXCEPT ![c] = "done"]
  /\ UNCHANGED <<input, wanted,snap, attempts, checked, scanned, reason>>

\* The commit loses. Its writes are dropped, except the entry the LoserLeaves
\* bug keeps. Explicit: the client is told the transaction conflicted.
\* Autocommit: the write runs again from a fresh snapshot, or, under the
\* RerunReuses bug, goes straight to its write with no new check.
CommitLoses(c) ==
  /\ Conflicts(c)
  /\ IF LoserLeaves /\ Indexed
       THEN /\ seq' = seq + 1
            /\ entries' = entries \cup Landed(c)
            /\ lastWrite' = [lastWrite EXCEPT ![input[c]] = seq + 1]
       ELSE UNCHANGED <<seq, entries, lastWrite>>
  /\ UNCHANGED docs
  /\ IF Mode = "Explicit"
       THEN /\ pc' = [pc EXCEPT ![c] = "refused"]
            /\ reason' = [reason EXCEPT ![c] = "TxnConflict"]
            /\ UNCHANGED <<snap, attempts, checked, scanned>>
       ELSE IF RerunReuses
         THEN /\ pc' = [pc EXCEPT ![c] = "write"]
              /\ snap' = [snap EXCEPT ![c] = seq']
              /\ checked' = [checked EXCEPT ![c] = FALSE]
              /\ scanned' = [scanned EXCEPT ![c] = FALSE]
              /\ UNCHANGED <<attempts, reason>>
         ELSE /\ pc' = [pc EXCEPT ![c] = "begin"]
              /\ UNCHANGED <<snap, attempts, checked, scanned, reason>>
  /\ UNCHANGED <<input, wanted>>

\* A client at its commit either wins or loses, as validation decides.
Commit(c) ==
  /\ pc[c] = "commit"
  /\ (CommitWins(c) \/ CommitLoses(c))

\* Any client takes its next step.
Next == \E c \in Clients : Begin(c) \/ Filter(c) \/ Check(c) \/ Write(c) \/ Commit(c)

\* The runs: start in Init, then take steps of Next.
Spec == Init /\ [][Next]_vars

\* With the index, at most one committed document holds each non-nil value.
INV_AtMostOnePerNonNilValue ==
  Indexed => \A v \in Values \ {Nil} : Cardinality({c \in docs : input[c] = v}) <= 1

\* A refused client has no committed document and no committed entry.
INV_LoserLeavesNothing ==
  \A c \in Clients : pc[c] = "refused" =>
    /\ c \notin docs
    /\ \A e \in entries : e.owner # c

\* A refusal is the mode's own answer and has a cause: an autocommit write is
\* only ever refused as a unique violation, never as a conflict; a unique
\* violation means another committed document holds the writer's value; a
\* conflict means another committed document holds a value the client read
\* or wrote.
INV_LoserRefusedTyped ==
  \A c \in Clients : pc[c] = "refused" =>
    \* autocommit answers only with the unique violation
    /\ (Mode = "Autocommit" => reason[c] = "UniqueViolation")
    \* a unique violation: someone else committed the value c writes
    /\ (reason[c] = "UniqueViolation" => \E d \in docs \ {c} : input[d] = input[c])
    \* a conflict: someone else committed a value c wrote or read
    /\ (reason[c] = "TxnConflict" =>
          \E d \in docs \ {c} : input[d] \in {input[c], wanted[c]})

\* The house rule (a query read decides nothing): nobody is refused unless
\* another committed document holds the value it writes. The code breaks it
\* today, so only the RED configuration FilterReadDecides checks it.
INV_RefusedOnlyOverItsOwnValue ==
  \A c \in Clients : pc[c] = "refused" => \E d \in docs \ {c} : input[d] = input[c]

\* Without the index, nobody is refused.
INV_NoIndexNotRefused == ~Indexed => \A c \in Clients : pc[c] # "refused"

\* A client writing nil is never refused over nil: if it is refused at all,
\* it lost its commit over a non-nil value its filter read, which another
\* client committed.
INV_NilNotRefused ==
  \A c \in Clients : (input[c] = Nil /\ pc[c] = "refused") =>
    \* never a unique violation, which nil cannot be
    /\ reason[c] = "TxnConflict"
    \* the value it lost over is the non-nil one it read
    /\ wanted[c] # Nil
    /\ \E d \in docs \ {c} : input[d] = wanted[c]
====
