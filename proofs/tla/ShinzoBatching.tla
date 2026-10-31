---------------------------- MODULE ShinzoBatching ----------------------------
\* The story. A node speaking Go Shinzo's wire does two things with batches.
\*
\* Sending: each document it writes is added to its collection topic's
\* batch. A batch is published as soon as it holds BatchMax documents, and
\* otherwise when a clock tick finds that its first document has waited
\* FlushInterval ticks. Publishing can fail. A failure that can pass (no
\* subscribed peer yet, full send queues) keeps the batch to try again,
\* until it has failed MaxTries times, as a single update's gossip is
\* retried; then, or on a failure that never passes (a batch that will not
\* encode), the batch is dropped, and its documents reach peers through
\* replicators and document sync instead.
\*
\* Receiving: a CAR that arrives inline with an update is kept for its head
\* and the peer that offered it, within a byte budget, until the node asks
\* that peer for that head; then it is used, once, in place of a network
\* request. A kept CAR can also expire.
\*
\* What must never happen:
\*   - a published batch holds more than BatchMax documents;
\*   - a document waits in a batch past FlushInterval;
\*   - a document is lost or published twice: each one is published, still
\*     waiting, kept to retry, or dropped, exactly once;
\*   - a batch is dropped over a failure that can pass before it has used
\*     every try;
\*   - the kept CARs are over the byte budget, or the byte count drifts from
\*     what is really kept;
\*   - one offered CAR is used twice;
\*   - a CAR is used for a peer other than the one that offered it.
\*
\* Rust: crates/p2p/src/sync/shinzo_batch.rs ShinzoBatcher::add,
\* ShinzoBatcher::due (run on the host's flush tick), ShinzoBatcher::retry;
\* crates/p2p/src/host/p2p_host/shinzo_batch.rs P2PHost::publish_batch;
\* InlineCars::offer,
\* InlineCars::take, InlineCars::expire. Go: shinzo-develop
\* internal/db/p2p/batcher.go.
EXTENDS Naturals, Sequences, FiniteSets

\* Each Bug value removes exactly one guard, so its RED config must fail on
\* the invariant named next to it. "none" keeps every guard (GREEN).
\*   "overfull" publish a full batch only on the tick      -> INV_BatchBounded
\*   "noflush"  never publish a batch that is not full     -> INV_NoLongWait
\*   "budget"   keep a CAR whatever the bytes already kept -> INV_WithinBudget
\*   "count"    forget to count a replaced CAR's bytes out -> INV_BytesMatch
\*   "reuse"    leave a CAR in the store after using it    -> INV_UsedOnce
\*   "anypeer"  use a CAR whoever is asked for its head    -> INV_OwnPeer
\*   "nokeep"   drop a batch on its first passing failure  -> INV_RetryableKept
CONSTANT Bug

\* Sending side: three documents, batches of two, a two-tick flush, and
\* enough ticks to see a batch wait it out.
Docs == 1..3
BatchMax == 2
FlushInterval == 2
MaxTime == 4
\* A batch is tried at most this many times before it is dropped.
MaxTries == 2

\* Receiving side: one head, two peers, CARs of size 1 or 2, a budget of 2,
\* and at most three offers in all.
Heads == {"h"}
Peers == {"p", "q"}
Sizes == {1, 2}
Budget == 2
MaxOffers == 3

\* now: the clock, in ticks.
\* toAdd: the documents not written yet.
\* pending: the open batch, oldest first; since: when its first one came.
\* published: every batch published so far.
\* retrying: batches kept after a failure that can pass, as <<batch, failures>>.
\* dropped: batches given up on, as <<batch, failures, why>>, why being
\*   "retryable" (it used every try) or "permanent".
\* store: the kept CARs, a function from (head, peer) to <<id, size>>.
\* bytes: the byte count the store keeps.
\* nextId: the id the next offered CAR gets.
\* used: every use of a CAR, as <<id, offerer, asker, n>>, n counting uses so
\*   two uses of one offer stay two entries.
VARIABLES now, toAdd, pending, since, published, retrying, dropped, store,
          bytes, nextId, used

\* Every variable, for UNCHANGED.
vars == <<now, toAdd, pending, since, published, retrying, dropped, store,
          bytes, nextId, used>>
\* The sending side's variables, and the receiving side's.
sendVars == <<now, toAdd, pending, since, published, retrying, dropped>>
recvVars == <<store, bytes, nextId, used>>

\* The sum of a function's values' sizes (the second item of each value).
RECURSIVE SumOver(_, _)
SumOver(f, keys) ==
  IF keys = {} THEN 0
  ELSE LET k == CHOOSE k \in keys : TRUE
       IN f[k][2] + SumOver(f, keys \ {k})
StoreBytes == SumOver(store, DOMAIN store)

\* Nothing written, nothing kept, at time zero.
Init ==
  /\ now = 0
  /\ toAdd = Docs
  /\ pending = <<>>
  /\ since = 0
  /\ published = <<>>
  /\ retrying = {}
  /\ dropped = {}
  /\ store = [k \in {} |-> <<0, 0>>]
  /\ bytes = 0
  /\ nextId = 1
  /\ used = {}

\* ---- Sending ----

\* Try to publish `batch`, which has failed `f` times so far, with `kept` the
\* retrying set to start from. It is published, or fails in a way that can
\* pass and is kept to retry (dropped once it has failed MaxTries times; the
\* "nokeep" bug drops it at once), or fails for good and is dropped.
Attempt(batch, f, kept) ==
  \/ /\ published' = Append(published, batch)
     /\ retrying' = kept
     /\ UNCHANGED dropped
  \/ /\ IF f + 1 < MaxTries /\ Bug # "nokeep"
        THEN /\ retrying' = kept \cup {<<batch, f + 1>>}
             /\ UNCHANGED dropped
        ELSE /\ dropped' = dropped \cup {<<batch, f + 1, "retryable">>}
             /\ retrying' = kept
     /\ UNCHANGED published
  \/ /\ dropped' = dropped \cup {<<batch, f, "permanent">>}
     /\ retrying' = kept
     /\ UNCHANGED published

\* Write document d: it joins the open batch, which starts its wait when it
\* was empty. A batch that is now full is published at once (the "overfull"
\* bug leaves it for the tick, and lets it keep growing).
Add ==
  \E d \in toAdd :
    LET batch == Append(pending, d)
    IN /\ toAdd' = toAdd \ {d}
       /\ since' = IF pending = <<>> THEN now ELSE since
       /\ IF Len(batch) >= BatchMax /\ Bug # "overfull"
          THEN /\ Attempt(batch, 0, retrying)
               /\ pending' = <<>>
          ELSE /\ pending' = batch
               /\ UNCHANGED <<published, retrying, dropped>>
       /\ UNCHANGED <<now>> /\ UNCHANGED recvVars

\* The clock ticks, and the flush that runs on it publishes the open batch
\* if its first document has now waited FlushInterval (the "noflush" bug
\* never does).
Tick ==
  /\ now < MaxTime
  /\ now' = now + 1
  /\ IF pending # <<>> /\ now + 1 - since >= FlushInterval /\ Bug # "noflush"
     THEN /\ Attempt(pending, 0, retrying)
          /\ pending' = <<>>
     ELSE UNCHANGED <<published, pending, retrying, dropped>>
  /\ UNCHANGED <<toAdd, since>> /\ UNCHANGED recvVars

\* A kept batch's backoff has passed: try it again.
Retry ==
  \E r \in retrying :
    /\ Attempt(r[1], r[2], retrying \ {r})
    /\ UNCHANGED <<now, toAdd, pending, since>> /\ UNCHANGED recvVars

\* ---- Receiving ----

\* Peer p offers a CAR of size s for head h. Any CAR already kept for the
\* same head from the same peer is replaced, and its bytes counted out (the
\* "count" bug forgets to). As in the code, the CAR is kept only if it fits
\* the budget on top of everything kept, the one it replaces included (the
\* "budget" bug keeps it anyway).
Offer ==
  \E h \in Heads, p \in Peers, s \in Sizes :
    LET k == <<h, p>>
        freed == IF k \in DOMAIN store /\ Bug # "count" THEN store[k][2] ELSE 0
    IN /\ nextId <= MaxOffers
       /\ nextId' = nextId + 1
       /\ IF bytes + s <= Budget \/ Bug = "budget"
          THEN /\ store' = [x \in DOMAIN store \cup {k} |->
                              IF x = k THEN <<nextId, s>> ELSE store[x]]
               /\ bytes' = bytes - freed + s
          ELSE UNCHANGED <<store, bytes>>
       /\ UNCHANGED <<used>> /\ UNCHANGED sendVars

