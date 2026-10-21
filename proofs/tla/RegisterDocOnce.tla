---- MODULE RegisterDocOnce ----
\* The story, in plain words.
\*
\* When a document is created under a policy, the node records who owns it:
\* one key, /acp/<collection>/<doc>/owner/<did>, in its own store. The
\* registration runs in one transaction. It looks under the document's prefix
\* /acp/<collection>/<doc>/. If it finds anything, the document is already
\* registered and it returns false without writing. If it finds nothing, it
\* writes its owner key and commits.
\*
\* Two registrations of the same document can run at once on two cores, for
\* two different owners. Both look before either has committed, so both find
\* nothing. Each writes its own owner key, and the two keys differ. Only the
\* commit can tell them apart: it has to refuse the second one, which then
\* returns false. The store's contract says exactly one of them wins.
\*
\* Example: Alice and Bob both register doc 7. Both scans find nothing. Alice
\* commits first and owns doc 7. Bob's commit has to conflict, so Bob gets
\* false. If Bob's commit went through, doc 7 would have two owners, and each
\* could grant, revoke or delete for it.
\*
\* What used to make the second commit fail: both also wrote a "sentinel" key,
\* /acp-reg/<collection>/<doc>, holding the byte 1. On RocksDB, two
\* transactions that write one key always conflicted. regolith commits a write
\* that leaves a key holding the bytes it already holds, so on regolith the
\* sentinel never conflicts and both registrations won. Deleting a document's
\* tuples also left the sentinel behind.
\*
\* The fix: the scan that decides is a validated scan. At commit, regolith
\* checks whether anything was written inside the scanned range after the
\* transaction began, and conflicts if so. Alice's owner key lands inside the
\* range Bob scanned, so Bob conflicts. The sentinel is gone.
\*
\* What must never happen:
\*   * a document with two owners;
\*   * a registration that returned true for someone who does not own the
\*     document.
\* And what must happen: when registrations race and nothing is deleted, one
\* of them wins.
\*
\* Code this models (defradb.rs):
\*   crates/acp/src/store.rs        AcpStore::register_doc_atomic (the contract)
\*   crates/acp/src/persistent.rs   PersistentAcpStore::register_doc_atomic,
\*                                  PersistentAcpStore::delete_doc_tuples
\*   crates/storage/src/backends/regolith/iterator.rs  a validated scan is
\*                                  regolith::ScanCheck::Range
\* and regolith 0.2.1:
\*   ScanCheck::Range               a write inside the scanned range newer than
\*                                  the snapshot conflicts (commit/range_rule.rs)
\*   IsolationLevel::DefraLevel     an ordinary key's put conflicts with a newer
\*                                  commit to it, unless the key already holds
\*                                  what the put would leave
\*
\* Correspondence table (model -> Rust):
\*   Begin          persistent.rs register_doc_atomic: `new_txn(false)` takes the
\*                  snapshot, the prefix scan's `iter.next()` gives
\*                  `already_registered`, and `return Ok(false)` when it is set
\*   Commit         persistent.rs register_doc_atomic: `txn.set` of the owner
\*                  tuple, then `txn.commit()`; `Err(e) if e.is_txn_conflict()`
\*                  becomes `Ok(false)`
\*   ValidatedScan  persistent.rs register_doc_atomic: `.with_validation(true)`
\*                  on the scan's IterOptions; iterator.rs maps it to
\*                  `ScanCheck::Range`, which regolith checks at commit
\*   Sentinel       main before this change: `txn.set` of `/acp-reg/{c}/{d}` to
\*                  `[1]`, judged by DefraLevel's rule for ordinary keys
\*   Unregister     persistent.rs delete_doc_tuples: one transaction deletes every
\*                  key under the document's prefix; the sentinel stays
\*   owners         the keys `/acp/{c}/{d}/owner/{did}` (relation.rs storage_key)
\*   rangeSeq       the newest commit inside the prefix, what range_rule.rs
\*                  `written_in_range` compares with the snapshot
\*   sentinel,
\*   sentinelSeq    the `/acp-reg/{c}/{d}` key and its newest commit
\*   seq, snap      regolith's commit sequence and each transaction's snapshot
\*   winners        history variable, no Rust counterpart

EXTENDS Naturals, FiniteSets

CONSTANTS
  Owners,        \* the people registering the same document, one registration each
  MaxDeletes,    \* how many times the document's tuples may be deleted
  ValidatedScan, \* TRUE: the deciding scan is validated (the fix)
  Sentinel       \* TRUE: each registration also writes the sentinel byte (main before the fix)

VARIABLES
  seq,         \* regolith's commit sequence: each commit takes the next number
  owners,      \* who has an owner key stored under the document's prefix
  rangeSeq,    \* the sequence of the newest commit that wrote under the prefix
  sentinel,    \* the sentinel key's value: "absent", or "one" for the byte 1
  sentinelSeq, \* the sequence of the newest commit that wrote the sentinel
  pc,          \* where each registration is: "idle", "open", "won" or "lost"
  snap,        \* the sequence each registration's transaction began at
  deletes,     \* how many deletions of the document's tuples have run
  winners      \* history: registrations that returned true since the last deletion

