--------------------------- MODULE MissedTopicResync ---------------------------
\* The story. A provider updates a document N times and announces each new
\* head on a topic through a pinc hub. The hub may drop an announcement; when
\* it does, it tells the receiver that messages of that topic may be missing
\* (pinc's Event::Missed). The receiver then pulls the heads again with a
\* request its peers already serve, and fetches what it lacks.
\*
\* Three receivers:
\*   "doc":     follows the document, so the topic is the document's ID and
\*              the pull is a DocSync request for that document;
\*   "branch":  subscribes to a branchable collection, so the topic is the
\*              collection's ID and the pull is a BranchableSync request;
\*   "foreign": is told about a topic it neither follows nor subscribes to.
\*
\* What must never happen:
\*   - a receiver that follows the topic is left behind once announcements
\*     stop and every missed report is handled;
\*   - a receiver pulls for a topic it does not follow.
\*
\* Rust: crates/database/pinc/src/topics.rs topic_events (Event::Missed becomes
\* TransportEvent::TopicMissed); crates/database/p2p/src/sync/coordinator/
\* event_handler/topic_missed.rs handle_topic_missed (the topic check, and
\* DocSync for a followed document, BranchableSync for a subscribed
\* collection); event_handler/doc_sync.rs handle_doc_sync_reply and
\* event_handler/branchable_sync.rs handle_branchable_sync_reply (a reply's
\* heads are fetched).
EXTENDS Naturals

\* N: how many updates the provider makes.
\* Bug: "none" keeps every guard (GREEN); each other value removes one:
\*   "nopull"    a missed report is only logged       -> INV_Converges
\*   "wrongkind" a document's topic is pulled with a
\*               BranchableSync request                -> INV_Converges
\*   "anytopic"  a report for any topic is pulled      -> INV_OnlyOwnTopics
CONSTANTS N, Bug

\* provided: the provider's latest head (0 before any update). got: the
\* latest head the receiver holds. missed: a missed report is waiting to be
\* handled. pulls: pulls sent for a foreign topic. world: the receiver.
VARIABLES provided, got, missed, pulls, world

\* Every variable, for UNCHANGED.
vars == <<provided, got, missed, pulls, world>>

\* The larger of two heads.
Max(a, b) == IF a >= b THEN a ELSE b

\* Whether this receiver follows the topic.
Own == world # "foreign"

\* The heads a pull brings back: a DocSync request names a document, so it
\* answers on a document's topic; a BranchableSync request names a
\* collection, so it answers on a branchable collection's topic.
Answer(kind) ==
  IF (kind = "docsync" /\ world = "doc") \/ (kind = "branchable" /\ world = "branch")
  THEN provided
  ELSE 0

\* The pull a topic gets: DocSync for a document, BranchableSync for a
\* collection ("wrongkind" sends a document's topic the collection request).
PullKind == IF world = "doc" /\ Bug # "wrongkind" THEN "docsync" ELSE "branchable"

\* Each receiver starts before any update.
Init ==
  /\ world \in {"doc", "branch", "foreign"}
  /\ provided = 0
  /\ got = 0
  /\ missed = FALSE
  /\ pulls = 0

\* The provider makes an update; its announcement reaches the receiver, or
\* the hub drops it and reports the topic missed.
Publish ==
  \* Only while updates remain.
  /\ provided < N
  \* The new head.
  /\ provided' = provided + 1
  \* Delivered: a following receiver fetches up to it. Dropped: reported.
  /\ \/ /\ got' = IF Own THEN provided + 1 ELSE got
        /\ UNCHANGED missed
     \/ /\ missed' = TRUE
        /\ UNCHANGED got
  \* The receiver and its foreign pulls do not change here.
  /\ UNCHANGED <<pulls, world>>

\* The receiver handles a missed report: for a topic it follows it pulls the
\* heads and fetches what the reply names ("nopull" only logs; "anytopic"
\* pulls a foreign topic too).
HandleMissed ==
  \* A report is waiting.
  /\ missed
  \* It is handled now.
  /\ missed' = FALSE
  \* A pull happens for an own topic, or for any with "anytopic".
  /\ IF Bug # "nopull" /\ (Own \/ Bug = "anytopic")
       \* The reply's heads are fetched; a foreign pull is counted.
       THEN /\ got' = IF Own THEN Max(got, Answer(PullKind)) ELSE got
            /\ pulls' = IF Own THEN pulls ELSE pulls + 1
       \* Nothing is pulled.
       ELSE UNCHANGED <<got, pulls>>
  \* The provider and the receiver kind do not change here.
  /\ UNCHANGED <<provided, world>>

\* Nothing is left to do; it stays that way.
Done ==
  /\ provided = N
  /\ ~missed
  /\ UNCHANGED vars

\* One step.
Next == Publish \/ HandleMissed \/ Done

\* Any run.
Spec == Init /\ [][Next]_vars

\* Every variable stays in its range.
INV_TypeOK ==
  /\ provided \in 0..N
  /\ got \in 0..N
  /\ missed \in BOOLEAN
  /\ pulls \in Nat

\* Once updates stop and every report is handled, a following receiver
\* holds the latest head.
INV_Converges == (Own /\ provided = N /\ ~missed) => got = N

\* A topic the receiver does not follow is never pulled.
INV_OnlyOwnTopics == pulls = 0
=============================================================================
