---- MODULE ServiceRecords ----
\* The story: a node (the owner) signs a record of the services it offers and
\* republishes it, with a higher sequence number, whenever it changes or
\* before it expires. Another node (the reader) keeps the newest copy it has
\* seen in its record book, and uses it to pick peers.
\*
\* Who acts: the owner publishing records, the discovery gossip topic
\* carrying them (it may lose or reorder them), the owner's pkarr pointer
\* that names its newest sequence number, the reader querying the owner when
\* its pointer is ahead (on every connected peer's turn in a rotation, not
\* only when it connects), and time making old records expire.
\*
\* What must never happen: an older record replaces a newer one, or the
\* reader picks a peer from an expired record. What must always happen: the
\* reader ends up with the owner's newest record, even when gossip loses it.
\*
\* Records are named by their sequence number: the owner signs exactly one
\* record per number, so the number says which record it is.
\*
\* Code it models (crates/p2p/src/iroh/records):
\*   book.rs      Book::offer     (a record replaces a lower seq or an expired one, and is never expired)
\*   book.rs      Book::live      (an expired record is never handed out)
\*   refresh.rs   Refresh::check  (query the owner when its pointer is ahead of the book)
\*   task.rs      Records::rotate (the lookup that enables Query keeps recurring)
\*   task.rs      Records::renew  (Publish and UpdatePointer: a higher seq, then its pointer)
\*   task.rs      Records::on_input, Input::Pointer and RecordsMessage::Records (Query, Deliver)
EXTENDS Naturals

CONSTANTS
  MaxSeq,       \* how many records the owner publishes in the run
  SeqMode,      \* "Increasing" (a record must beat the book's seq) | "Any" (the last one in wins)
  ChooseMode,   \* "CheckExpiry" (an expired record is never used) | "NoCheck"
  RefreshMode   \* "QueryOnStale" (a pointer ahead of the book triggers a query, again and again)
                \* | "QueryOnConnect" (only the lookup made when the peer connects) | "GossipOnly"

\* The owner publishes at least one record.
ASSUME MaxSeq \in Nat \ {0}
ASSUME SeqMode \in {"Increasing", "Any"}
ASSUME ChooseMode \in {"CheckExpiry", "NoCheck"}
ASSUME RefreshMode \in {"QueryOnStale", "QueryOnConnect", "GossipOnly"}

\* Sequence numbers the owner may sign; 0 means "no record".
Seqs == 1..MaxSeq

VARIABLES
  latest,        \* the newest record the owner has signed (0 before the first)
  pointer,       \* the seq the owner's pkarr pointer names (it trails latest)
  inflight,      \* records the discovery topic is still carrying to the reader
  book,          \* the reader's copy of the owner's record (0 when it has none)
  highest,       \* the highest seq the reader has ever accepted
  expired,       \* records whose expiry time has passed
  choseExpired,  \* the reader once picked a peer from an expired record
  queried        \* the reader has queried the owner at least once

vars == <<latest, pointer, inflight, book, highest, expired, choseExpired, queried>>

TypeOK ==
  /\ latest \in 0..MaxSeq
  /\ pointer \in 0..MaxSeq
  /\ inflight \subseteq Seqs
  /\ book \in 0..MaxSeq
  /\ highest \in 0..MaxSeq
  /\ expired \subseteq Seqs
  /\ choseExpired \in BOOLEAN
  /\ queried \in BOOLEAN

\* Nothing is published, carried, kept or expired yet.
Init ==
  /\ latest = 0
  /\ pointer = 0
  /\ inflight = {}
  /\ book = 0
  /\ highest = 0
  /\ expired = {}
  /\ choseExpired = FALSE
  /\ queried = FALSE

\* The reader takes record s, if the book's rule lets it. A record must not
\* have expired, and (when sequence numbers are checked) must be newer than
\* the copy the reader already has, unless that copy has expired: an expired
\* copy counts as none, so an owner whose seq went back is not shut out.
Offer(s) ==
  IF /\ s \notin expired
     /\ SeqMode = "Any" \/ s > book \/ book \in expired
  THEN /\ book' = s
       \* Remember the highest seq ever accepted, to catch a step backwards.
       /\ highest' = IF s > highest THEN s ELSE highest
  ELSE UNCHANGED <<book, highest>>

\* The owner signs its next record and announces it on the discovery topic.
Publish ==
  /\ latest < MaxSeq
  /\ latest' = latest + 1
  /\ inflight' = inflight \cup {latest + 1}
  /\ UNCHANGED <<pointer, book, highest, expired, choseExpired, queried>>

\* The owner's pkarr pointer catches up with its newest record.
UpdatePointer ==
  /\ pointer < latest
  /\ pointer' = latest
  /\ UNCHANGED <<latest, inflight, book, highest, expired, choseExpired, queried>>

\* The topic delivers one announcement it carries, in any order.
Deliver(s) ==
  /\ s \in inflight
  /\ inflight' = inflight \ {s}
  /\ Offer(s)
  /\ UNCHANGED <<latest, pointer, expired, choseExpired, queried>>

\* The topic loses one announcement. Nothing promises gossip delivers.
Lose(s) ==
  /\ s \in inflight
  /\ inflight' = inflight \ {s}
  /\ UNCHANGED <<latest, pointer, book, highest, expired, choseExpired, queried>>

\* The reader sees the pointer ahead of its book (or its copy expired) and
\* asks the owner directly, which answers with its newest record. Looking
\* the pointer up only when the peer connects allows one query at most.
Query ==
  /\ RefreshMode = "QueryOnStale" \/ (RefreshMode = "QueryOnConnect" /\ ~queried)
  /\ pointer > book \/ book \in expired
  /\ Offer(latest)
  /\ queried' = TRUE
  /\ UNCHANGED <<latest, pointer, inflight, expired, choseExpired>>

\* An old record's expiry time passes. The owner refreshes its newest record
\* before it expires, so only records it has replaced run out, in the order
\* it signed them.
Expire(s) ==
  /\ s < latest
  /\ s \notin expired
  /\ \A t \in 1..(s - 1) : t \in expired
  /\ expired' = expired \cup {s}
  /\ UNCHANGED <<latest, pointer, inflight, book, highest, choseExpired, queried>>

\* The reader picks a peer from its copy of the record. With the expiry
\* check it refuses an expired copy; without it, it may use one.
Choose ==
  /\ book # 0
  /\ ChooseMode = "NoCheck" \/ book \notin expired
  /\ choseExpired' = (choseExpired \/ book \in expired)
  /\ UNCHANGED <<latest, pointer, inflight, book, highest, expired, queried>>

Next ==
  \/ Publish
  \/ UpdatePointer
  \/ \E s \in Seqs : Deliver(s) \/ Lose(s) \/ Expire(s)
  \/ Query
  \/ Choose

\* The owner keeps publishing, its pointer keeps catching up, the reader
\* keeps querying when it is behind, and gossip delivers what it has not
\* lost. Losing and expiring are never forced.
Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ WF_vars(Publish)
  /\ WF_vars(UpdatePointer)
  /\ WF_vars(Query)
  /\ WF_vars(\E s \in Seqs : Deliver(s))

INV_TypeOK == TypeOK

\* An older record never replaces a newer one: the book always holds the
\* highest seq the reader has accepted.
INV_SeqNeverGoesBack == book = highest

\* The reader never picks a peer from an expired record.
INV_NoExpiredChosen == ~choseExpired

\* The reader ends up holding the owner's newest record, and keeps it.
LIVE_NewestReached == <>[](book = MaxSeq)
====
