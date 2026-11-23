---- MODULE DagReadiness ----
\* The story. A node holds one root block and wants to know whether the whole
\* DAG under it is here. It walks the root's children: a merged child is done,
\* a stored child is read, a child it does not hold is "missing" and gets
\* fetched. Reading a stored child can go wrong: the block can be deleted
\* between being linked and being read, the read can fail, the storage can
\* report a conflict, or the bytes can be corrupt.
\*
\* Who acts: the walker (find_all_missing_links), and the eight places that
\* ask it whether a DAG is complete (the start of a fetch, after a rooted CAR,
\* the selective loop, the final check, the DocSync re-merge, the PushLog
\* admission, the pending retry and the restore sweep), plus a sweep that
\* retries every root that was waiting on a block that just arrived.
\*
\* What must never happen: a DAG is declared ready although one of its
\* blocks was never read; a failed walk is taken as "nothing missing"; a
\* block that is not there is not reported missing; a pushed head is acked
\* without something that will finish it; one root's failed walk stops the
\* sweep before the other waiting roots are retried; a storage conflict loses
\* its type, so nobody retries it.
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
\*   manager/process/pushlog.rs  process_block_inner (pushAdmission: only a pushed
\*     block that does not parse is refused; a walk error registers [root] and acks)
\*   manager/process/pending_dag.rs  retry_pending_dag (pendingRetry: an error resets
\*     the frontier to [root] and returns Ok(false)), retry_pending_dags_waiting_on
\*     (the sweep), and the restore walk in resync_persisted_pending_dags (restore:
\*     an error is the frontier [root])
\* start, walkLoop and restore already took an error as incomplete before
\* #2600; their RED configurations keep them that way. The other callers
\* (replication/handlers.rs dag_is_local, branchable_sync.rs, car.rs) also
\* never take an error as complete.
\* One level of children stands for the whole DAG: the walk treats every
\* level the same way, and a child's children only add more of the same reads.
EXTENDS Naturals, FiniteSets

