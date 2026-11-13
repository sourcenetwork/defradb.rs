/-!
What an in-process call to an embedded Vera node tells its caller.

The vera.rs provider can reach a Vera node hosted in its own process. It asks
through the node's method table and gets the answer back without a socket.
Each call ends in one of six ways: the node gives a result, the node refuses
with an error code, the result is longer than the caller allows, the result is
not JSON, the node gives no answer before the caller's deadline, or the call
never reached the node (the node has stopped, or the request could not be
handed over).

The provider already knows these endings from its network transports. It asks
again after a busy answer or a timeout and gives up on anything else. The
in-process transport has to report each ending in the same terms, so the
provider behaves as it does over the network.

What must never happen:
  * a result the network transports would accept is refused in-process;
  * a caller waits without end for a node that does not answer;
  * an answer that came in time is thrown away;
  * a busy or slow node is not asked again, or a stopped node is asked again
    and again;
  * two different endings are reported as the same one, so that a caller
    cannot tell a stopped node from a busy or a slow one.

Rust owner: `within` in `crates/vera-embedded/src/call.rs`, `bounded` in
`crates/vera-embedded/src/deadline.rs`, `client_error` in
`crates/vera-embedded/src/submission.rs` and `transport_error` in
`crates/vera-embedded/src/acp.rs`. The retry rule they feed is
`ClientError::retryable` in `crates/vera/src/vera_rs/client.rs`.

The model is over the outcome of a call. That the node's method table gives
the answers its network server gives is vera.rs's own property, assumed here.
-/
namespace VeraEmbeddedTransport

/-- Whether a result of `length` bytes is within the caller's limit of `limit`
bytes. The in-process transport measures the JSON text of the result. -/
def within (length limit : Nat) : Bool :=
  -- A result exactly as long as the limit still fits.
  decide (length ≤ limit)

/-- Whether a network transport accepts the same result. It measures the whole
response body: the result inside a JSON-RPC envelope of `envelope` bytes. -/
def networkAccepts (envelope length limit : Nat) : Bool :=
  -- The envelope counts against the limit too.
  decide (envelope + length ≤ limit)

/-- A result is accepted in-process exactly when it is no longer than the
limit. -/
theorem within_iff (length limit : Nat) : within length limit = true ↔ length ≤ limit := by
  simp [within]

/-- A result the network transports accept is accepted in-process: moving the
node into the process refuses nothing that was valid before. -/
theorem network_accepted_is_accepted (envelope length limit : Nat)
    (accepted : networkAccepts envelope length limit = true) : within length limit = true := by
  simp [networkAccepts] at accepted
  simp [within]
  omega

/-- A result longer than the limit is refused, whatever else is true of it. -/
theorem longer_is_refused (length limit : Nat) (longer : limit < length) :
    within length limit = false := by
  simp [within]
  omega

/-- Whether the caller gets the node's answer. `answer` is the moment the node
answers, if it ever does, and `deadline` the moment the caller stops waiting.
The answer is looked for first, so one that is there at the deadline counts. -/
def answered (answer : Option Nat) (deadline : Nat) : Bool :=
  match answer with
  -- The node answered: in time when that is no later than the deadline.
  | some moment => decide (moment ≤ deadline)
  -- The node never answers: the wait ends at the deadline without one.
  | none => false

/-- An answer that comes no later than the deadline is the caller's: the
deadline throws nothing away that came in time. -/
theorem answer_in_time_is_kept (moment deadline : Nat) (inTime : moment ≤ deadline) :
    answered (some moment) deadline = true := by
  simp [answered, inTime]

/-- A node that never answers ends the wait at the deadline, whatever the
deadline is: no caller waits without end. -/
theorem silent_node_ends_the_wait (deadline : Nat) : answered none deadline = false :=
  rfl

/-- An answer that comes after the deadline is not waited for. -/
theorem late_answer_is_not_waited_for (moment deadline : Nat) (late : deadline < moment) :
    answered (some moment) deadline = false := by
  simp [answered]
  omega

/-- How a call ended when it produced no result. -/
inductive Failure where
  /-- The node answered with a JSON-RPC error carrying this code. -/
  | node (code : Int)
  /-- The result is longer than the caller allows. -/
  | tooLarge
  /-- The result is not JSON. -/
  | malformed
  /-- The node gave no answer before the caller's deadline. -/
  | elapsed
  /-- The call did not reach the node: it has stopped, or the request could
  not be handed over. -/
  | channel
  deriving DecidableEq

/-- What the submission client is told: the error it already gets from the
HTTP transport for the same ending. -/
inductive ClientError where
  /-- The node's own error, with its code. -/
  | rpc (code : Int)
  /-- The response broke the caller's limit. -/
  | invalidResponse
  /-- The response is not JSON. -/
  | json
  /-- The transport failed, and says whether asking again may help. -/
  | transport (retryable : Bool)
  deriving DecidableEq

/-- The submission client's error for each ending. -/
def clientError : Failure → ClientError
  -- The node's code is passed on untouched.
  | .node code => .rpc code
  -- Same refusal as an oversized HTTP response.
  | .tooLarge => .invalidResponse
  -- Same refusal as an HTTP body that is not JSON.
  | .malformed => .json
  -- The node may be slow, not gone: asking again may help, as after an HTTP
  -- timeout.
  | .elapsed => .transport true
  -- The node is gone or the request cannot be sent: asking again cannot help.
  | .channel => .transport false

