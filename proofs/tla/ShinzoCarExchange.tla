------------------------- MODULE ShinzoCarExchange -------------------------
\* The story. A node (the requester) heard about some new document heads but
\* does not hold their blocks yet. It asks other peers (the sources: first the
\* creator, then the relay) for one CAR per head over Go Shinzo's `car`
\* channel. A source answers with a list that lines up with a prefix of the
\* heads it was asked for: an empty entry means "I will not or cannot serve
\* that head", and heads past the end of the list were not tried and may be
\* asked for again. A request can also fail (timeout, dial error, reset), or
\* be cancelled by the requester itself.
\*
\* What must never happen:
\*   - a CAR is imported for a head it is not rooted at (a peer could swap
\*     content in);
\*   - a head the source may not give this peer is served to it;
\*   - a reply entry is matched to a head that was never asked for;
\*   - a head is forgotten: every head ends imported from a CAR, or handed to
\*     the ordinary DAG walk;
\*   - the requester asks one source forever;
\*   - a peer is held back for a cancellation that was the requester's doing;
\*   - a head an honest, healthy, allowed source holds and can fit in a reply
\*     goes to the slow DAG walk anyway.
\*
\* Every node answers Go's request shape whether or not its own Shinzo wire
\* is on: the switch picks what a node sends, never what it serves, so a
\* Shinzo node in a mixed Rust cluster gets one answer per ask instead of a
\* timeout. Rust: crates/p2p/src/host/p2p_host/shinzo_car.rs
\* serve_shinzo_cars (served with the wire off too).
\* Rust: crates/p2p/src/sync/shinzo_car.rs serve_cars (the source side),
\* accept_reply (the requester side) and CarPeerBackoff (who is passed over);
\* the request and its fallback run in crates/p2p/src/two_stream/handler/
\* shinzo_car.rs. Moving on to the next source is the sync coordinator's
\* provider rotation, which already exists. Go: shinzo-develop
\* internal/db/p2p/car_exchange.go serveCARs, fetchCARsFrom, requestCARs.
EXTENDS Naturals, Sequences, FiniteSets

\* Each Bug value removes exactly one guard, so its RED config must fail on
\* the invariant named next to it. "none" keeps every guard (GREEN).
\*   "root"      accept a CAR whatever its root        -> INV_RootMatches
\*   "access"    serve a head without the access gate   -> INV_NoLeak
\*   "length"    accept a reply longer than the ask     -> INV_NoStrayEntry
\*   "empty"     treat an empty reply as progress       -> INV_RoundsBounded
\*   "fallback"  drop the asked heads when a request fails -> INV_Accounted
\*   "blame"     back off a peer for our own cancel     -> INV_BlameOnlyFaults
\*   "oversize"  stop the reply at a too-big CAR        -> INV_Served
\*   "wireoff"   a source with the Shinzo wire off ignores Go's request
\*               -> INV_EveryNodeAnswers
CONSTANT Bug

\* Two heads are enough to show a reply that covers only a prefix.
N == 2
\* The heads, by their position in the request.
Heads == 1..N
\* The sources, in the order Go puts them before backoff reorders them.
Sources == {"creator", "relay"}
\* How many CAR bytes one reply may carry; also the biggest CAR ever sent.
Budget == 1
\* A small CAR fits a reply; a huge one is bigger than any whole reply.
Sizes == {1, 2}
\* A source's standing from earlier failures: asked, or not asked at all
\* because it failed too many times in a row and its window is still open.
Standings == {"ready", "passedOver"}

