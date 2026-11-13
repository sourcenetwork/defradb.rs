/-!
What an in-process call to an embedded Vera node tells its caller.

The vera.rs provider can reach a Vera node hosted in its own process. It asks
through the node's method table and gets the answer back without a socket.
Each call ends in one of five ways: the node gives a result, the node refuses
with an error code, the result is longer than the caller allows, the result is
not JSON, or the call never reached the node (the node has stopped, or the
request could not be handed over).

The provider already knows these endings from its network transports. It asks
a busy node again and gives up on anything else. The in-process transport has
to report each ending in the same terms, so the provider behaves as it does
over the network.

What must never happen:
  * a result the network transports would accept is refused in-process;
  * a busy node is not asked again, or a stopped node is asked again and again;
  * two different endings are reported as the same one, so that a caller
    cannot tell a stopped node from a busy one.

Rust owner: `within` in `crates/vera-embedded/src/call.rs`, `client_error` in
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

/-- How a call ended when it produced no result. -/
inductive Failure where
  /-- The node answered with a JSON-RPC error carrying this code. -/
  | node (code : Int)
  /-- The result is longer than the caller allows. -/
  | tooLarge
  /-- The result is not JSON. -/
  | malformed
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

/-- A stopped node is not asked again: the request stays in the worker's
journal for the next node instead of being repeated at a node that is gone. -/
theorem stopped_node_is_not_asked_again : retryable (clientError .channel) = false :=
  rfl

/-- The provider asks again in exactly one case: the node answered with one of
the two codes it retries over HTTP. Moving the node in-process adds no retry
and removes none. -/
theorem asked_again_iff (failure : Failure) :
    retryable (clientError failure) = true ↔
      failure = .node (-32002) ∨ failure = .node (-32000) := by
  cases failure with
  | node code => simp [clientError, retryable]
  | tooLarge => simp [clientError, retryable]
  | malformed => simp [clientError, retryable]
  | channel => simp [clientError, retryable]

/-- Different endings give the submission client different errors. In
particular a stopped node never looks like a busy one. -/
theorem client_errors_stay_distinct (a b : Failure) (same : clientError a = clientError b) :
    a = b := by
  cases a <;> cases b <;> simp [clientError] at same ⊢
  exact same

/-- What the light client is told: the error it already gets from its HTTP
transport for the same ending. -/
inductive TransportError where
  /-- The node's own error, with its code. -/
  | rpc (code : Int)
  /-- The response broke the caller's limit. -/
  | responseTooLarge
  /-- Any other failure; `channel` says it is this transport's own typed
  error, which a caller can recognise, and not the JSON decoder's. -/
  | other (channel : Bool)
  deriving DecidableEq

/-- The light client's error for each ending. -/
def transportError : Failure → TransportError
  -- The node's code is passed on untouched.
  | .node code => .rpc code
  -- Same refusal as an oversized HTTP response.
  | .tooLarge => .responseTooLarge
  -- The decoder's own error, as over HTTP.
  | .malformed => .other false
  -- This transport's typed error, which names a stopped node.
  | .channel => .other true

/-- Different endings give the light client different errors too. -/
theorem transport_errors_stay_distinct (a b : Failure)
    (same : transportError a = transportError b) : a = b := by
  cases a <;> cases b <;> simp [transportError] at same ⊢
  exact same

end VeraEmbeddedTransport
