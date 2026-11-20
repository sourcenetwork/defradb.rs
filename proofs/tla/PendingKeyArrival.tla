---- MODULE PendingKeyArrival ----
\* A verified key arrival can resolve any blocked pending-DAG root, so the
\* listener empties every retry backoff into the due queue. The clock still
\* owns dispatch, and a root being dispatched does not resurrect a promoted
\* root after it leaves the queue.

EXTENDS Naturals

CONSTANTS
  Promotes,      \* TRUE: key arrival moves every backoff root due
  MaxArrivals    \* bound key-arrival events to keep the state space finite

Roots == {1, 2}

VARIABLES
  due,       \* roots ready for the retry clock
  backoff,   \* roots waiting out their retry deadline
  attempts,  \* roots taken from the queue by an in-flight fetch
  merged,    \* roots whose DAG has merged
  arrivals   \* number of key arrivals seen

vars == <<due, backoff, attempts, merged, arrivals>>

TypeOK ==
  /\ due \cup backoff = Roots
  /\ due \cap backoff = {}
  /\ attempts \in [Roots -> 0..1]
  /\ merged \subseteq Roots
  /\ arrivals \in 0..MaxArrivals

Init ==
  /\ backoff = Roots
  /\ due = {}
  /\ attempts = [r \in Roots |-> 0]
  /\ merged = {}
  /\ arrivals = 0

KeyArrival ==
  /\ arrivals < MaxArrivals
  /\ arrivals' = arrivals + 1
  /\ IF Promotes
     THEN /\ due' = due \cup backoff
          /\ backoff' = {}
     ELSE /\ UNCHANGED due
          /\ UNCHANGED backoff
  /\ UNCHANGED <<attempts, merged>>

Fetch(r) ==
  /\ r \in due
  /\ attempts[r] = 0
  /\ due' = due \ {r}
  /\ attempts' = [attempts EXCEPT ![r] = 1]
  /\ UNCHANGED <<backoff, merged, arrivals>>

Merge(r) ==
  /\ attempts[r] = 1
  /\ r \notin due
  /\ r \notin merged
  /\ merged' = merged \cup {r}
  /\ attempts' = [attempts EXCEPT ![r] = 0]
  /\ UNCHANGED <<due, backoff, arrivals>>

Next ==
  \/ KeyArrival
  \/ \E r \in Roots : Fetch(r) \/ Merge(r)

Spec == Init /\ [][Next]_vars

FairSpec ==
  /\ Spec
  /\ WF_vars(KeyArrival)
  /\ \A r \in Roots :
       /\ WF_vars(Fetch(r))
       /\ WF_vars(Merge(r))

INV_DuePartition == TypeOK

INV_KeyArrivalPromotes ==
  (arrivals > 0 /\ Promotes) => backoff = {}

LIVE_PromotedBackoffDrains == Promotes => <>(backoff = {})

====