\* Every variable, for "nothing changed".
vars == <<seq, owners, rangeSeq, sentinel, sentinelSeq, pc, snap, deletes, winners>>

\* The kinds of value each variable may hold.
TypeOK ==
  /\ seq \in Nat                                         \* a commit number
  /\ owners \subseteq Owners                             \* some of the people
  /\ rangeSeq \in Nat                                    \* a commit number
  /\ sentinel \in {"absent", "one"}                      \* missing, or the byte 1
  /\ sentinelSeq \in Nat                                 \* a commit number
  /\ pc \in [Owners -> {"idle", "open", "won", "lost"}]  \* one step per person
  /\ snap \in [Owners -> Nat]                            \* one snapshot per person
  /\ deletes \in 0..MaxDeletes                           \* within the allowance
  /\ winners \subseteq Owners                            \* some of the people

\* At the start nothing is stored and nobody has begun.
Init ==
  /\ seq = 0                             \* no commit yet
  /\ owners = {}                         \* the document has no owner
  /\ rangeSeq = 0                        \* nothing written under the prefix
  /\ sentinel = "absent"                 \* no sentinel key
  /\ sentinelSeq = 0                     \* never written
  /\ pc = [o \in Owners |-> "idle"]      \* every registration is still to come
  /\ snap = [o \in Owners |-> 0]         \* no snapshot taken yet
  /\ deletes = 0                         \* no deletion yet
  /\ winners = {}                        \* nobody has won

\* A registration begins: its transaction takes a snapshot and scans the
\* document's prefix. Something there means the document is registered: it
\* returns false and writes nothing. An empty prefix leaves it open.
Begin(o) ==
  /\ pc[o] = "idle"                                  \* this registration has not begun
  /\ snap' = [snap EXCEPT ![o] = seq]                \* its snapshot is the newest commit
  /\ pc' = [pc EXCEPT ![o] =
             IF owners = {} THEN "open" ELSE "lost"] \* empty prefix: go on; else false
  /\ UNCHANGED <<seq, owners, rangeSeq, sentinel, sentinelSeq, deletes, winners>>

\* Whether regolith refuses o's commit. With the validated scan, any commit
\* under the prefix after o's snapshot conflicts. With the sentinel, a newer
\* commit to the sentinel conflicts only if the key no longer holds the byte 1
\* o would write, and every writer writes 1, so it never does.
Conflicts(o) ==
  \/ ValidatedScan /\ rangeSeq > snap[o]                   \* the scanned range changed
  \/ Sentinel /\ sentinelSeq > snap[o] /\ sentinel # "one" \* the sentinel changed bytes

\* An open registration commits its owner key (and the sentinel, on main).
\* A conflict turns into false; otherwise it owns the document.
Commit(o) ==
  /\ pc[o] = "open"                                        \* it found the prefix empty
  /\ IF Conflicts(o)
       THEN /\ pc' = [pc EXCEPT ![o] = "lost"]             \* refused: returns false
            /\ UNCHANGED <<seq, owners, rangeSeq, sentinel, sentinelSeq, winners>>
       ELSE /\ seq' = seq + 1                              \* the commit takes a number
            /\ owners' = owners \cup {o}                   \* its owner key is stored
            /\ rangeSeq' = seq + 1                         \* a write under the prefix
            /\ sentinel' = IF Sentinel THEN "one" ELSE sentinel       \* main writes 1
            /\ sentinelSeq' = IF Sentinel THEN seq + 1 ELSE sentinelSeq \* at this commit
            /\ winners' = winners \cup {o}                 \* it returned true
            /\ pc' = [pc EXCEPT ![o] = "won"]              \* and is done
  /\ UNCHANGED <<snap, deletes>>

\* The document's tuples are deleted in one transaction, the owner key among
\* them. The sentinel is not under the prefix and stays. Registrations after
\* this may register the document again.
Unregister ==
  /\ owners # {}                  \* there is something to delete
  /\ deletes < MaxDeletes         \* within the allowance, so the model stays finite
  /\ seq' = seq + 1               \* the deletion is a commit
  /\ owners' = {}                 \* every owner key is gone
  /\ rangeSeq' = seq + 1          \* the deletes are writes under the prefix
  /\ deletes' = deletes + 1       \* one more deletion
  /\ winners' = {}                \* a new round of registration starts
  /\ UNCHANGED <<sentinel, sentinelSeq, pc, snap>>

\* Every registration has returned.
Done == \A o \in Owners : pc[o] \in {"won", "lost"}

\* One step: a registration begins or commits, or the tuples are deleted.
\* Once all have returned nothing more needs to happen, which is not a deadlock.
Next ==
  \/ \E o \in Owners : Begin(o) \/ Commit(o)  \* some registration moves
  \/ Unregister                               \* or the document's tuples go
  \/ Done /\ UNCHANGED vars                   \* or everything has finished

\* The whole behaviour: start, then any sequence of steps.
Spec == Init /\ [][Next]_vars

\* A document never has two owners.
INV_OneOwner == Cardinality(owners) <= 1

\* Whoever returned true owns the document, and nobody else does.
INV_WinnersOwn == winners = owners

\* When every registration has returned and nothing was deleted, one of them won.
INV_SomeoneWins == (Done /\ deletes = 0) => winners # {}
====
