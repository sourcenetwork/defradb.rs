---- MODULE TxnAdmission ----
\* The story, in plain words.
\*
\* A caller begins an explicit transaction and then sends requests that name
\* it by its ID. Callers are identities ("alice", "bob") or nobody ("anon").
\* Requests come in through one of two surfaces, each with its own registry:
\*   http  a server's HTTP API, where many remote callers share one node;
\*   ffi   the C API of an embedded node (crates/embedded, used by
\*         crates/ffi), where the one process embedding the node is the only
\*         caller, and the identity a request carries is the user that process
\*         acts for in that request.
\* A registry remembers an owner key for each transaction it began, and lets a
\* request reach the transaction only when the request's key is the same
\* (Owners::run and Table::admits in crates/db/src/txn/owner). What the key is
\* depends on the registry's ownership mode (TxnOwnership in
\* crates/db/src/txn/registry/mod.rs):
\*   identity  the key is the identity the request acts as;
\*   embedder  every request has the same key.
\* A request that reaches the transaction runs its work as the identity the
\* request itself carries, so access control judges that identity.
\*
\* What must never happen:
\*   1. on the FFI, a request is turned away from a transaction the embedding
\*      process began, because the request acts as a different identity than
\*      the one the transaction began with. Go never does this, and Go's
\*      integration suite fails on it (issue #2335) - INV_FfiReaches;
\*   2. on HTTP, a caller reaches a transaction another identity began, or the
\*      identity that began it is turned away - INV_HttpOwnerOnly;
\*   3. a request runs its work as some identity other than its own, so access
\*      control judges the wrong user - INV_ActsAsCaller.
\*
\* The Rust code each guard models:
\*   EmbedderMode   crates/embedded/src/node.rs build_node: the embedded node
\*                  builds its registry with TxnOwnership::Embedder;
\*   IdentityKey    crates/db/src/txn/registry/mod.rs: TxnOwnership defaults
\*                  to ByIdentity, which every other registry keeps;
\*   ActsAsRequest  crates/db/src/txn/registry/mod.rs in_txn and
\*                  crates/db/src/txn/registry/lifecycle.rs run_as: the work
\*                  runs inside the request's own context (scope.mirror).
\*
\* Out of the model: commits, discards and idle expiry, which end a
\* transaction and are covered by CommitFence; they admit a request by the
\* same key check as the requests modeled here.

EXTENDS Naturals, Sequences  \* numbers, and lists of who began each transaction

CONSTANTS
  EmbedderMode,   \* TRUE: the FFI registry gives every request the same key
  IdentityKey,    \* TRUE: the HTTP registry keys a request by its identity
  ActsAsRequest   \* TRUE: a request's work runs as the request's own identity

\* Who may send a request; "anon" is a request with no identity.
Callers == {"anon", "alice", "bob"}

\* The two surfaces, each with its own registry.
Surfaces == {"http", "ffi"}

\* How many transactions each registry may begin, to keep the model small.
MaxTxns == 2

\* The ownership mode of the registry behind surface s.
Mode(s) ==
  \* The FFI registry: embedder mode, unless the embedded node forgot to ask.
  IF s = "ffi" THEN (IF EmbedderMode THEN "embedder" ELSE "identity")
  \* The HTTP registry: identity mode, unless embedder mode leaked into it.
  ELSE (IF IdentityKey THEN "identity" ELSE "embedder")

\* The owner key of a request from caller c on surface s (caller in
\* crates/db/src/txn/registry/mod.rs).
Key(s, c) ==
  \* In embedder mode every request has one and the same key;
  IF Mode(s) = "embedder" THEN "embedder"
  \* in identity mode the key is the identity the request acts as.
  ELSE c

\* The outcome of a request: where it came in, who sent it, who began the
\* transaction it named, whether it reached it, and as whom its work ran.
Outcomes == [surface : Surfaces \cup {"none"},
             who     : Callers,
             owner   : Callers,
             reached : BOOLEAN,
             actedAs : Callers]

\* The outcome recorded before any request has been sent.
NoRequest == [surface |-> "none", who |-> "anon", owner |-> "anon",
              reached |-> FALSE, actedAs |-> "anon"]

VARIABLES
  began,  \* began[s][i]: the caller that began transaction i on surface s
  last    \* the outcome of the most recent request

\* Every variable, for stuttering steps.
vars == <<began, last>>

\* Each variable holds a value of the right shape.
TypeOK ==
  \* Each surface keeps a list of who began its transactions, no longer than
  \* the limit;
  /\ \A s \in Surfaces : began[s] \in Seq(Callers) /\ Len(began[s]) <= MaxTxns
  \* the last outcome is a well-formed outcome.
  /\ last \in Outcomes

\* At the start no transaction exists and no request has been sent.
Init ==
  \* No transactions on either surface;
  /\ began = [s \in Surfaces |-> <<>>]
  \* nothing has been asked yet.
  /\ last = NoRequest

\* Caller c begins a transaction on surface s; its owner key is fixed now.
Begin(s, c) ==
  \* The registry still has room in the model;
  /\ Len(began[s]) < MaxTxns
  \* it records who began the new transaction;
  /\ began' = [began EXCEPT ![s] = Append(@, c)]
  \* and no request is answered by beginning.
  /\ UNCHANGED last

\* Caller c sends a request on surface s naming transaction i of that surface.
Use(s, c, i) ==
  \* The caller that began the transaction.
  LET owner   == began[s][i]
      \* Table::admits: the request reaches it when the two keys are the same.
      reached == Key(s, owner) = Key(s, c)
  IN
  \* Record the outcome. A request that reached the transaction runs its work
  \* as its own identity, or, with that guard broken, as the beginner's.
  /\ last' = [surface |-> s, who |-> c, owner |-> owner, reached |-> reached,
              actedAs |-> IF reached /\ ~ActsAsRequest THEN owner ELSE c]
  \* Using a transaction begins none.
  /\ UNCHANGED began

\* One step: some caller begins a transaction, or uses one that exists.
Next ==
  \* Any caller begins a transaction on any surface;
  \/ \E s \in Surfaces, c \in Callers : Begin(s, c)
  \* or any caller names any transaction of the surface it comes in through.
  \/ \E s \in Surfaces, c \in Callers : \E i \in 1..Len(began[s]) : Use(s, c, i)

\* The whole behaviour: start empty, then take steps.
Spec == Init /\ [][Next]_vars

\* Promise: on the FFI, a request reaches every transaction the embedding
\* process began, whatever identity either one acts as.
INV_FfiReaches == last.surface = "ffi" => last.reached

\* Promise: on HTTP, a request reaches a transaction exactly when it acts as
\* the identity that began it.
INV_HttpOwnerOnly == last.surface = "http" => (last.reached <=> last.who = last.owner)

\* Promise: a request that reaches a transaction runs its work as its own
\* identity, never as the identity that began the transaction.
INV_ActsAsCaller == last.reached => last.actedAs = last.who

====