/-- Whether the provider asks again after an error. A node that is busy
(`-32002`) or momentarily unavailable (`-32000`) is asked again; a transport
failure is asked again only when it says so. -/
def retryable : ClientError → Bool
  -- The two codes the provider retries over HTTP.
  | .rpc code => code == -32002 || code == -32000
  -- A transport failure carries its own answer.
  | .transport again => again
  -- Nothing else is retried.
  | _ => false

/-- A node's error keeps its code, so the provider reads it as it does over
HTTP. -/
theorem node_error_keeps_its_code (code : Int) : clientError (.node code) = .rpc code :=
  rfl

/-- A busy node is asked again. -/
theorem busy_node_is_asked_again : retryable (clientError (.node (-32002))) = true := by
  decide

/-- A node that gave no answer in time is asked again. -/
theorem slow_node_is_asked_again : retryable (clientError .elapsed) = true :=
  rfl

/-- A stopped node is not asked again: the request stays in the worker's
journal for the next node instead of being repeated at a node that is gone. -/
theorem stopped_node_is_not_asked_again : retryable (clientError .channel) = false :=
  rfl

/-- The provider asks again in exactly three cases: the node answered with one
of the two codes it retries, or it gave no answer in time. -/
theorem asked_again_iff (failure : Failure) :
    retryable (clientError failure) = true ↔
      failure = .node (-32002) ∨ failure = .node (-32000) ∨ failure = .elapsed := by
  cases failure with
  | node code => simp [clientError, retryable]
  | tooLarge => simp [clientError, retryable]
  | malformed => simp [clientError, retryable]
  | elapsed => simp [clientError, retryable]
  | channel => simp [clientError, retryable]

/-- How a call over the HTTP transport ends without a result, as the
provider's retry rule sees it. -/
inductive HttpFailure where
  /-- The node answered with a JSON-RPC error carrying this code. -/
  | node (code : Int)
  /-- The response is longer than the caller allows. -/
  | tooLarge
  /-- The response is not JSON. -/
  | malformed
  /-- The request timed out. -/
  | timeout

/-- Whether the provider asks again after an HTTP ending: after a busy answer
and after a timeout, which the HTTP transport reports as its own retryable
error. -/
def httpAskedAgain : HttpFailure → Bool
  -- The two codes the provider retries.
  | .node code => code == -32002 || code == -32000
  -- A timeout is always asked again.
  | .timeout => true
  -- Nothing else is retried.
  | _ => false

/-- The HTTP ending an in-process ending stands for. A node that has stopped
has none: over HTTP the provider cannot tell a stopped node from an
unreachable one, in-process it can. -/
def overHttp : Failure → Option HttpFailure
  -- The node's own error is the same either way.
  | .node code => some (.node code)
  -- So is an oversized result.
  | .tooLarge => some .tooLarge
  -- So is a result that is not JSON.
  | .malformed => some .malformed
  -- No answer before the deadline is what a timeout is.
  | .elapsed => some .timeout
  -- A stopped in-process node has no HTTP counterpart.
  | .channel => none

/-- Wherever an in-process ending has an HTTP counterpart, the provider asks
again in-process exactly when it does over HTTP. Moving the node into the
process adds no retry and removes none. -/
theorem asked_again_as_over_http (failure : Failure) (http : HttpFailure)
    (same : overHttp failure = some http) :
    retryable (clientError failure) = httpAskedAgain http := by
  cases failure <;> simp [overHttp] at same <;> subst same <;> rfl

/-- Different endings give the submission client different errors. In
particular a stopped node never looks like a busy or a slow one. -/
theorem client_errors_stay_distinct (a b : Failure) (same : clientError a = clientError b) :
    a = b := by
  cases a <;> cases b <;> simp [clientError] at same ⊢
  exact same

/-- What an "other" failure of the light client's transport holds. The two
typed errors are this transport's own, which a caller can recognise. -/
inductive Cause where
  /-- The JSON decoder's error. -/
  | decoder
  /-- This transport's typed error for a node that gave no answer in time. -/
  | elapsed
  /-- This transport's typed error for a node that has stopped, or a request
  that could not be handed over. -/
  | channel
  deriving DecidableEq

/-- What the light client is told: the error it already gets from its HTTP
transport for the same ending. -/
inductive TransportError where
  /-- The node's own error, with its code. -/
  | rpc (code : Int)
  /-- The response broke the caller's limit. -/
  | responseTooLarge
  /-- Any other failure, with what it holds. -/
  | other (cause : Cause)
  deriving DecidableEq

/-- The light client's error for each ending. -/
def transportError : Failure → TransportError
  -- The node's code is passed on untouched.
  | .node code => .rpc code
  -- Same refusal as an oversized HTTP response.
  | .tooLarge => .responseTooLarge
  -- The decoder's own error, as over HTTP.
  | .malformed => .other .decoder
  -- Named with its method and deadline. The header loop subscribes again and
  -- a read fails, as after an HTTP timeout.
  | .elapsed => .other .elapsed
  -- This transport's typed error, which names a stopped node.
  | .channel => .other .channel

/-- Different endings give the light client different errors too. -/
theorem transport_errors_stay_distinct (a b : Failure)
    (same : transportError a = transportError b) : a = b := by
  cases a <;> cases b <;> simp [transportError] at same ⊢
  exact same

end VeraEmbeddedTransport
