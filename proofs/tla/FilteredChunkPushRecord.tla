---- MODULE FilteredChunkPushRecord ----
\* The story: a node (the sender) replicates three collections to a second
\* node (the receiver): `user`, `article`, and `chunk`, which holds the
\* `@chunk` documents of the articles. At first nothing is filtered. Then an
\* admin adds the replicator again with a filter on `article`, so only the
\* matching articles may reach the receiver. A chunk has none of its
\* article's fields, so the replicator record pairs the chunk collection with
\* the article collection, and the article's filter decides the chunks too.
\*
\* The record lives twice: in memory, where every push is decided, and on
\* disk, where a restart loads it from.
\*
\* What can happen:
\*   - the admin adds the filter; the install writes both records at once;
\*   - the admin removes `user` or `article` from the replicator; the node
\*     changes the record in memory, then rewrites the one on disk, and the
\*     add can land between the two steps;
\*   - the node restarts and loads the record from disk, pairing the chunk
\*     collection again from the schema; that lookup can fail;
\*   - the record on disk can become unreadable;
\*   - the sender writes an article and its chunk, and the live push decides
\*     both by the article's document;
\*   - an article or a chunk merged from another node is forwarded with no
\*     article document at hand; bitswap serves chunk blocks the same way;
\*   - a lost push is retried, reading the record on disk.
\*
\* What must never happen:
\*   - the receiver gets a document of an article the filter does not match,
\*     once the filter was added (INV_NothingUnmatched).
\*
\* Code it models:
\*   crates/surfaces/p2p-adapter/src/libp2p.rs, iroh.rs, pinc.rs  add_replicator (pairs the
\*     chunk collection at install), resolve_collections_for_remove, remove_replicator
\*   crates/surfaces/p2p-adapter/src/lib.rs  set_chunk_sources, pair_chunk_sources (keeps a
\*     stored pair whose lookup fails), collections_to_remove
\*   crates/surfaces/p2p-adapter/src/replicator_persist.rs  narrow (one transaction reads
\*     and rewrites the record on disk, and only drops collections from it)
\*   crates/surfaces/p2p-adapter/src/transport_doc_pusher.rs  retry_doc (an unreadable
\*     record sends nothing)
\*   crates/surfaces/p2p-adapter/src/restore.rs  restore_p2p_state (an unreadable record is skipped)
\*   crates/database/p2p/src/replicator.rs  ReplicatorInfo::admission, remove_collection,
\*     retain_chunk_sources
\* Finite sets are all this model needs.
EXTENDS FiniteSets, Naturals

CONSTANTS
  Articles,           \* the articles, for example {a1, a2}
  Matches,            \* the articles the filter matches, for example {a1}
  MaxRestarts,        \* how many times the sender may restart
  LookupCanFail,      \* TRUE: a restart may fail to look up the article's chunk collection
  CanCorrupt,         \* TRUE: the record on disk may become unreadable
  AddPairs,           \* TRUE: adding the replicator pairs the chunk collection, on every transport (the fix)
  RemoveTakesChunks,  \* TRUE: removing the article removes its chunk collection too (the fix)
  KeepStoredPair,     \* TRUE: a restart whose lookup fails keeps the pair on disk (the fix)
  PersistInOneTxn,    \* TRUE: a removal reads and rewrites the record on disk in one transaction (the fix)
  PersistOnlyNarrows, \* TRUE: a removal only drops collections from the record on disk, never adds one (the fix)
  BadRecordFailsClosed \* TRUE: an unreadable record never decides as an unfiltered one (the fix)

\* The filter matches some of the articles.
ASSUME Matches \subseteq Articles
\* The restart bound is a small number.
ASSUME MaxRestarts \in Nat
\* Each switch is yes-or-no.
ASSUME LookupCanFail \in BOOLEAN /\ CanCorrupt \in BOOLEAN /\ AddPairs \in BOOLEAN
\* More switches, each yes-or-no.
ASSUME RemoveTakesChunks \in BOOLEAN /\ KeepStoredPair \in BOOLEAN
\* The last switches, each yes-or-no.
ASSUME PersistInOneTxn \in BOOLEAN /\ PersistOnlyNarrows \in BOOLEAN /\ BadRecordFailsClosed \in BOOLEAN

\* The three replicated collections.
Cols == {"user", "article", "chunk"}
\* The two documents of an article that travel: the article and its chunk.
Kinds == {"article", "chunk"}
\* A received document names its article and which of its documents it is.
Items == Articles \X Kinds

\* A record as the code keeps it: a filter only on a replicated article, and
\* a pair only while the article is filtered and both collections replicate
\* (retain_chunk_sources).
Tidy(cols, filt, pair) ==
  LET f == filt /\ "article" \in cols                    \* the filter goes with its collection
  IN [cols |-> cols, filt |-> f, pair |-> pair /\ f /\ "chunk" \in cols] \* and the pair with both

\* Every record a node can hold.
Records == {Tidy(c, f, p) : c \in SUBSET Cols, f \in BOOLEAN, p \in BOOLEAN}

