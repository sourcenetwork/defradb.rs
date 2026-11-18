/-!
# SSE delivery after loss or a query error

A subscriber has a bounded queue. A full queue can drop an update. The HTTP
stream must then end so its client can fetch state again. A query error must
be sent as an error, even when the query has no rows. An empty successful query
is still filtered away. This models the decision in `graphql_sse` in
`crates/http/src/handlers/graphql/query.rs`.
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

end SseDelivery
