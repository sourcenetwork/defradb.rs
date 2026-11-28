---- MODULE RetryRegistrationFence ----
\* The story. A node commits a document and owes it to every replicator peer
\* it lists. Before it queues a push for a peer it registers a durable retry
\* marker for that peer, so a lost push is retried later. Each registration
\* first takes a number (a token) from a shared counter, then writes the
\* marker in a transaction of its own. Taking the number and writing the
\* marker are two steps, so tokens can be written out of order.
\*
\* Who acts:
\*   - the committing write (crates/database/p2p/src/sync/coordinator/broadcast.rs
\*     admit_replicator_push) lists the peers, asks for every peer's
\*     registration at once, waits for all of them, and queues a push for each
\*     peer whose scope is owed by its marker or a newer one;
\*   - each registration (crates/engine/storage/src/stores/peerstore.rs
\*     RegisterScopeMarkerAttempt::run) writes its marker, or is rejected, or
\*     fails on a storage fault;
\*   - other writers register markers for the same peers;
\*   - an operator removes a peer, adds it back, or rewrites it
\*     (delete_replicator, create_replicator), which gives it a new incarnation.
\*
\* What must never happen:
\*   - a marker lands on a peer incarnation other than the one its
\*     registration read, or lands below a newer marker
\*     (ACT_RegistrationCannotCrossFence);
\*   - once the write is done, a listed peer has no decision, or the queued
\*     pushes are not exactly the peers whose scope is owed
\*     (INV_FanOutPeerLocal).
\*
\* The registrations of one write run at once: nothing here waits for one
\* peer's registration before asking for the next. The write waits only for
\* all of them together before it queues.
\*
\* State and its code (crates/engine/storage/src/stores/peerstore.rs unless named):
\*   present[p]     the ReplicatorKey row (create_replicator, delete_replicator)
\*   epoch[p]       the ReplicatorIncarnationKey value; only equality is read
\*   allocated      the /rep/retry/marker-generation sequence
\*   staged         RegisterScopeMarkerAttempts allocated and not yet decided;
\*                  owner[g], captured[g] are their peer_id, expected_incarnation
\*   marker[p]      the scope's retry marker generation, 0 when absent
\*   phase, listed  admit_replicator_push before, inside and after its fan-out
\*   mine[p]        the generation the fan-out allocated for p's job, 0 for none
\*   outcome[p]     p's job: undecided, marker_generation set, or in unregistered
\*   queued         the jobs passed to enqueue_replicator_push

\* Naturals gives numbers; FiniteSets gives Cardinality, the size of a set.
EXTENDS Naturals, FiniteSets

\* The knobs each configuration sets.
CONSTANT Peers,      \* the replicator peers the write may list
         Background, \* how many registrations other writers may make
         MaxEpoch,   \* the highest incarnation a peer may reach
         Faults,     \* whether a registration may fail on a storage fault
         CheckEpoch, \* guard: a registration lands only on the incarnation it read
         CheckOrder, \* guard: a registration never lands below a newer marker
         KeepFallbackFence, \* guard: a failed batch keeps its staged registration
         FanOut      \* how the write treats its registrations (see below)

\* Faults is on or off.
ASSUME /\ Faults \in BOOLEAN /\ KeepFallbackFence \in BOOLEAN
\* "PerPeer" is the code: each peer decided on its own, queued after all are
\* decided. "AbortOnFirstFail" stops at the first rejection. "NoJoin" queues
\* before every registration is decided.
ASSUME FanOut \in {"PerPeer", "AbortOnFirstFail", "NoJoin"}

\* The model's state, described in the table above.
VARIABLES present, epoch, allocated, owner, staged, captured, marker,
          phase, listed, mine, outcome, queued

\* Every variable, for stuttering steps and fairness.
vars == <<present, epoch, allocated, owner, staged, captured, marker,
          phase, listed, mine, outcome, queued>>

\* Every token that can ever be handed out: one per background registration
\* and one per peer the write registers.
Tokens == 1..(Background + Cardinality(Peers))

\* The owner of a token not handed out yet.
NoPeer == "none"

