---- MODULE DagReadiness ----
\* The story. A node holds one root block and wants to know whether the whole
\* DAG under it is here. It walks the root's children: a merged child is done,
\* a stored child is read, a child it does not hold is "missing" and gets
\* fetched. Reading a stored child can go wrong: the block can be deleted
\* between being linked and being read, the read can fail, the storage can
\* report a conflict, or the bytes can be corrupt. Reading the root block
\* itself from the store can go wrong the same ways: it can be gone, or the
\* read can fail.
\*
\* Who acts: the walker (find_all_missing_links), and the eight places that
\* ask it whether a DAG is complete (the start of a fetch, after a rooted CAR,
\* the selective loop, the final check, the DocSync re-merge, the PushLog
\* admission, the pending retry and the restore sweep), a sweep that retries
\* every root that was waiting on a block that just arrived, the push's own
\* closing sweep (a head registered with the frontier [root] waits on itself,
\* so that sweep retries it before the push replies), and the CAR size-limit
\* check, which decides whether a provider's "too big to send" notice names a
\* block this node still misses.
\*
\* What must never happen: a DAG is declared ready although one of its
\* blocks, or its root, was never read; a failed walk is taken as "nothing
\* missing"; a block that is not there is not reported missing; a pushed head
\* is acked without something that will finish it; a head that was durably
\* registered gets an error reply; one root's failed read stops the sweep
\* before the other waiting roots are retried; a storage conflict loses its
\* type, so nobody retries it; a local read fault is blamed on the provider.
\*
\* Code it models (crates/database/p2p/src/sync):
\*   manager/links.rs  find_all_missing_links (one get per unmerged child: Some is
\*     read, None is missing, Err fails the walk with its type kept) and
\*     frontier_or_unknown (a failed walk is the frontier [root])
\*   coordinator/dag_fetcher.rs  poll_fetch_dag (start: ready only on Ok([])),
\*     fetch_dag_attempt (rootedCar: an error is the frontier [root];
\*     walkLoop: an error ends the attempt Incomplete; final: an error is Incomplete)
\*   coordinator/event_handler/doc_sync.rs  handle_doc_sync_reply (docSync: an error
\*     sends the root to the fetch list instead of BlockReceived)
\*   coordinator/event_handler/car.rs  car_size_limit_disposition (car: a local
\*     fault reading the root or walking below it completes the response, so it
\*     is never recorded as the provider's failure)
\*   manager/process/pushlog.rs  process_block_inner (pushAdmission: only a pushed
\*     block that does not parse is refused; a walk error registers [root];
\*     the closing retry_pending_dags_waiting_on is PushClose)
\*   manager/process/pending_dag.rs  retry_pending_dag (pendingRetry: a root that is
\*     gone is the frontier [root]; a root read fault or a walk error is the
\*     frontier [root] and returns Ok(false)), retry_pending_dags_waiting_on
\*     (the sweep), and the restore walk in resync_persisted_pending_dags (restore:
\*     an error is the frontier [root])
\* start, walkLoop and restore already took an error as incomplete before
\* #2600; their RED configurations keep them that way. The other callers
\* (replication/handlers.rs dag_is_local, branchable_sync.rs) also never take
\* an error as complete.
\* One level of children stands for the whole DAG: the walk treats every
\* level the same way, and a child's children only add more of the same reads.
\* The CAR notice always names child 1: a notice on a block the node does not
\* need is the same as one on a child it holds.
EXTENDS Naturals, FiniteSets

CONSTANTS
  N,                    \* how many children the root links
  SkipNone,             \* RED: a child that reads as None is skipped, not missing
  DropErr,              \* RED: the walker drops a failed child read (main before #2600)
  ErrAsEmpty,           \* RED: the sites that take a failed walk as "nothing missing"
  ErrAbortsSweep,       \* RED: a failed walk in the pending retry returns its error
  ConflictStringified,  \* RED: a storage conflict comes back as a plain storage error
  RootFaultErrs,        \* RED: a root that is gone or fails to read makes the retry return an error
  CarFaultBlamesPeer    \* RED: a local fault in the CAR size-limit check fails the response

\* The children, walked in order.
Children == 1..N
\* The root itself as a frontier entry: 0 is no child's number.
Root == 0
\* What reading one child finds.
Outcomes == {"merged", "present", "absent", "deleted", "storageErr", "conflict", "corrupt"}
\* What reading the stored root block finds.
RootOutcomes == {"ok", "gone", "fault"}
\* The places that act on a walk's answer.
Sites == {"start", "rootedCar", "walkLoop", "final", "docSync", "pushAdmission", "pendingRetry", "restore"}
\* The sites that only read the answer (the push and the sweep have their own actions).
WalkSites == Sites \ {"pushAdmission", "pendingRetry"}
\* The two roots waiting on the block that just arrived: A is the root above,
\* B is a healthy root whose DAG is all here.
Waiters == {"A", "B"}

VARIABLES
  outcome,       \* what a read of each child finds (fixed for the run)
  rootRead,      \* what a read of the stored root block finds (fixed for the run)
  pushedParses,  \* whether the pushed root block itself parses
  decision,      \* what each site decided: "none", "ready", "fetch", "unknown" or "refused"
  registered,    \* whether the push durably registered the head
  pushReply,     \* what the push answered: "none", "ack" or "error"
  acked,         \* whether the push of the root was acked
  frontier,      \* the pending row's frontier after the push or the retry
  toRetry,       \* waiting roots the sweep has not retried yet
  retried,       \* waiting roots the sweep retried
  car            \* the CAR size-limit check: "none", "completed", "sizeLimit" or "peerFailure"
vars == <<outcome, rootRead, pushedParses, decision, registered, pushReply, acked,
          frontier, toRetry, retried, car>>

\* -------- The walk --------

\* A child the walk read as Some: it is stored and the read worked.
Read(c) == outcome[c] = "present"
\* A child the walk reports missing: absent, or deleted before its read
\* (unless the RED skip drops the deleted one).
Missing(c) == outcome[c] = "absent" \/ (outcome[c] = "deleted" /\ ~SkipNone)
\* A child whose read fails the walk (unless the RED walker drops the failure).
Fails(c) == outcome[c] \in {"storageErr", "conflict", "corrupt"} /\ ~DropErr
\* Whether some child's read fails the walk.
WalkErr == \E c \in Children : Fails(c)
\* The first failing child: the walk stops there.
FirstErr == CHOOSE c \in Children : Fails(c) /\ \A d \in Children : Fails(d) => c <= d
\* The error's type: a conflict stays a conflict unless the RED stringifies it.
ErrKind == IF outcome[FirstErr] = "conflict" /\ ~ConflictStringified THEN "conflict" ELSE "storage"
\* The walk's missing set when it succeeds.
MissingSet == {c \in Children : Missing(c)}
\* Every child is merged or was read as Some: the DAG really is all here.
FullyRead == \A c \in Children : outcome[c] \in {"merged", "present"}

\* What a site decides from the walk: ready on an empty frontier, fetch on a
\* non-empty one, unknown on an error, unless the RED takes the error as empty.
Decide(site) ==
  IF ~WalkErr
  THEN IF MissingSet = {} THEN "ready" ELSE "fetch"
  ELSE IF site \in ErrAsEmpty THEN "ready" ELSE "unknown"

\* The frontier a decision leaves on the pending row.
FrontierOf(d) ==
  CASE d = "ready" -> {}
  \* ready: nothing is missing;
    [] d = "fetch" -> MissingSet
  \* incomplete: the missing children;
    [] OTHER -> {Root}
  \* unknown, or the root itself is missing: the root.

\* -------- The pending retry, which reads the stored root first --------

\* What the retry decides: a root that is gone is missing (fetch it); a root
\* that fails to read is unknown, unless the RED takes it as empty; a root
\* that reads walks its children.
RetryDecision ==
  CASE rootRead = "gone" -> "unknown"
  \* the root itself is missing, so the frontier is the root;
    [] rootRead = "fault" -> IF "pendingRetry" \in ErrAsEmpty THEN "ready" ELSE "unknown"
  \* a root read fault is an unknown frontier;
    [] OTHER -> Decide("pendingRetry")
  \* a readable root: the walk decides.

\* Whether the retry returns an error instead of a decision: a root read
\* problem under its RED, or a failed walk under its RED.
RetryErr ==
  \/ rootRead # "ok" /\ RootFaultErrs
  \/ rootRead = "ok" /\ WalkErr /\ ErrAbortsSweep

\* -------- Actions --------

\* Every way the children and the root can read, and whether the pushed block parses.
Init ==
  /\ outcome \in [Children -> Outcomes]
  \* each child reads one way for the whole run;
  /\ rootRead \in RootOutcomes
  \* the stored root reads one way for the whole run;
  /\ pushedParses \in BOOLEAN
  \* the pushed root parses or it does not;
  /\ decision = [s \in Sites |-> "none"]
  \* no site has decided yet;
  /\ registered = FALSE
  \* nothing is registered;
  /\ pushReply = "none"
  \* the push has not replied;
  /\ acked = FALSE
  \* nothing is acked;
  /\ frontier = {}
  \* no pending row yet;
  /\ toRetry = Waiters
  \* both waiting roots are due for the sweep;
  /\ retried = {}
  \* none has been retried;
  /\ car = "none"
  \* and no CAR response has been checked.

\* A reading site walks the root and decides.
Visit(s) ==
  /\ decision[s] = "none"
  \* the site has not decided yet;
  /\ decision' = [decision EXCEPT ![s] = Decide(s)]
  \* it decides from the walk;
  /\ UNCHANGED <<outcome, rootRead, pushedParses, registered, pushReply, acked,
                 frontier, toRetry, retried, car>>
  \* and nothing else moves.

\* The root is pushed. A root that does not parse is refused with an error
\* reply; otherwise the head registers durably with the walk's frontier
\* ([root] when it is unknown). The pushed bytes come with the message, so
\* this walk does not read the stored root.
Push ==
  /\ decision["pushAdmission"] = "none"
  \* the push has not been decided;
  /\ IF ~pushedParses
     THEN /\ decision' = [decision EXCEPT !["pushAdmission"] = "refused"]
          \* a pushed block that does not parse is refused;
          /\ pushReply' = "error"
          \* with an error reply;
          /\ UNCHANGED <<registered, frontier>>
          \* and nothing is registered;
     ELSE /\ decision' = [decision EXCEPT !["pushAdmission"] = Decide("pushAdmission")]
          \* the admission decides from the walk;
          /\ registered' = TRUE
          \* the head is durably registered;
          /\ frontier' = FrontierOf(Decide("pushAdmission"))
          \* with the walk's frontier;
          /\ UNCHANGED pushReply
          \* and the reply waits for the closing sweep.
  /\ UNCHANGED <<outcome, rootRead, pushedParses, acked, toRetry, retried, car>>
  \* the children, the ack and the sweep do not move.

\* The push's closing sweep. A head registered with the frontier [root]
\* waits on itself: the sweep reads the stored root, empties the row's
\* frontier, and retries it. A retry that returns an error turns into the
\* push's error reply, though the head is registered, and leaves the frontier
\* empty. Otherwise the push is acked.
PushClose ==
  /\ registered /\ pushReply = "none"
  \* a registered head that has not replied yet;
  /\ IF frontier = {Root}
     THEN IF RetryErr
          THEN /\ pushReply' = "error"
               \* the retry's error becomes the push's reply;
               /\ frontier' = {}
               \* the row keeps the frontier the sweep emptied;
               /\ UNCHANGED acked
               \* and nothing is acked;
          ELSE /\ pushReply' = "ack"
               \* the retry decided, so the push is acked;
               /\ acked' = TRUE
               \* acked;
               /\ frontier' = FrontierOf(RetryDecision)
               \* with the retry's frontier;
     ELSE /\ pushReply' = "ack"
          \* not a waiter on itself: acked at once;
          /\ acked' = TRUE
          \* acked;
          /\ UNCHANGED frontier
          \* with the admission's frontier.
  /\ UNCHANGED <<outcome, rootRead, pushedParses, decision, registered, toRetry, retried, car>>
  \* nothing else moves.

\* The sweep retries one waiting root. Root A's retry reads its root and
\* walks its DAG: its frontier becomes the retry's answer. A retry that
\* returns an error ends the sweep and leaves A's frontier empty.
SweepStep(r) ==
  /\ r \in toRetry
  \* r is still due;
  /\ retried' = retried \cup {r}
  \* r is retried;
  /\ IF r = "A"
     THEN /\ decision' = [decision EXCEPT !["pendingRetry"] =
                            IF RetryErr THEN "none" ELSE RetryDecision]
          \* A's retry decides, unless it returned an error;
          /\ frontier' = IF RetryErr THEN {} ELSE FrontierOf(RetryDecision)
          \* its frontier is the retry's answer, or empty after an error;
          /\ toRetry' = IF RetryErr THEN {} ELSE toRetry \ {r}
          \* an error ends the sweep;
     ELSE /\ toRetry' = toRetry \ {r}
          \* B is all here and completes;
          /\ UNCHANGED <<decision, frontier>>
          \* without touching A's state.
  /\ UNCHANGED <<outcome, rootRead, pushedParses, registered, pushReply, acked, car>>
  \* the children, the push and the CAR check do not move.

\* A provider's CAR response carries a notice that child 1 is too big to
\* send. The check reads the stored root and walks it: a local fault
\* completes the response (the readiness check after it walks again), unless
\* the RED fails the response, which is recorded against the provider. A
\* successful walk honours the notice only for a child it misses.
CarCheck ==
  /\ car = "none"
  \* no response has been checked;
  /\ car' = IF rootRead = "fault" \/ (rootRead = "ok" /\ WalkErr)
            THEN IF CarFaultBlamesPeer THEN "peerFailure" ELSE "completed"
            \* a local fault: completed, or the provider is blamed under the RED;
            ELSE IF rootRead = "ok" /\ 1 \in MissingSet THEN "sizeLimit" ELSE "completed"
            \* the notice counts only for a child the node misses;
  /\ UNCHANGED <<outcome, rootRead, pushedParses, decision, registered, pushReply, acked,
                 frontier, toRetry, retried>>
  \* nothing else moves.

\* Every site has decided, the push replied and the sweep is over: the run rests here.
Done ==
  /\ \A s \in Sites \ {"pendingRetry"} : decision[s] # "none"
  \* every reading site and the push decided;
  /\ pushReply # "none"
  \* the push replied;
  /\ toRetry = {}
  \* the sweep has nothing left;
  /\ car # "none"
  \* the CAR response was checked;
  /\ UNCHANGED vars
  \* and nothing moves any more.

\* Every step a site, the push, the sweeps or the CAR check can take, or the rest.
Next ==
  \/ \E s \in WalkSites : Visit(s)
  \/ Push
  \/ PushClose
  \/ \E r \in Waiters : SweepStep(r)
  \/ CarCheck
  \/ Done

\* The model starts in Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* -------- What must hold --------

\* A site declares the DAG ready only if every child was merged or read, and
\* the pending retry only if it also read the root.
INV_ReadyImpliesRead ==
  \A s \in Sites : decision[s] = "ready" => (FullyRead /\ (s = "pendingRetry" => rootRead = "ok"))

\* A failed walk never makes a site declare the DAG ready.
INV_ErrorNeverComplete == WalkErr => \A s \in Sites : decision[s] # "ready"

\* A child that is not there is reported missing by a successful walk.
INV_AbsentIsMissing ==
  ~WalkErr => \A c \in Children : outcome[c] \in {"absent", "deleted"} => c \in MissingSet

\* An acked push leaves a pending row that still waits on something, unless
\* the whole DAG was read.
INV_AckedImpliesOwned == acked => (frontier # {} \/ FullyRead)

\* Only a pushed block that does not parse is refused.
INV_RefusedOnlyUnparsed == decision["pushAdmission"] = "refused" => ~pushedParses

\* A head that was durably registered never gets an error reply.
INV_RegisteredNeverErrors == registered => pushReply # "error"

\* When the sweep is over, every waiting root was retried.
INV_SweepTotal == toRetry = {} => retried = Waiters

\* A walk that stopped on a conflict reports a conflict, which callers retry.
INV_ConflictTyped == WalkErr /\ outcome[FirstErr] = "conflict" => ErrKind = "conflict"

\* A local read fault in the CAR size-limit check is never recorded against the provider.
INV_LocalFaultNotBlamed == car # "peerFailure"
====