\* held: which heads each source has stored.
\* readable: which heads each source may hand to this requester.
\* liar: a source that may answer with wrong CARs or too many entries.
\* size: the size of each head's CAR.
\* standing: each source's backoff standing when the fetch starts.
\* order: the sources still to ask, in order.
\* pending: the heads still without a CAR, in request order.
\* ask: the heads asked of the current source and not yet answered.
\* unserved: heads the current source could not give, kept for the next one.
\* result: per head, "none" while open, "car" once imported, "walk" if left
\*   to the DAG walk.
\* root: per head, the root of the CAR imported for it (0 when none).
\* servedBy: per head, the source whose CAR was imported (or "none").
\* rounds: requests sent to the current source.
\* failed: per source, whether any request to it failed or was cancelled.
\* faulted, backedOff: per source, whether it really failed, and whether the
\*   requester held it back.
\* stray: whether an entry was matched to a head that was not asked for.
\* done: the fetch has finished.
\* wireOn: per source, whether its own Shinzo wire is on.
\* ignored: whether a source let a request go unanswered because its wire
\*   was off.
VARIABLES held, readable, liar, size, standing, order, pending, ask,
          unserved, result, root, servedBy, rounds, failed, faulted,
          backedOff, stray, done, wireOn, ignored

\* Every variable, for UNCHANGED.
vars == <<held, readable, liar, size, standing, order, pending, ask,
          unserved, result, root, servedBy, rounds, failed, faulted,
          backedOff, stray, done, wireOn, ignored>>

\* A sequence of all members of a set, in one fixed order.
SeqOf(S) == CHOOSE s \in [1..Cardinality(S) -> S] :
              \A i, j \in 1..Cardinality(S) : i # j => s[i] # s[j]

\* The sources that will be asked, in one fixed order: every ready one, and
\* no passed-over one.
Ordered(st) == SeqOf({s \in Sources : st[s] = "ready"})

\* Everything the world may be at the start: who holds what, who may read
\* what, who lies, how big each CAR is, and each source's standing.
Init ==
  /\ held \in [Sources -> SUBSET Heads]
  /\ readable \in [Sources -> SUBSET Heads]
  /\ liar \in [Sources -> BOOLEAN]
  /\ size \in [Heads -> Sizes]
  /\ standing \in [Sources -> Standings]
  /\ order = Ordered(standing)
  \* All heads start pending, in request order.
  /\ pending = [i \in Heads |-> i]
  /\ ask = <<>>
  /\ unserved = <<>>
  /\ result = [h \in Heads |-> "none"]
  /\ root = [h \in Heads |-> 0]
  /\ servedBy = [h \in Heads |-> "none"]
  /\ rounds = 0
  /\ failed = [s \in Sources |-> FALSE]
  /\ faulted = [s \in Sources |-> FALSE]
  /\ backedOff = [s \in Sources |-> FALSE]
  /\ stray = FALSE
  /\ done = FALSE
  \* Any source may have its wire on or off; nothing is ignored yet.
  /\ wireOn \in [Sources -> BOOLEAN]
  /\ ignored = FALSE

\* The source being asked now is the head of `order`.
Current == Head(order)

\* What the source does with one asked head, given the bytes left in this
\* reply. "skip" sends an empty entry; "stop" ends the reply before it;
\* "serve" sends its CAR. Not held, or not allowed, is an empty entry; a CAR
\* bigger than a whole reply is an empty entry too (the "oversize" bug stops
\* the reply there instead); a CAR that does not fit what is left stops the
\* reply, so the head is asked for again.
Verdict(s, h, left) ==
  IF h \notin held[s] THEN "skip"
  ELSE IF h \notin readable[s] /\ Bug # "access" THEN "skip"
  ELSE IF size[h] > Budget THEN (IF Bug = "oversize" THEN "stop" ELSE "skip")
  ELSE IF size[h] > left THEN "stop"
  ELSE "serve"

\* The reply an honest source builds for the asked heads `a`, as a sequence of
\* entries: 0 for empty, otherwise the CAR's root. Walk the heads in order,
\* spending the budget, until one says "stop" or the heads run out.
RECURSIVE Build(_, _, _, _)
Build(s, a, left, acc) ==
  IF a = <<>> THEN acc
  ELSE LET h == Head(a)
           v == Verdict(s, h, left)
       IN IF v = "stop" THEN acc
          ELSE IF v = "skip" THEN Build(s, Tail(a), left, Append(acc, 0))
          ELSE Build(s, Tail(a), left - size[h], Append(acc, h))

