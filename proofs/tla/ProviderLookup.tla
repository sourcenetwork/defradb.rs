---- MODULE ProviderLookup ----
\* The story: our node (the reader) needs a provider for a collection and
\* looks in its book of service records. The owner of that collection
\* announced its record on gossip, but the announcement never reached the
\* reader. The reader is connected to one peer (the helper). When a lookup
\* finds nothing, the reader asks the helper who holds the collection, and
\* the helper answers with the records it holds, its own included. A later
\* lookup finds the owner.
\*
\* Who acts: the helper (learning the owner's record from gossip, answering
\* questions), the reader (looking up, asking when its book has nothing).
\* The helper may be the owner itself, in which case its book never holds the
\* owner's record: only its own record can answer.
\*
\* What must never happen: the reader asks while its book already has a
\* provider. What must always happen: the reader finds the owner. A reader
\* asks at most once per ask interval, however many lookups it makes.
\*
\* Code it models (crates/p2p/src/iroh/records):
\*   task.rs  Records::providers              (Lookup: the book, else Who to connected peers, once per interval)
\*   task.rs  Records::on_message, Who         (Answer: book records and our own record)
\*   task.rs  Records::on_message, Records     (Answer lands in the reader's book)
EXTENDS Naturals

CONSTANTS
  HelperIsOwner, \* TRUE when the connected peer is the owner itself
  AskMode,       \* "Ask" (an empty lookup asks the helper) | "NoAsk"
  GuardMode,     \* "Empty" (only an empty lookup asks) | "Always" (every lookup asks)
  AnswerMode     \* "WithOwn" (the answer includes the helper's own record) | "BookOnly"

ASSUME HelperIsOwner \in BOOLEAN
ASSUME AskMode \in {"Ask", "NoAsk"}
ASSUME GuardMode \in {"Empty", "Always"}
ASSUME AnswerMode \in {"WithOwn", "BookOnly"}

VARIABLES
  helperHas,  \* the helper's book holds the owner's record
  readerHas,  \* the reader's book holds the owner's record
  asked,      \* the reader asked within the current ask interval
  inflight,   \* a question to the helper is unanswered
  needless    \* ghost: the reader asked while its book had a provider

vars == <<helperHas, readerHas, asked, inflight, needless>>

TypeOK ==
  /\ helperHas \in BOOLEAN
  /\ readerHas \in BOOLEAN
  /\ asked \in BOOLEAN
  /\ inflight \in BOOLEAN
  /\ needless \in BOOLEAN

\* Nobody but the owner knows the owner's record yet.
Init ==
  /\ helperHas = FALSE
  /\ readerHas = FALSE
  /\ asked = FALSE
  /\ inflight = FALSE
  /\ needless = FALSE

\* The helper hears the owner's announcement. An owner never puts its own
\* record in its book.
HelperLearns ==
  /\ ~HelperIsOwner
  /\ ~helperHas
  /\ helperHas' = TRUE
  /\ UNCHANGED <<readerHas, asked, inflight, needless>>

\* The reader looks up providers. A book with a provider answers it; an empty
\* book sends a question, at most once per ask interval.
Lookup ==
  /\ ~asked
  /\ AskMode = "Ask"
  /\ asked' = TRUE
  /\ inflight' = TRUE
  /\ needless' = (needless \/ readerHas)
  /\ UNCHANGED <<helperHas, readerHas>>

\* An empty lookup is the only kind that asks, unless the guard is broken.
LookupGuarded ==
  /\ GuardMode = "Always" \/ ~readerHas
  /\ Lookup

\* The helper answers. The owner's record is in the answer if the helper's
\* book holds it, or if the helper is the owner and sends its own record.
Answer ==
  /\ inflight
  /\ inflight' = FALSE
  /\ readerHas' = (readerHas \/ helperHas \/ (HelperIsOwner /\ AnswerMode = "WithOwn"))
  /\ UNCHANGED <<helperHas, asked, needless>>

\* The ask interval passes, so an empty lookup may ask again.
IntervalPasses ==
  /\ asked
  /\ asked' = FALSE
  /\ UNCHANGED <<helperHas, readerHas, inflight, needless>>

Next == HelperLearns \/ LookupGuarded \/ Answer \/ IntervalPasses

\* Gossip reaches the helper eventually, the reader keeps looking up, a
\* question is eventually answered, and the interval eventually passes.
Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ WF_vars(HelperLearns)
  /\ WF_vars(LookupGuarded)
  /\ WF_vars(Answer)
  /\ WF_vars(IntervalPasses)

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* The reader asks only when its book has no provider.
INV_AskOnlyWhenEmpty == ~needless

\* The reader finds the owner.
LIVE_Found == <>readerHas
====
