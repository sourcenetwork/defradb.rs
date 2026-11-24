/-!
# SSE delivery after loss or a query error

A subscriber has a bounded queue. A full queue can drop an update. The HTTP
stream must then end so its client can fetch state again. A query error must
be sent as an error, even when the query has no rows. An empty successful query
is still filtered away. Before its first event the stream checks the request
once through the plain path (`open_failure`): an invalid request the plain path
refuses sends that error and completes, and a check that failed for a reason
that passes keeps the stream. This models the decision in `graphql_sse` in
`crates/surfaces/http/src/handlers/graphql/query.rs`, and the FFI
subscription's per-update decision in `crates/entry-points/ffi/src/query/exec.rs`;
the open check's ordering is `proofs/tla/SubscriptionOpenCheck.tla`.
-/
namespace SseDelivery

-- A scoped query can return rows, no rows, or an error.
inductive Result where
  | rows | empty | error
  deriving DecidableEq

-- The stream sends a result, skips it, or ends for recovery.
inductive Action where
  | sendRows | skip | sendError | recover
  deriving DecidableEq

-- A dropped update takes priority over the scoped query's answer.
def decide (dropped : Bool) (result : Result) : Action :=
  if dropped then .recover else
    match result with
    | .rows => .sendRows
    | .empty => .skip
    | .error => .sendError

-- A detected loss always asks the client to recover.
theorem loss_requires_recovery (result : Result) : decide true result = .recover := by
  rfl

-- An overflow closes only a subscription that opted into terminal loss.
def closeAfterOverflow (closeOnOverflow : Bool) : Bool := closeOnOverflow

-- A loss-aware stream closes even if its queue has already been drained.
theorem loss_aware_closes : closeAfterOverflow true = true := by
  rfl

-- Ordinary raw subscribers keep their existing lossy queue behavior.
theorem raw_stays_open : closeAfterOverflow false = false := by
  rfl

-- A failed query is never mistaken for a successful empty result.
theorem errors_are_forwarded : decide false .error = .sendError := by
  rfl

-- A successful empty result remains invisible.
theorem empty_is_skipped : decide false .empty = .skip := by
  rfl

-- A typed authorization refusal is different from a successful empty read.
inductive QueryFailure where
  | permissionDenied | other
  deriving DecidableEq

-- The wire code comes from the failure's type, never from its message.
def code (failure : QueryFailure) : Option String :=
  match failure with
  | .permissionDenied => some "PERMISSION_DENIED"
  | .other => none

-- A client can identify an explicit authorization refusal.
theorem denial_has_code : code .permissionDenied = some "PERMISSION_DENIED" := by
  rfl

-- An unrelated query failure does not assert a refusal.
theorem other_has_no_denial : code .other = none := by
  rfl

-- Once subscribed, the stream runs the request once as a plain request,
-- raced with the server closing. A good request answers that it needs a
-- stream; an invalid one (an unknown filter field) answers with its error; a
-- check can also fail for a reason that passes (a store read failed, a
-- provider answered late), which says nothing about the request.
inductive OpenCheck where
  | needsStream | invalid | transient | closing
  deriving DecidableEq

-- What the stream does after that check.
inductive OpenOutcome where
  | stream | errorThenComplete | complete
  deriving DecidableEq

-- The decision `open_failure` makes in `graphql_sse`.
def openOutcome : OpenCheck → OpenOutcome
  -- A good request streams its updates.
  | .needsStream => .stream
  -- An invalid one sends its error as the first result, then completes.
  | .invalid => .errorThenComplete
  -- A failure that passes keeps the stream, unlike Go's, which ends it.
  | .transient => .stream
  -- A closing server completes the stream without waiting for the check.
  | .closing => .complete

-- An invalid request is told why, and its stream ends.
theorem bad_open_reports_then_completes : openOutcome .invalid = .errorThenComplete := by
  rfl

-- A check that failed for a reason that passes keeps the stream.
theorem transient_open_keeps_stream : openOutcome .transient = .stream := by
  rfl

-- Closing ends the stream even while its check runs.
theorem closing_open_completes : openOutcome .closing = .complete := by
  rfl

-- Only an invalid request ends its stream with an error at open.
theorem only_invalid_open_is_refused (check : OpenCheck) :
    openOutcome check = .errorThenComplete ↔ check = .invalid := by
  -- One case per check answer.
  cases check <;> simp [openOutcome]

-- A kept subscription runs the request again for each update. That run can
-- return rows, no rows, an error saying the request itself is invalid (a bad
-- filter kept past a check that failed for a reason that passes), or an
-- error that may pass.
inductive UpdateRun where
  | rows | empty | invalid | transient
  deriving DecidableEq

-- The FFI subscription sends the run, skips it, or sends it and ends.
inductive UpdateAction where
  | send | skip | sendThenEnd
  deriving DecidableEq

-- The decision `subscribe` in `crates/entry-points/ffi/src/query/exec.rs`
-- makes for each update, with `update_refused` naming an invalid request.
def ffiUpdate : UpdateRun → UpdateAction
  -- Rows are a result for the client.
  | .rows => .send
  -- No rows: the filter excluded the document, so there is nothing to send.
  | .empty => .skip
  -- An invalid request is told why, and the subscription ends.
  | .invalid => .sendThenEnd
  -- An error that may pass carries no rows, so it is skipped and the
  -- subscription stays for the next update.
  | .transient => .skip

-- A bad request found by an update reaches the client as its last result.
theorem ffi_invalid_update_reports_then_ends : ffiUpdate .invalid = .sendThenEnd := by
  rfl

-- Only an invalid request ends the FFI subscription on an update.
theorem ffi_only_invalid_update_ends (run : UpdateRun) :
    ffiUpdate run = .sendThenEnd ↔ run = .invalid := by
  -- One case per run answer.
  cases run <;> simp [ffiUpdate]

end SseDelivery
