/-!
# CommitResponseWait.Answer: a server bound answers unknown without cancelling

The story, in plain words. A client waits for one transaction commit. The
server has its own bound, not the client's idle clock. If no real outcome is
known by that bound, the server answers `unknown`; the worker-local commit
keeps its ticket, so the answer is not an abort. Without the bound, a wedged
commit can leave the client waiting forever.

The temporal story is `proofs/tla/CommitResponseWait.tla`; this file carries
the one-step law the model takes on trust.

Rust anchors: `Server::serve_router`'s commit bound in
`crates/surfaces/http/src/server.rs`, `QueryRunner::commit_txn_as`'s detached
registry commit in `crates/engine/query/src/runner/executor.rs`, and the
worker-owned commit in `crates/database/db/src/txn/owner/`.
-/

namespace CommitResponseWait

-- The answer a commit request can receive.
inductive Answer where
  -- Nothing has answered yet.
  | none
  -- Storage reported that the commit landed.
  | success
  -- Storage reported that the commit failed.
  | failure
  -- The server's bound passed before storage could report either outcome.
  | unknown
  -- Answers can be compared and printed.
  deriving DecidableEq, Repr

-- The ticket a bounded answer leaves behind.
inductive Ticket where
  -- The commit stays owned by its worker and may still land or fail.
  | kept
  -- A bad bound cancelled the commit; the RED model rejects this.
  | cancelled
  -- Tickets can be compared and printed.
  deriving DecidableEq, Repr

-- What the server answers for `known` at `clock`, under `boundEnabled` and `bound`.
def serverAnswer (boundEnabled : Bool) (known : Option Bool)
    (clock bound : Nat) : Answer :=
  -- If the bound is on, no outcome is known, and the bound has passed, answer unknown.
  if boundEnabled && known.isNone && bound ≤ clock then
    -- This is the explicit unknown outcome.
    .unknown
  -- Otherwise, report the outcome storage knows, if it knows one.
  else
    -- A landed commit reports success; a failed one reports failure.
    match known with
    -- Success is the real answer for a landed commit.
    | some true => .success
    -- Failure is the real answer for a failed commit.
    | some false => .failure
    -- With no bound, no outcome yet means the client still waits.
    | none => .none

-- What a bounded answer does to the commit's ticket.
def afterBound (ticket : Ticket) : Ticket :=
  -- The real design preserves the worker-local commit exactly.
  ticket

-- Promise: with the bound on and no known outcome, a passed bound answers unknown.
theorem bound_answers_unknown (clock bound : Nat)
    (hbound : bound ≤ clock) :
    -- The server's answer is explicitly unknown.
    serverAnswer true none clock bound = .unknown := by
  -- Reduce the definition and use the passed-bound hypothesis.
  simp [serverAnswer, hbound]

-- Promise: a bounded answer does not cancel the ticket.
theorem unknown_keeps_ticket (ticket : Ticket) :
    -- Whatever ticket entered the bound leaves it unchanged.
    afterBound ticket = ticket := by
  -- Compute the identity.
  rfl

-- Promise: with the bound off, no known outcome leaves the client waiting.
theorem no_bound_waits (clock bound : Nat) :
    -- The server has still sent no answer.
    serverAnswer false none clock bound = .none := by
  -- Reduce the disabled-bound branch.
  simp [serverAnswer]

end CommitResponseWait