CONSTANTS
  N,                    \* how many children the root links
  SkipNone,             \* RED: a child that reads as None is skipped, not missing
  DropErr,              \* RED: the walker drops a failed child read (main before #2600)
  ErrAsEmpty,           \* RED: the sites that take a failed walk as "nothing missing"
  ErrAbortsSweep,       \* RED: a failed walk in the pending retry stops the sweep
  ConflictStringified   \* RED: a storage conflict comes back as a plain storage error

\* The children, walked in order.
Children == 1..N
\* What reading one child finds.
Outcomes == {"merged", "present", "absent", "deleted", "storageErr", "conflict", "corrupt"}
\* The places that act on a walk's answer.
Sites == {"start", "rootedCar", "walkLoop", "final", "docSync", "pushAdmission", "pendingRetry", "restore"}
\* The sites that only read the answer (the push and the sweep have their own actions).
WalkSites == Sites \ {"pushAdmission", "pendingRetry"}
\* The two roots waiting on the block that just arrived: A is the root above,
\* B is a healthy root whose DAG is all here.
Waiters == {"A", "B"}

VARIABLES
  outcome,       \* what a read of each child finds (fixed for the run)
  pushedParses,  \* whether the pushed root block itself parses
  decision,      \* what each site decided: "none", "ready", "fetch" or "unknown"
  acked,         \* whether the push of the root was acked
  frontier,      \* the pending row's frontier after the push or the retry
  toRetry,       \* waiting roots the sweep has not retried yet
  retried        \* waiting roots the sweep retried
vars == <<outcome, pushedParses, decision, acked, frontier, toRetry, retried>>

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

\* -------- Actions --------

\* Every way the children can read, and whether the pushed block parses.
Init ==
  /\ outcome \in [Children -> Outcomes]
  \* each child reads one way for the whole run;
  /\ pushedParses \in BOOLEAN
  \* the pushed root parses or it does not;
  /\ decision = [s \in Sites |-> "none"]
  \* no site has decided yet;
  /\ acked = FALSE
  \* nothing is acked;
  /\ frontier = {}
  \* no pending row yet;
  /\ toRetry = Waiters
  \* both waiting roots are due for the sweep;
  /\ retried = {}
  \* and none has been retried.

\* A reading site walks the root and decides.
Visit(s) ==
  /\ decision[s] = "none"
  \* the site has not decided yet;
  /\ decision' = [decision EXCEPT ![s] = Decide(s)]
  \* it decides from the walk;
  /\ UNCHANGED <<outcome, pushedParses, acked, frontier, toRetry, retried>>
  \* and nothing else moves.

\* The root is pushed. A root that does not parse is refused; otherwise the
\* head registers with the walk's frontier ([root] when it is unknown) and is acked.
Push ==
  /\ decision["pushAdmission"] = "none"
  \* the push has not been decided;
  /\ IF ~pushedParses
     THEN /\ decision' = [decision EXCEPT !["pushAdmission"] = "refused"]
          \* a pushed block that does not parse gets an error reply;
          /\ UNCHANGED <<acked, frontier>>
          \* nothing is acked or registered;
     ELSE /\ decision' = [decision EXCEPT !["pushAdmission"] = Decide("pushAdmission")]
          \* the admission decides from the walk;
          /\ acked' = TRUE
          \* the push is acked;
          /\ frontier' = CASE Decide("pushAdmission") = "ready" -> {}
                         \* a complete DAG registers with nothing missing;
                           [] Decide("pushAdmission") = "fetch" -> MissingSet
                         \* an incomplete one with its missing children;
                           [] OTHER -> {"root"}
                         \* an unknown one with the root itself.
  /\ UNCHANGED <<outcome, pushedParses, toRetry, retried>>
  \* the children and the sweep do not move.

\* The sweep retries one waiting root. Root A's retry walks its DAG: its
\* frontier becomes the walk's answer. A failed walk stops the sweep only
\* under the RED.
SweepStep(r) ==
  /\ r \in toRetry
  \* r is still due;
  /\ retried' = retried \cup {r}
  \* r is retried;
  /\ IF r = "A"
     THEN /\ decision' = [decision EXCEPT !["pendingRetry"] = Decide("pendingRetry")]
          \* A's retry decides from its walk;
          /\ frontier' = CASE Decide("pendingRetry") = "ready" -> {}
                         \* ready: nothing is missing;
                           [] Decide("pendingRetry") = "fetch" -> MissingSet
                         \* incomplete: the missing children;
                           [] OTHER -> {"root"}
                         \* unknown: the root itself;
          /\ toRetry' = IF WalkErr /\ ErrAbortsSweep THEN {} ELSE toRetry \ {r}
          \* a failed walk ends the sweep only under the RED;
     ELSE /\ toRetry' = toRetry \ {r}
          \* B is all here and completes;
          /\ UNCHANGED <<decision, frontier>>
          \* without touching A's state.
  /\ UNCHANGED <<outcome, pushedParses, acked>>
  \* the children and the push do not move.

\* Every site has decided and the sweep is over: the run rests here.
Done ==
  /\ \A s \in Sites \ {"pendingRetry"} : decision[s] # "none"
  \* every reading site and the push decided;
  /\ toRetry = {}
  \* the sweep has nothing left;
  /\ UNCHANGED vars
  \* and nothing moves any more.

\* Every step a site, the push or the sweep can take, or the rest.
Next ==
  \/ \E s \in WalkSites : Visit(s)
  \/ Push
  \/ \E r \in Waiters : SweepStep(r)
  \/ Done

\* The model starts in Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* -------- What must hold --------

\* A site declares the DAG ready only if every child was merged or read.
INV_ReadyImpliesRead == \A s \in Sites : decision[s] = "ready" => FullyRead

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

\* When the sweep is over, every waiting root was retried.
INV_SweepTotal == toRetry = {} => retried = Waiters

\* A walk that stopped on a conflict reports a conflict, which callers retry.
INV_ConflictTyped == WalkErr /\ outcome[FirstErr] = "conflict" => ErrKind = "conflict"
====