\* The replies a source may send: the honest one, and for a liar also an
\* empty one, one with an extra entry tacked on the end, or one whose served
\* roots are swapped for the other head's.
Replies(s, a) ==
  LET honest == Build(s, a, Budget, <<>>)
  IN {honest} \cup
     (IF liar[s]
      THEN {<<>>, Append(honest, 1)} \cup
           {[i \in 1..Len(honest) |->
               IF honest[i] = 0 THEN 0 ELSE (honest[i] % N) + 1] }
      ELSE {})

\* Start asking the current source: every pending head is asked.
Ask ==
  /\ ~done /\ order # <<>> /\ pending # <<>> /\ ask = <<>> /\ rounds = 0
  /\ ask' = pending
  /\ unserved' = <<>>
  /\ UNCHANGED <<held, readable, liar, size, standing, order, pending,
                 result, root, servedBy, rounds, failed, faulted, backedOff,
                 stray, done, wireOn, ignored>>

\* Move on to the next source with what this one left unserved. The heads
\* still pending are exactly those.
NextSource ==
  /\ ~done /\ order # <<>> /\ ask = <<>> /\ rounds > 0
  /\ order' = Tail(order)
  /\ pending' = unserved
  /\ unserved' = <<>>
  /\ rounds' = 0
  /\ UNCHANGED <<held, readable, liar, size, standing, ask, result, root,
                 servedBy, failed, faulted, backedOff, stray, done, wireOn,
                 ignored>>

\* The request fails. A fault (timeout, dial, reset) backs the source off; a
\* cancellation is the requester's own doing and does not, unless the
\* "blame" bug is planted. Either way every asked head goes back to the
\* unserved list, unless the "fallback" bug drops them.
Fail ==
  /\ ~done /\ ask # <<>>
  /\ failed' = [failed EXCEPT ![Current] = TRUE]
  /\ \E kind \in {"fault", "cancel"} :
       /\ faulted' = [faulted EXCEPT ![Current] = @ \/ kind = "fault"]
       /\ backedOff' = [backedOff EXCEPT ![Current] =
                          @ \/ kind = "fault" \/ Bug = "blame"]
  /\ unserved' = IF Bug = "fallback" THEN unserved ELSE unserved \o ask
  /\ ask' = <<>>
  /\ rounds' = rounds + 1
  /\ UNCHANGED <<held, readable, liar, size, standing, order, pending,
                 result, root, servedBy, stray, done, wireOn, ignored>>

\* Whether entry e of a reply is usable for head h: not empty, and rooted at
\* h (the "root" bug skips the root check).
Usable(e, h) == e # 0 /\ (e = h \/ Bug = "root")

\* The source answers. A reply longer than the ask, or (unless the "empty"
\* bug is planted) one with no entries, is a bad reply: the asked heads go
\* back to the unserved list and this source is done. Otherwise each entry
\* is matched to the head at the same position: usable entries are
\* imported, the rest go to the unserved list, and heads past the end of the
\* reply stay asked for the next round.
Reply ==
  /\ ~done /\ ask # <<>>
  \* A source answers whatever its own switch says (the "wireoff" bug keeps
  \* a switched-off source silent).
  /\ (wireOn[Current] \/ Bug # "wireoff")
  /\ \E r \in Replies(Current, ask) :
       IF (Len(r) > Len(ask) /\ Bug # "length")
          \/ (Len(r) = 0 /\ Bug # "empty")
       THEN /\ unserved' = unserved \o ask
            /\ ask' = <<>>
            /\ UNCHANGED <<result, root, servedBy, stray>>
       ELSE
         LET k == Len(r)
             m == IF k < Len(ask) THEN k ELSE Len(ask)
             got == {i \in 1..m : Usable(r[i], ask[i])}
             miss == SelectSeq([i \in 1..m |-> i], LAMBDA i : i \notin got)
         IN /\ stray' = (stray \/ k > Len(ask))
            /\ result' = [h \in Heads |->
                            IF \E i \in got : ask[i] = h THEN "car"
                            ELSE result[h]]
            /\ root' = [h \in Heads |->
                          IF \E i \in got : ask[i] = h
                          THEN r[CHOOSE i \in got : ask[i] = h]
                          ELSE root[h]]
            /\ servedBy' = [h \in Heads |->
                              IF \E i \in got : ask[i] = h THEN Current
                              ELSE servedBy[h]]
            /\ unserved' = unserved \o [j \in 1..Len(miss) |-> ask[miss[j]]]
            /\ ask' = [j \in 1..(Len(ask) - m) |-> ask[m + j]]
  /\ rounds' = rounds + 1
  /\ UNCHANGED <<held, readable, liar, size, standing, order, pending,
                 failed, faulted, backedOff, done, wireOn, ignored>>