\* The replicator before the filter: every collection, nothing filtered.
Unfiltered == Tidy(Cols, FALSE, FALSE)

\* No replicator at all.
NoRecord == Tidy({}, FALSE, FALSE)

\* No removal is in progress.
Idle == [phase |-> "idle", cols |-> {}, snap |-> NoRecord, bad |-> FALSE]

VARIABLES
  live,     \* the record in memory, which decides every push
  stored,   \* the record on disk, which a restart and a retry read
  corrupt,  \* TRUE when the record on disk can no longer be read
  added,    \* TRUE once the filter was added
  rm,       \* the removal in progress: its phase, the collections it keeps, what it read from disk
  removed,  \* the collections a removal has started on
  written,  \* the articles written on the sender, which a retry may owe
  recv,     \* the documents the receiver got after the filter was added
  restarts  \* how many times the sender restarted

\* All the variables, for stuttering steps.
vars == <<live, stored, corrupt, added, rm, removed, written, recv, restarts>>

\* The type of every variable.
TypeOK ==
  /\ live \in Records                                  \* the record in memory is well formed
  /\ stored \in Records                                \* and so is the one on disk
  /\ corrupt \in BOOLEAN                               \* readable or not
  /\ added \in BOOLEAN                                 \* the filter was added or not
  /\ rm.phase \in {"idle", "persist"}                  \* a removal is idle or rewriting the disk
  /\ rm.cols \subseteq Cols                            \* the collections it keeps
  /\ rm.snap \in Records /\ rm.bad \in BOOLEAN         \* and what it read from disk
  /\ removed \subseteq {"user", "article"}             \* the removals started
  /\ written \subseteq Articles                        \* the articles written
  /\ recv \subseteq Items                              \* what the receiver got
  /\ restarts \in 0..MaxRestarts                       \* the restarts so far

\* The unfiltered replicator is in place, in memory and on disk.
Init ==
  /\ live = Unfiltered       \* in memory
  /\ stored = Unfiltered     \* and on disk
  /\ corrupt = FALSE         \* readable
  /\ added = FALSE           \* no filter yet
  /\ rm = Idle               \* no removal running
  /\ removed = {}            \* none started
  /\ written = {}            \* nothing written
  /\ recv = {}               \* nothing received since the filter
  /\ restarts = 0            \* no restart yet

\* Whether article a passes the article filter of record r.
Passes(r, a) == ~r.filt \/ a \in Matches

\* What a local write of article a sends: both documents are decided by the
\* article's document and its filter, with or without the pair.
WriteSends(r, a) ==
  (IF "article" \in r.cols /\ Passes(r, a) THEN {<<a, "article">>} ELSE {}) \* the article
  \cup (IF "chunk" \in r.cols /\ Passes(r, a) THEN {<<a, "chunk">>} ELSE {}) \* its chunk

\* What a forward of article a's documents sends, with no article document at
\* hand. A filtered article is refused. A chunk is decided by the article only
\* through the pair; without it the chunk collection has no filter and passes.
ForwardSends(r, a) ==
  (IF "article" \in r.cols /\ ~r.filt THEN {<<a, "article">>} ELSE {}) \* the article
  \cup (IF "chunk" \in r.cols /\ ~r.pair THEN {<<a, "chunk">>} ELSE {}) \* its chunk

\* The admin adds the replicator again with the article filter, naming every
\* collection. The install transaction writes the record on disk, and the
\* node then keeps the same record in memory. The bug leaves the pair out.
AddFilter ==
  /\ ~added                                            \* once
  /\ added' = TRUE                                     \* now filtered
  /\ live' = Tidy(Cols, TRUE, AddPairs)                \* in memory
  /\ stored' = live'                                   \* and on disk
  /\ corrupt' = FALSE                                  \* a fresh record reads again
  /\ UNCHANGED <<rm, removed, written, recv, restarts>> \* nothing else moves

\* The admin removes collection c. The node changes the record in memory and
\* reads back the collections it keeps. Without one transaction it reads the
\* disk now, so an add can land before it writes. A removal that leaves
\* nothing deletes the replicator in both places at once.
RemoveLive(c) ==
  /\ rm.phase = "idle" /\ c \in {"user", "article"}      \* one removal at a time
  /\ c \notin removed /\ c \in live.cols                 \* of a collection it still has
  /\ removed' = removed \cup {c}                         \* started
  /\ LET drop == {c} \cup (IF c = "article" /\ RemoveTakesChunks THEN {"chunk"} ELSE {}) \* the bug keeps the chunk collection
         kept == Tidy(live.cols \ drop, live.filt, live.pair)                          \* the record in memory after
     IN IF kept.cols = {}
        THEN /\ live' = NoRecord /\ stored' = NoRecord   \* the whole replicator goes
             /\ corrupt' = FALSE                         \* with its bytes on disk
             /\ UNCHANGED rm                             \* nothing left to rewrite
        ELSE /\ live' = kept                             \* the record in memory after
             /\ rm' = [phase |-> "persist", cols |-> kept.cols, snap |-> stored, bad |-> corrupt] \* the disk read now
             /\ UNCHANGED <<stored, corrupt>>            \* the disk is written next
  /\ UNCHANGED <<added, written, recv, restarts>>         \* nothing else moves

