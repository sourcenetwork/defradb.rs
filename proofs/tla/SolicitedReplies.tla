---- MODULE SolicitedReplies ----
\* The story: our node sends a peer requests whose replies come back on a new
\* stream (SE query, manage, manage query). Before sending a request, the
\* endpoint task writes down that it expects a reply from that peer on that
\* reply tag, until a deadline. When a reply stream arrives, admission takes
\* one expectation before it reads the stream's length or body; with nothing
\* to take, it resets the stream.
\*
\* Who acts: the endpoint task (expecting, then sending, a request; pruning
\* expired entries), the peer (replying once per request), the same peer or
\* anyone holding its connection (opening reply streams nobody asked for),
\* admission tasks (taking expectations, concurrently with the endpoint
\* task), and time (closing the reply window).
\*
\* What must never happen: a reply stream is admitted when no reply is owed,
\* or a reply that is owed is refused.
\*
\* Matching is per peer and reply tag, the keys the stream tag and the
\* connection give before the body is read. Which request a reply answers is
\* matched downstream by message id, in the correlators. One key is modelled:
\* keys never touch each other's entries. The cost of matching by key: a
\* reply later than its window can take the slot of a newer request to the
\* same peer, whose reply is then refused and its requester times out as if
\* the peer were slow. The window is twice the longest requester timeout.
\*
\* Code it models (crates/p2p/src/iroh):
\*   solicited.rs          Solicited::expect (ExpectRead, ExpectWrite; an expired entry restarts at one)
\*   solicited.rs          Solicited::take   (Admit)
\*   solicited.rs          Solicited::prune  (Prune)
\*   endpoint_commands.rs  the SE query, manage and manage query request arms (expect, then send)
\*   endpoint_streams.rs   admit_stream      (take before reading the length)
EXTENDS Naturals