\* How many tokens the write itself has taken, one per peer it registered.
FanTokens == Cardinality({p \in Peers : mine[p] # 0})

\* The variables of the markers and their registrations.
fenceVars == <<present, epoch, allocated, owner, staged, captured, marker>>

\* The variables of the committing write.
fanVars == <<phase, listed, mine, outcome, queued>>

\* At the start every peer is present at incarnation 1, nothing is allocated,
\* no marker exists, and the write has not listed its peers yet.
Init == /\ present = [p \in Peers |-> TRUE]          \* every peer is configured
        /\ epoch = [p \in Peers |-> 1]               \* at its first incarnation
        /\ allocated = 0                             \* no token taken yet
        /\ owner = [g \in Tokens |-> NoPeer]         \* no token belongs to anyone
        /\ staged = {}                               \* no registration in flight
        /\ captured = [g \in Tokens |-> 0]           \* no incarnation read yet
        /\ marker = [p \in Peers |-> 0]              \* no marker yet
        /\ phase = "idle"                            \* the write has not begun
        /\ listed = {}                               \* and has listed no peer
        /\ mine = [p \in Peers |-> 0]                \* nor taken any token
        /\ outcome = [p \in Peers |-> "none"]        \* nor decided any peer
        /\ queued = {}                               \* nor queued any push

\* Take the next token for peer p and start its registration: the token is
\* p's, and the registration remembers the incarnation it read.
Stage(p) == /\ allocated' = allocated + 1                                \* the counter moves on
            /\ owner' = [owner EXCEPT ![allocated + 1] = p]              \* the new token is p's
            /\ staged' = staged \cup {allocated + 1}                     \* its registration is in flight
            /\ captured' = [captured EXCEPT ![allocated + 1] = epoch[p]] \* reading p's incarnation now

\* Another writer's registration for the same peer scope begins.
Allocate(p) == /\ present[p]                              \* only for a configured peer
               /\ allocated - FanTokens < Background       \* within the background budget
               /\ Stage(p)                                \* takes a token and starts
               /\ UNCHANGED <<present, epoch, marker>>     \* nothing durable changes yet
               /\ UNCHANGED fanVars                        \* the write is not involved

\* delete_replicator removes peer p and its marker.
Forget(p) == /\ present[p]                                              \* p is configured
             /\ present' = [present EXCEPT ![p] = FALSE]                 \* its row goes
             /\ marker' = [marker EXCEPT ![p] = 0]                       \* and its marker with it
             /\ UNCHANGED <<epoch, allocated, owner, staged, captured>>  \* tokens are untouched
             /\ UNCHANGED fanVars                                        \* the write is not involved

\* create_replicator adds a removed peer p back under a new incarnation.
Readd(p) == /\ ~present[p]                                               \* p was removed
            /\ epoch[p] < MaxEpoch                                       \* within the bound
            /\ present' = [present EXCEPT ![p] = TRUE]                   \* its row is back
            /\ epoch' = [epoch EXCEPT ![p] = @ + 1]                      \* at a new incarnation
            /\ UNCHANGED <<allocated, owner, staged, captured, marker>>  \* nothing else moves
            /\ UNCHANGED fanVars                                         \* the write is not involved

\* create_replicator rewrites a present peer's incarnation and keeps its markers.
Reincarnate(p) == /\ present[p]                                                  \* p is configured
                  /\ epoch[p] < MaxEpoch                                         \* within the bound
                  /\ epoch' = [epoch EXCEPT ![p] = @ + 1]                        \* a new incarnation
                  /\ UNCHANGED <<present, allocated, owner, staged, captured, marker>> \* nothing else moves
                  /\ UNCHANGED fanVars                                           \* the write is not involved

\* Whether token g's registration may land now: its peer is present, still
\* at the incarnation it read (when that guard is on), and its token is not
\* older than the peer's current marker (when that guard is on).
Fence(g) == LET p == owner[g] IN
              /\ present[p]                               \* the peer still exists
              /\ (~CheckEpoch \/ captured[g] = epoch[p])   \* same incarnation it read
              /\ (~CheckOrder \/ g >= marker[p])           \* not below a newer marker

\* When token g is the write's own token for its peer, record that peer's
\* decision; another writer's token leaves the write's decisions alone.
Decide(g, result) == outcome' = IF mine[owner[g]] = g
                                  THEN [outcome EXCEPT ![owner[g]] = result]
                                  ELSE outcome

\* Token g's registration passes its fence and writes the marker.
Commit(g) == /\ g \in staged                                      \* it is in flight
             /\ Fence(g)                                          \* and allowed to land
             /\ marker' = [marker EXCEPT ![owner[g]] = g]          \* the marker is written
             /\ staged' = staged \ {g}                             \* it is no longer in flight
             /\ Decide(g, "registered")                           \* the write learns it landed
             /\ UNCHANGED <<present, epoch, allocated, owner, captured>> \* nothing else moves
             /\ UNCHANGED <<phase, listed, mine, queued>>          \* the write only learns

\* RED only: after a failed batch, a fresh single call can read the new
\* incarnation and leave a marker for a peer the original call did not see.
\* A guarded retry already uses Commit and Reject with the original token.
FallbackRebind(g) ==
  /\ ~KeepFallbackFence                                      \* only when the guard is removed
  /\ g \in staged                                            \* the original registration is in flight
  /\ LET p == owner[g] IN                                    \* the peer it was sent to
       /\ present[p] /\ captured[g] # epoch[p]                \* a replacement incarnation exists
       /\ g >= marker[p]                                     \* the old token can expose the fence breach
       /\ marker' = [marker EXCEPT ![p] = g]                   \* a fresh call leaves a marker there
  /\ staged' = staged \ {g}                                   \* the registration is now decided
  /\ Decide(g, "registered")                                \* the write learns it landed
  /\ UNCHANGED <<present, epoch, allocated, owner, captured>> \* keep the original incarnation as witness
  /\ UNCHANGED <<phase, listed, mine, queued>>                \* the write only learns

\* Token g fails its fence and writes nothing. If a newer marker already
\* owes the scope under the same incarnation, the push still carries its
\* own token, which cannot clear the newer marker.
Reject(g) == /\ g \in staged                                      \* it is in flight
             /\ ~Fence(g)                                         \* and not allowed to land
             /\ staged' = staged \ {g}                             \* it is no longer in flight
             /\ LET p == owner[g] IN                             \* which peer this was for
                  Decide(g, IF present[p] /\ captured[g] = epoch[p] /\ g < marker[p]
                            THEN "registered" ELSE "unregistered") \* owed by a newer marker or refused
             /\ UNCHANGED <<present, epoch, allocated, owner, captured, marker>> \* nothing durable moves
             /\ UNCHANGED <<phase, listed, mine, queued>>          \* the write only learns

\* The write's own registration for a peer fails on a storage fault.
Fault(g) == /\ Faults                                              \* faults are possible
            /\ g \in staged                                       \* it is in flight
            /\ mine[owner[g]] = g                                 \* and it is the write's own
            /\ staged' = staged \ {g}                              \* it is no longer in flight
            /\ Decide(g, "unregistered")                          \* the write learns it failed
            /\ UNCHANGED <<present, epoch, allocated, owner, captured, marker>> \* nothing durable moves
            /\ UNCHANGED <<phase, listed, mine, queued>>           \* the write only learns

\* broadcast.rs push_jobs: the write snapshots the replicator list.
List == /\ phase = "idle"                                  \* the write has not begun
        /\ phase' = "register"                              \* it starts registering
        /\ listed' = {p \in Peers : present[p]}            \* every peer configured now
        /\ UNCHANGED fenceVars                             \* nothing durable moves
        /\ UNCHANGED <<mine, outcome, queued>>             \* no decision yet

\* admit_replicator_push asks for peer p's registration. Every peer's request
\* goes out at once, so this does not wait for any other peer's outcome.
FanRegister(p) ==
  /\ phase = "register"                                         \* the write is registering
  /\ p \in listed                                               \* p is one of its peers
  /\ outcome[p] = "none"                                        \* p is not decided
  /\ mine[p] = 0                                                \* and not asked for yet
  /\ FanOut # "AbortOnFirstFail" \/ \A q \in listed : outcome[q] # "unregistered" \* RED: stop after a failure
  /\ IF present[p]
       THEN /\ Stage(p)                                         \* a present peer takes a token
            /\ mine' = [mine EXCEPT ![p] = allocated + 1]       \* which is the write's for p
            /\ UNCHANGED outcome                                \* still undecided
       ELSE /\ outcome' = [outcome EXCEPT ![p] = "unregistered"] \* a gone peer fails at once
            /\ UNCHANGED <<allocated, owner, staged, captured, mine>> \* and takes no token
  /\ UNCHANGED <<present, epoch, marker, phase, listed, queued>>  \* nothing else moves

\* RED only: the write gives up at its first failed peer.
AbortFanOut == /\ FanOut = "AbortOnFirstFail"                   \* only in that configuration
               /\ phase = "register"                            \* while registering
               /\ \E p \in listed : outcome[p] = "unregistered" \* some peer failed
               /\ phase' = "done"                               \* the write stops there
               /\ UNCHANGED fenceVars                           \* nothing durable moves
               /\ UNCHANGED <<listed, mine, outcome, queued>>   \* and nothing is queued

\* The write queues a push for every peer whose scope is owed. In the
\* code it waits for every registration together first (join_all); "NoJoin"
\* plants the bug of queueing while some are still undecided.
Enqueue == /\ phase = "register"                                          \* while registering
           /\ FanOut = "NoJoin" \/ \A p \in listed : outcome[p] # "none"  \* every peer decided
           /\ FanOut # "AbortOnFirstFail" \/ \A p \in listed : outcome[p] = "registered" \* RED only
           /\ queued' = {p \in listed : outcome[p] = "registered"}       \* queue the registered peers
           /\ phase' = "done"                                             \* the write is done
           /\ UNCHANGED fenceVars                                         \* nothing durable moves
           /\ UNCHANGED <<listed, mine, outcome>>                         \* decisions stand

\* One step of the committing write: list, ask, learn its own outcome, or finish.
FanStep == \/ List                                                                   \* list the peers
           \/ (\E p \in Peers : FanRegister(p))                                      \* ask for one
           \/ (\E p \in Peers : mine[p] \in staged /\ (Commit(mine[p]) \/ Reject(mine[p]))) \* learn one
           \/ AbortFanOut                                                            \* RED: give up
           \/ Enqueue                                                                \* queue and finish

\* Every step the system can take: the write, the operator, other writers,
\* and any registration landing, failing its fence, or faulting.
Next == \/ FanStep                                                            \* the write moves
        \/ \E p \in Peers : Allocate(p) \/ Forget(p) \/ Readd(p) \/ Reincarnate(p) \* others act
        \/ \E g \in Tokens : Commit(g) \/ Reject(g) \/ Fault(g) \/ FallbackRebind(g)                \* a registration ends

\* The write keeps moving while it can; nothing else is forced to.
Spec == Init /\ [][Next]_vars /\ WF_vars(FanStep)

\* Every variable holds a value of the kind it should.
TypeOK == /\ present \in [Peers -> BOOLEAN]                             \* peers are in or out
          /\ epoch \in [Peers -> 1..MaxEpoch]                           \* incarnations are bounded
          /\ allocated \in 0..Cardinality(Tokens)                       \* the counter is bounded
          /\ owner \in [Tokens -> Peers \cup {NoPeer}]                  \* a token has one owner
          /\ staged \subseteq Tokens                                    \* in flight are tokens
          /\ captured \in [Tokens -> 0..MaxEpoch]                       \* incarnations read
          /\ marker \in [Peers -> 0..Cardinality(Tokens)]               \* markers are tokens
          /\ phase \in {"idle", "register", "done"}                     \* the write's phases
          /\ listed \subseteq Peers                                     \* listed are peers
          /\ mine \in [Peers -> 0..Cardinality(Tokens)]                 \* the write's tokens
          /\ outcome \in [Peers -> {"none", "registered", "unregistered"}] \* its decisions
          /\ queued \subseteq Peers                                     \* queued are peers

\* A registration lands only on the incarnation it captured, never below the marker.
ACT_RegistrationCannotCrossFence ==
  [][\A g \in staged : marker'[owner[g]] = g =>          \* whenever token g's marker lands,
       /\ captured[g] = epoch[owner[g]]                  \* its peer is at the incarnation it read
       /\ g >= marker[owner[g]]]_vars                    \* and it is not older than the marker

\* Each listed peer is decided on its own; exactly the registered ones are queued.
INV_FanOutPeerLocal ==
  phase = "done" => \A p \in listed : outcome[p] # "none"                \* every peer decided
                                      /\ (p \in queued <=> outcome[p] = "registered") \* queued iff landed

\* The write always finishes.
LIVE_FanOutDecides == <>(phase = "done")
====