\* The removal rewrites the record on disk to keep only rm.cols, with the
\* filter and pair it read. In one transaction it reads the disk as it is
\* now; otherwise it uses what it read before. It keeps only collections the
\* record on disk has: rm.cols came from memory, where an add may have put a
\* collection whose filter the disk does not hold; the bug writes rm.cols
\* as they are. An unreadable record is left alone; the bug writes an
\* unfiltered record in its place.
PersistWrite ==
  /\ rm.phase = "persist"                                \* a removal is rewriting the disk
  /\ rm' = Idle                                          \* and is done after this
  /\ LET base == IF PersistInOneTxn THEN stored ELSE rm.snap   \* the record it decides from
         bad == IF PersistInOneTxn THEN corrupt ELSE rm.bad    \* whether that record reads
         cols == IF PersistOnlyNarrows THEN base.cols \cap rm.cols ELSE rm.cols \* what it keeps
     IN IF bad
        THEN IF BadRecordFailsClosed
             THEN UNCHANGED <<stored, corrupt>>                           \* the rewrite is refused
             ELSE /\ stored' = Tidy(rm.cols, FALSE, FALSE)                \* bug: an unfiltered record
                  /\ corrupt' = FALSE                                     \* that reads fine
        ELSE /\ stored' = Tidy(cols, base.filt, base.pair)                \* the read filter and pair, narrowed
             /\ corrupt' = FALSE                                          \* a fresh write reads
  /\ UNCHANGED <<live, added, removed, written, recv, restarts>>          \* nothing else moves

\* The sender restarts and loads the record on disk; a removal in progress
\* dies with it. An unreadable record is skipped, so no replicator comes
\* back. Otherwise the pair is looked up again from the schema; when the
\* lookup fails the fix keeps the pair on disk, and the bug drops it.
Restart ==
  /\ restarts < MaxRestarts                              \* a bounded number of restarts
  /\ restarts' = restarts + 1                            \* one more
  /\ rm' = Idle                                          \* a removal in progress dies
  /\ IF corrupt
     THEN live' = NoRecord                               \* the unreadable record is skipped
     ELSE \E found \in (IF LookupCanFail THEN BOOLEAN ELSE {TRUE}) : \* the lookup may fail
            live' = Tidy(stored.cols, stored.filt,
                         IF found THEN TRUE ELSE KeepStoredPair /\ stored.pair) \* the pair again, or the stored one
  /\ UNCHANGED <<stored, corrupt, added, removed, written, recv>> \* nothing else moves

\* The record on disk becomes unreadable.
Corrupt ==
  /\ CanCorrupt /\ ~corrupt                              \* when that can happen
  /\ corrupt' = TRUE                                     \* it no longer reads
  /\ UNCHANGED <<live, stored, added, rm, removed, written, recv, restarts>> \* nothing else moves

\* The sender writes article a and its chunk; the live push decides them.
Write(a) ==
  /\ added                                               \* after the filter
  /\ written' = written \cup {a}                         \* a retry may owe it now
  /\ recv' = recv \cup WriteSends(live, a)               \* what the push sends
  /\ UNCHANGED <<live, stored, corrupt, added, rm, removed, restarts>> \* nothing else moves

\* Article a's documents merge here from another node and are forwarded.
Forward(a) ==
  /\ added                                               \* after the filter
  /\ recv' = recv \cup ForwardSends(live, a)             \* what the forward sends
  /\ UNCHANGED <<live, stored, corrupt, added, rm, removed, written, restarts>> \* nothing else moves

\* A retry of a's document k reads an unreadable record. The fix sends
\* nothing and clears the marker; the bug sends it as if unfiltered.
RetryBadRecord(a, k) ==
  /\ added /\ a \in written /\ corrupt                   \* a marker and an unreadable record
  /\ ~BadRecordFailsClosed                               \* the fix sends nothing, so nothing changes
  /\ recv' = recv \cup {<<a, k>>}                        \* the bug sends it
  /\ UNCHANGED <<live, stored, corrupt, added, rm, removed, written, restarts>> \* nothing else moves

\* Every step the system can take.
Next ==
  \/ AddFilter \/ PersistWrite \/ Restart \/ Corrupt       \* admin, disk and restarts
  \/ \E c \in {"user", "article"} : RemoveLive(c)          \* removals
  \/ \E a \in Articles : Write(a) \/ Forward(a)            \* pushes
  \/ \E a \in Articles, k \in Kinds : RetryBadRecord(a, k) \* retries of an unreadable record

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Once the filter is added, the receiver gets no document of an article it
\* does not match: not by a write, a forward, a restart or a retry.
INV_NothingUnmatched ==
  \A a \in Articles \ Matches : \A k \in Kinds : <<a, k>> \notin recv
====
