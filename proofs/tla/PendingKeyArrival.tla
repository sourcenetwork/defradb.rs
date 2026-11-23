---- MODULE PendingKeyArrival ----
\* A verified key arrival can resolve any blocked pending-DAG root, so the
\* listener empties every retry backoff into the due queue. The clock still
\* owns dispatch, and a root being dispatched does not resurrect a promoted
\* root after it leaves the queue.

EXTENDS Naturals

CONSTANTS
  Promotes,      \* TRUE: key arrival moves every backoff root due
  KeepsPromotion, \* TRUE: an in-flight promotion survives failure
  MaxArrivals    \* bound key-arrival events to keep the state space finite

Roots == {1, 2}

VARIABLES
  due,       \* roots ready for the retry clock
  backoff,   \* roots waiting out their retry deadline
  attempts,  \* roots taken from the queue by an in-flight fetch
  merged,    \* roots whose DAG has merged
  promoted,  \* in-flight roots promoted since claim
  failed,    \* whether the modeled failure occurred
  arrivals   \* number of key arrivals seen

vars == <<due, backoff, attempts, merged, promoted, failed, arrivals>>

TypeOK ==
  /\ due \subseteq Roots
  /\ backoff \subseteq Roots
  /\ merged \subseteq Roots
  /\ due \cap backoff = {}
  /\ due \cap merged = {}
  /\ backoff \cap merged = {}
  /\ \A r \in Roots :
       (attempts[r] = 1) <=> r \notin (due \cup backoff \cup merged)
  /\ attempts \in [Roots -> 0..1]
  /\ merged \subseteq Roots
  /\ promoted \subseteq Roots
  /\ failed \in BOOLEAN
  /\ arrivals \in 0..MaxArrivals

Init ==
  /\ backoff = Roots
  /\ due = {}
  /\ attempts = [r \in Roots |-> 0]
  /\ merged = {}
  /\ promoted = {}
  /\ failed = FALSE
  /\ arrivals = 0

KeyArrival ==
  /\ arrivals < MaxArrivals
  /\ arrivals' = arrivals + 1
  /\ IF Promotes
     THEN /\ due' = due \cup backoff
          /\ backoff' = {}
          /\ promoted' = promoted \cup {r \in Roots : attempts[r] = 1}
     ELSE /\ UNCHANGED due
          /\ UNCHANGED <<backoff, promoted>>
  /\ UNCHANGED <<attempts, merged, failed>>

Fetch(r) ==
  /\ r \in due
  /\ attempts[r] = 0
  /\ due' = due \ {r}
  /\ attempts' = [attempts EXCEPT ![r] = 1]
  /\ promoted' = promoted \ {r}
  /\ UNCHANGED <<backoff, merged, failed, arrivals>>

Merge(r) ==
  /\ attempts[r] = 1
  /\ r \notin due
  /\ r \notin merged
  /\ merged' = merged \cup {r}
  /\ attempts' = [attempts EXCEPT ![r] = 0]
  /\ promoted' = promoted \ {r}
  /\ UNCHANGED <<due, backoff, failed, arrivals>>

\* An in-flight promotion survives a failed fetch's re-arm.
Fail(r) ==
  /\ attempts[r] = 1 /\ r \in promoted /\ ~failed
  /\ attempts' = [attempts EXCEPT ![r] = 0]
  /\ promoted' = promoted \ {r}
  /\ failed' = TRUE
  /\ due' = IF KeepsPromotion THEN due \cup {r} ELSE due
  /\ backoff' = IF KeepsPromotion THEN backoff ELSE backoff \cup {r}
  /\ UNCHANGED <<merged, arrivals>>

Next ==
  \/ KeyArrival
  \/ \E r \in Roots : Fetch(r) \/ Merge(r) \/ Fail(r)

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

LIVE_PromotedBackoffDrains == <>(backoff = {})

====