\* The "wireoff" bug only: a source whose wire is off reads Go's request as
\* nothing and never answers, so the requester waits out its timeout and
\* counts a fault against a healthy source. The asked heads go back to the
\* unserved list, as for any failure.
Ignore ==
  /\ Bug = "wireoff" /\ ~done /\ ask # <<>> /\ ~wireOn[Current]
  /\ ignored' = TRUE
  /\ failed' = [failed EXCEPT ![Current] = TRUE]
  /\ faulted' = [faulted EXCEPT ![Current] = TRUE]
  /\ backedOff' = [backedOff EXCEPT ![Current] = TRUE]
  /\ unserved' = unserved \o ask
  /\ ask' = <<>>
  /\ rounds' = rounds + 1
  /\ UNCHANGED <<held, readable, liar, size, standing, order, pending,
                 result, root, servedBy, stray, done, wireOn>>

\* The heads in a sequence, as a set.
Range(s) == {s[i] : i \in DOMAIN s}

\* No sources left, or nothing left to ask: every head still pending goes to
\* the DAG walk, and the fetch is done. A head neither imported nor pending
\* was dropped somewhere and stays "none": nobody will fetch it.
Finish ==
  /\ ~done /\ ask = <<>>
  /\ order = <<>> \/ (pending = <<>> /\ rounds = 0)
  /\ result' = [h \in Heads |->
                  IF result[h] = "car" THEN "car"
                  ELSE IF h \in Range(pending) THEN "walk"
                  ELSE "none"]
  /\ done' = TRUE
  /\ UNCHANGED <<held, readable, liar, size, standing, order, pending, ask,
                 unserved, root, servedBy, rounds, failed, faulted, backedOff,
                 stray, wireOn, ignored>>

\* Every step the protocol can take.
Next == Ask \/ NextSource \/ Fail \/ Reply \/ Ignore \/ Finish

\* Every imported CAR is rooted at the head it was imported for.
INV_RootMatches == \A h \in Heads : result[h] = "car" => root[h] = h

\* A source never hands over a head this requester may not read.
INV_NoLeak ==
  \A h \in Heads : result[h] = "car" => h \in readable[servedBy[h]]

\* No reply entry is ever matched past the heads that were asked.
INV_NoStrayEntry == ~stray

\* One source is asked at most once per head, so asking always ends.
INV_RoundsBounded == rounds <= N

\* When the fetch ends, every head was imported or handed to the DAG walk.
INV_Accounted == done => \A h \in Heads : result[h] \in {"car", "walk"}

\* A source is held back only after a failure that was its own.
INV_BlameOnlyFaults == \A s \in Sources : backedOff[s] => faulted[s]

\* A source that is not passed over, holds the head, may give it, fits it in
\* a reply, tells the truth and never fails, ends up serving it (or another
\* source did first).
INV_Served ==
  done =>
    \A h \in Heads :
      (\E s \in Sources :
         /\ standing[s] # "passedOver" /\ h \in held[s] /\ h \in readable[s]
         /\ size[h] <= Budget /\ ~liar[s] /\ ~failed[s])
      => result[h] = "car"

\* No source ever leaves a request unanswered because of its own switch: a
\* Shinzo node never waits out a timeout on a Rust peer that has the wire off.
INV_EveryNodeAnswers == ~ignored

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