\* The node asks peer a for head h, and uses the CAR a offered for it, which
\* leaves the store (the "reuse" bug leaves it there). The "anypeer" bug uses
\* a CAR any peer offered for h.
Take ==
  \E h \in Heads, a \in Peers, p \in Peers :
    LET k == <<h, p>>
    IN /\ k \in DOMAIN store
       /\ p = a \/ Bug = "anypeer"
       /\ used' = used \cup {<<store[k][1], p, a, Cardinality(used) + 1>>}
       /\ IF Bug = "reuse"
          THEN UNCHANGED <<store, bytes>>
          ELSE /\ store' = [x \in DOMAIN store \ {k} |-> store[x]]
               /\ bytes' = bytes - store[k][2]
       /\ UNCHANGED <<nextId>> /\ UNCHANGED sendVars

\* A kept CAR outlives its time-to-live and is dropped.
Expire ==
  \E k \in DOMAIN store :
    /\ store' = [x \in DOMAIN store \ {k} |-> store[x]]
    /\ bytes' = bytes - store[k][2]
    /\ UNCHANGED <<nextId, used>> /\ UNCHANGED sendVars

\* Every step the node can take.
Next == Add \/ Tick \/ Retry \/ Offer \/ Take \/ Expire

\* ---- What must hold ----

