------------------------ MODULE SettleBeforeObserving ------------------------
\* The story. A transaction commits a relationship change on a document; the
\* change is owed to the external provider until the node's drainer settles
\* it. Moments later an observation of that relationship arrives (a projection
\* of the next write asks for it, or a caller asks directly). The provider
\* does not hold the change yet, so its answer would predate it.
\*
\* Instead of refusing at once, the observation waits at the network edge
\* for the change to settle: it is woken whenever a record of something owed
\* is written or deleted, and gives up once its patience is spent. When the
\* change has settled it asks the provider, which now holds it. A change that
\* is parked or refused never settles, so the wait gives up on it at once; a
\* provider that is down keeps the change owed, so the wait runs out.
\*
\* What must never happen: the observation answers from a provider state
\* older than the change it found owed; or it waits without bound.
\*
\* The code: ExternalDocumentAcp::observe_actor_relationship and
\* ExternalDocumentAcp::await_tuple in crates/engine/acp/src/external.rs, with
\* OBSERVATION_PATIENCE; the projection's prepared relationship input is
\* resolved through it at the edge (crates/database/db/src/write/prepare.rs),
\* and DocumentMutationContext::projected_actor_relationship keys that input
\* by the tuple's latest staged change (ops::tuple_head).
\* Only the standard operators on numbers are used.
EXTENDS Naturals

\* Bug: "none", "nowait" (the provider is asked while the change is owed), or
\* "unbounded" (the wait does not give up). Patience: how many wake-ups the
\* wait sits through.
CONSTANTS Bug, Patience

\* staged: the version of the latest change staged on the tuple. settled: the
\* version the provider holds. up: the provider is reachable. parked: the
\* latest change is parked or refused. phase: where the observation is.
\* needed: the version owed when the observation began. seen: the version
\* the provider answered at. waited: wake-ups sat through. answer: the outcome.
VARIABLES staged, settled, up, parked, phase, needed, seen, waited, answer

\* Every variable, for stuttering steps.
vars == <<staged, settled, up, parked, phase, needed, seen, waited, answer>>

\* A change is owed: the provider does not hold the latest staged one.
Owed == settled < staged

\* One change was just committed and is owed; the observation has not begun.
Init == /\ staged = 1 /\ settled = 0 /\ up \in BOOLEAN /\ parked \in BOOLEAN
        /\ phase = "idle" /\ needed = 0 /\ seen = 0 /\ waited = 0 /\ answer = "none"

\* The drainer settles the latest change, if the provider is up and the
\* change is not parked.
Settle == /\ Owed /\ up /\ ~parked
          /\ settled' = staged
          /\ UNCHANGED <<staged, up, parked, phase, needed, seen, waited, answer>>

\* The provider goes down or comes back.
Toggle == /\ up' = ~up
          /\ UNCHANGED <<staged, settled, parked, phase, needed, seen, waited, answer>>

\* The observation begins: it notes the version owed.
Begin == /\ phase = "idle" /\ phase' = "waiting" /\ needed' = staged
         /\ UNCHANGED <<staged, settled, up, parked, seen, waited, answer>>

\* A record changes while the change is still owed and not parked: the wait
\* is woken and sits on, while its patience lasts (without bound, with the bug).
Wake == /\ phase = "waiting" /\ Owed /\ ~parked
        /\ (waited < Patience \/ (Bug = "unbounded" /\ waited <= Patience))
        /\ waited' = waited + 1
        /\ UNCHANGED <<staged, settled, up, parked, phase, needed, seen, answer>>

\* The wait gives up: its patience is spent, or the change is parked.
GiveUp == /\ phase = "waiting" /\ Owed
          /\ ((waited >= Patience /\ Bug # "unbounded") \/ parked)
          /\ answer' = "unsettled" /\ phase' = "done"
          /\ UNCHANGED <<staged, settled, up, parked, needed, seen, waited>>

\* Nothing is owed (or, with the bug, regardless): the provider is asked.
Ask == /\ phase = "waiting" /\ (~Owed \/ Bug = "nowait")
       /\ seen' = settled /\ answer' = "answered" /\ phase' = "done"
       /\ UNCHANGED <<staged, settled, up, parked, needed, waited>>

\* Any of these, in any order.
Next == Settle \/ Toggle \/ Begin \/ Wake \/ GiveUp \/ Ask

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* An answer comes from a provider state that holds the change the
\* observation found owed.
INV_NotOlderThanOwed == answer = "answered" => seen >= needed

\* The wait sits through no more wake-ups than its patience.
INV_Bounded == waited <= Patience
=============================================================================