CONSTANTS
  MaxReq,     \* how many requests the endpoint sends in the run
  MatchMode,  \* "Match" (admit only against a live expectation) | "NoMatch" (admit everything)
  TakeMode,   \* "Decrement" (admitting uses up one expectation) | "Keep"
  PruneMode,  \* "ExpiredSerial" | "ExpiredConcurrent" (prune may run between an expect's read and write) | "Any" (prune live entries too)
  SendMode,   \* "ExpectFirst" (expect, then send) | "SendFirst" (send, then expect)
  ExpiredMode \* "Reset" (an expect drops an expired entry's leftovers) | "Carry"

\* The endpoint sends at least one request.
ASSUME MaxReq \in Nat \ {0}
ASSUME MatchMode \in {"Match", "NoMatch"}
ASSUME TakeMode \in {"Decrement", "Keep"}
ASSUME PruneMode \in {"ExpiredSerial", "ExpiredConcurrent", "Any"}
ASSUME SendMode \in {"ExpectFirst", "SendFirst"}
ASSUME ExpiredMode \in {"Reset", "Carry"}

VARIABLES
  sent,        \* requests the endpoint has started
  entry,       \* the table's entry for the key: 0 when absent, else its identity
  count,       \* the entry's expectations not yet taken
  live,        \* the entry's deadline has not passed
  nextId,      \* the identity a newly inserted entry gets
  expecting,   \* an expect has read the table and not written yet
  seen,        \* the entry that expect read (0: it found none)
  owed,        \* ghost: replies the peer still owes inside the window
  inflight,    \* honest replies on their way
  bad,         \* ghost: a reply stream was admitted when none was owed
  refused      \* ghost: an owed honest reply was refused

vars == <<sent, entry, count, live, nextId, expecting, seen, owed, inflight, bad, refused>>

TypeOK ==
  /\ sent \in 0..MaxReq
  /\ entry \in 0..MaxReq
  /\ count \in 0..MaxReq
  /\ live \in BOOLEAN
  /\ nextId \in 1..(MaxReq + 1)
  /\ expecting \in BOOLEAN
  /\ seen \in 0..MaxReq
  /\ owed \in 0..MaxReq
  /\ inflight \in 0..MaxReq
  /\ bad \in BOOLEAN
  /\ refused \in BOOLEAN

\* Nothing sent, nothing expected, nothing owed.
Init ==
  /\ sent = 0
  /\ entry = 0
  /\ count = 0
  /\ live = FALSE
  /\ nextId = 1
  /\ expecting = FALSE
  /\ seen = 0
  /\ owed = 0
  /\ inflight = 0
  /\ bad = FALSE
  /\ refused = FALSE

\* The request goes out: from now on the peer owes a reply.
SendRequest ==
  /\ owed' = owed + 1
  /\ inflight' = inflight + 1

\* The endpoint task starts an expect: it looks up the key's entry. Under
\* "SendFirst" the request has already gone out at this point.
ExpectRead ==
  /\ ~expecting
  /\ sent < MaxReq
  /\ expecting' = TRUE
  /\ seen' = entry
  /\ sent' = sent + 1
  /\ IF SendMode = "SendFirst"
     THEN SendRequest
     ELSE UNCHANGED <<owed, inflight>>
  /\ UNCHANGED <<entry, count, live, nextId, bad, refused>>

\* The expect writes: it adds one to the entry it read and moves its deadline
\* on, or inserts a new entry if it found none. An entry whose deadline has
\* passed starts again from one: its old expectations belong to requests
\* whose requesters gave up. If the entry it read has been removed since, the
\* write lands on the removed entry and is lost.
ExpectWrite ==
  /\ expecting
  /\ expecting' = FALSE
  /\ CASE seen # 0 /\ seen = entry ->
            \* The entry it read is still in the table.
            /\ count' = IF live \/ ExpiredMode = "Carry" THEN count + 1 ELSE 1
            /\ live' = TRUE
            /\ UNCHANGED <<entry, nextId>>
       [] seen = 0 /\ entry = 0 ->
            \* It found no entry, and there still is none: insert one.
            /\ entry' = nextId
            /\ nextId' = nextId + 1
            /\ count' = 1
            /\ live' = TRUE
       [] OTHER ->
            \* The entry it read was pruned in between: the increment is lost.
            UNCHANGED <<entry, count, live, nextId>>
  /\ IF SendMode = "ExpectFirst"
     THEN SendRequest
     ELSE UNCHANGED <<owed, inflight>>
  /\ UNCHANGED <<sent, seen, bad, refused>>

\* Time passes the entry's deadline. The requester has given up, so nothing
\* is owed any more; replies still on their way arrive late.
Expire ==
  /\ live
  /\ live' = FALSE
  /\ owed' = 0
  /\ UNCHANGED <<sent, entry, count, nextId, expecting, seen, inflight, bad, refused>>

\* The endpoint task removes the entry. Only an expired one, unless the mode
\* plants the bug; and never between its own expect's read and write, since
\* expect and prune both run on the endpoint task.
Prune ==
  /\ entry # 0
  /\ PruneMode = "Any" \/ ~live
  /\ PruneMode # "ExpiredSerial" \/ ~expecting
  /\ entry' = 0
  /\ count' = 0
  /\ live' = FALSE
  /\ UNCHANGED <<sent, nextId, expecting, seen, owed, inflight, bad, refused>>

\* Admission's check: a live entry with an expectation left.
Accepts ==
  MatchMode = "NoMatch" \/ (entry # 0 /\ live /\ count > 0)

\* A reply stream arrives. Admitted: it uses up an expectation, and is a bug
\* if no reply was owed. Refused: a bug if it was an owed honest reply.
Admit(honest) ==
  /\ IF Accepts
     THEN /\ count' = IF TakeMode = "Decrement" /\ count > 0 THEN count - 1 ELSE count
          /\ bad' = (bad \/ owed = 0)
          /\ owed' = IF owed > 0 THEN owed - 1 ELSE 0
          /\ UNCHANGED refused
     ELSE /\ refused' = (refused \/ (honest /\ owed > 0))
          /\ UNCHANGED <<count, bad, owed>>
  /\ UNCHANGED <<sent, entry, live, nextId, expecting, seen>>

\* The peer's reply to one of our requests arrives.
HonestReply ==
  /\ inflight > 0
  /\ inflight' = inflight - 1
  /\ Admit(TRUE)

\* A reply stream nobody asked for, or a second reply to one request.
ForgedReply ==
  /\ Admit(FALSE)
  /\ UNCHANGED inflight

Next ==
  \/ ExpectRead
  \/ ExpectWrite
  \/ Expire
  \/ Prune
  \/ HonestReply
  \/ ForgedReply

Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* A reply stream is admitted only while the peer owes a reply.
INV_NoUnsolicitedAdmitted == ~bad

\* A reply the peer owes, inside its window, is never refused.
INV_SolicitedAdmitted == ~refused
====