\* The documents in a sequence of batches, counted with repeats.
RECURSIVE Count(_, _)
Count(d, bs) ==
  IF bs = <<>> THEN 0
  ELSE Cardinality({i \in DOMAIN Head(bs) : Head(bs)[i] = d}) + Count(d, Tail(bs))

\* No published batch holds more than BatchMax documents.
INV_BatchBounded == \A i \in DOMAIN published : Len(published[i]) <= BatchMax

\* An open batch's first document has waited less than FlushInterval.
INV_NoLongWait == pending # <<>> => now - since < FlushInterval

\* How often d appears in the batches of a set of records, each record's
\* batch being its first item.
RECURSIVE CountIn(_, _)
CountIn(d, rs) ==
  IF rs = {} THEN 0
  ELSE LET r == CHOOSE r \in rs : TRUE
       IN Count(d, <<r[1]>>) + CountIn(d, rs \ {r})

\* Every written document is in exactly one place: published once, still in
\* the open batch, kept to retry, or dropped.
INV_ExactlyOnce ==
  \A d \in Docs \ toAdd :
    Count(d, Append(published, pending)) + CountIn(d, retrying) + CountIn(d, dropped) = 1

\* A batch dropped over a failure that can pass had used every try.
INV_RetryableKept ==
  \A r \in dropped : r[3] = "retryable" => r[2] = MaxTries

\* The kept CARs never exceed the byte budget.
INV_WithinBudget == bytes <= Budget

\* The byte count is exactly the size of what is kept.
INV_BytesMatch == bytes = StoreBytes

\* No offered CAR is used twice: each use names a different offer.
INV_UsedOnce ==
  \A u, v \in used : u[1] = v[1] => u = v

\* A CAR is only ever used for the peer that offered it.
INV_OwnPeer == \A u \in used : u[2] = u[3]

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
