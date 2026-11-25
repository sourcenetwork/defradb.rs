/-!
# Error conversion preserves the cause at a crate boundary

An internal helper can return a specific error. Its crate's public API wraps
that error, and a facade can wrap the crate's error in turn. Each wrapper
retains the typed cause. The lower crate does not need the facade's error type.

This model proves that either one or two boundary wrappers retain the original
cause and that converting a failed result leaves successful values unchanged.
It does not model Display text or the Error::source forwarding convention of
thiserror's transparent variants; compiler fixtures check the derive boundary.

`results` in `tools/code-shape/src/return_types.rs` collects result leaves
from returned containers and future outputs. A successful outer Result may
still carry another Result; its error must be checked too. A result's error
payload is a cause, not another returned result, so traversal stops there.
Compiler type resolution and written alias matching remain compiler boundaries.
Written alias provenance lives in tools/code-shape/src/written_results.rs;
return_types.rs owns the resolved delivered-value traversal.
A carrier delivers values through a known standard wrapper or a future.
Named domain structs and enums are value boundaries. A phantom parameter or
sender input alone delivers no result. Compiler fixtures check these cases
and callable error provenance.
For a named future, the compiler checks its resolved output's error owner.
An explicit output binding or a parameter visible in the generic output
also provides spelling. Substituting a selector could instead select a
different implementation, so result counts do not establish provenance.
The abstract model describes finite carriers. The compiler bounds the number
of future projections across the whole walk: a future can deliver larger or
multiple instances of itself. Plain containers use no projection budget.

-/

namespace ErrorBoundary

-- An error wrapper retains its cause as a value of the original type.
structure Wrapped (Cause : Type) where
  -- No string conversion or discarded information replaces this value.
  cause : Cause

-- Entering the next crate's public error type wraps exactly once.
def wrap (cause : Cause) : Wrapped Cause :=
  { cause := cause }

-- One boundary keeps the original typed error available.
theorem one_boundary_retains_cause (cause : Cause) : (wrap cause).cause = cause := by
  -- Reading the field reverses the wrapper construction.
  rfl

-- A facade can wrap the crate error without losing its specific cause.
theorem two_boundaries_retain_cause (cause : Cause) :
    (wrap (wrap cause)).cause.cause = cause := by
  -- Reading both wrappers recovers the same original value.
  rfl

-- A boundary's classification comes from the retained cause.
def observeBoundary (observe : Cause -> Observation) (error : Wrapped Cause) : Observation :=
  -- The wrapper supplies no new classification that could override the cause.
  observe error.cause

-- Wrapping a cause preserves every observer, including HTTP status and retry decisions.
theorem boundary_preserves_observation (observe : Cause -> Observation) (cause : Cause) :
    observeBoundary observe (wrap cause) = observe cause := by
  -- The observer receives the exact same cause.
  rfl

-- A read port keeps its existing context and the original index detail.
theorem boundary_keeps_index_message (detail : Cause -> String)
    (context : String) (cause : Cause) :
    context ++ observeBoundary detail (wrap cause) = context ++ detail cause := by
  -- The read port formats the retained cause, without an owning wrapper's prefix.
  rfl

-- Result conversion changes only the failure branch.
def atBoundary (result : Except Cause Value) : Except (Wrapped Cause) Value :=
  match result with
  -- A successful value crosses the boundary unchanged.
  | .ok value => .ok value
  -- A failure gains a typed wrapper owned by the next boundary.
  | .error cause => .error (wrap cause)

-- Error conversion cannot change a successful result.
theorem boundary_preserves_success (value : Value) :
    atBoundary (Cause := Cause) (.ok value) = .ok value := by
  -- The success branch returns that exact value.
  rfl

-- Error conversion retains a failed result's exact cause.
theorem boundary_preserves_failure (cause : Cause) :
    atBoundary (Value := Value) (.error cause) = .error (wrap cause) := by
  -- The failure branch adds only the wrapper.
  rfl

-- A rejected owned value can be handed back to its caller without changing it.
def rejectOwned (value : Rejected) : Except Rejected Accepted :=
  -- No other error wrapper replaces the caller's value.
  .error value

-- Returning the owned receiver as the error retains that exact receiver.
theorem rejection_retains_receiver (value : Rejected) :
    rejectOwned (Accepted := Accepted) value = .error value := by
  -- Rejection constructs only this error branch.
  rfl

-- A parameter belongs to a free function, an inherent method or a trait contract.
inductive GenericOwner where
  -- The function chooses its signature without an implementor's contract.
  | function
  -- An inherent method is part of an owned API, including a static method.
  | inherentMethod
  -- The defining trait chooses the contract implementors must follow.
  | traitContract

-- An owned API needs caller support; a trait keeps its implementor-selected error.
def mayForwardGeneric (owner : GenericOwner) (callerSupported : Bool) : Bool :=
  -- Which contract owns the parameter decides whether support is required.
  match owner with
  -- A function cannot manufacture an exemption with a return-only parameter.
  | .function => callerSupported
  -- A static inherent method has the same responsibility as a function.
  | .inherentMethod => callerSupported
  -- A trait's generic error is part of the contract its caller selected.
  | .traitContract => true

-- A return-only function parameter cannot bypass the crate boundary.
theorem return_only_error_rejected : mayForwardGeneric .function false = false := by
  -- There is no caller support for this owned error.
  rfl

-- Moving a return-only parameter onto an inherent impl cannot bypass the boundary.
theorem inherent_return_only_error_rejected :
    mayForwardGeneric .inherentMethod false = false := by
  -- No receiver, argument or associated error supports the parameter.
  rfl

-- Caller-supported errors pass through each kind of generic extension unchanged.
theorem input_error_forwarded (owner : GenericOwner) :
    mayForwardGeneric owner true = true := by
  -- Each owner either requires the support already present or owns a trait contract.
  cases owner <;> rfl

-- A trait's error parameter remains its implementor's contract without a value input.
theorem trait_error_contract_forwarded (callerSupported : Bool) :
    mayForwardGeneric .traitContract callerSupported = true := by
  -- The contract, rather than the method's arguments, supplies its provenance.
  rfl

-- A private failure either already belongs to its crate or needs that crate's wrapper.
inductive LocalFailure (Owned Specific : Type) where
  -- An owned failure must not acquire a redundant crate wrapper.
  | owned (cause : Owned)
  -- A specific refusal is retained in a crate-owned variant.
  | specific (cause : Specific)

-- A public boundary reuses owned causes and wraps only specific refusals.
def ownOrWrap (wrapper : Specific -> Owned) (error : LocalFailure Owned Specific) : Owned :=
  -- Choose by which layer owns the existing cause.
  match error with
  -- This is already the owning crate's error value.
  | .owned cause => cause
  -- This narrower cause needs the owning variant once.
  | .specific cause => wrapper cause

-- Reusing an owned cause preserves its exact value.
theorem owned_cause_is_reused (wrapper : Specific -> Owned) (cause : Owned) :
    ownOrWrap wrapper (.owned cause) = cause := by
  -- No conversion is applied to an already owned error.
  rfl

-- A specific refusal receives exactly the selected owning wrapper.
theorem specific_cause_is_wrapped (wrapper : Specific -> Owned) (cause : Specific) :
    ownOrWrap wrapper (.specific cause) = wrapper cause := by
  -- Only the specific branch calls the wrapper.
  rfl

-- A datastore error keeps its origin, its conflict facts, or a caller's opaque value.
inductive DatastoreFailure (Cause Conflict Abort : Type) where
  -- Legacy lifecycle APIs retain their existing storage context.
  | storage (cause : Cause)
  -- Newly owned raw relays retain the former bare engine cause.
  | engine (cause : Cause)
  -- The retry budget ended with these same conflict facts.
  | exhausted (conflict : Conflict)
  -- The caller owns this value; moving it must not change or copy it.
  | abort (value : Abort)
  -- The transaction already committed.
  | committed
  -- The transaction already discarded its changes.
  | discarded
  -- A namespace view still keeps the transaction in use.
  | inUse

-- Split a caller abort from a default error whose abort value cannot exist.
def splitDatastoreAbort (error : DatastoreFailure Cause Conflict Abort) :
    Sum Abort (DatastoreFailure Cause Conflict Empty) :=
  -- Preserve every non-abort constructor and its original data.
  match error with
  -- The caller gets the exact value it supplied.
  | .abort value => .inl value
  -- A contextual storage cause keeps its context.
  | .storage cause => .inr (.storage cause)
  -- A raw engine cause stays raw.
  | .engine cause => .inr (.engine cause)
  -- Conflict facts move without reclassification.
  | .exhausted conflict => .inr (.exhausted conflict)
  -- Lifecycle outcomes keep their identities.
  | .committed => .inr .committed
  -- Discard is distinct from commit.
  | .discarded => .inr .discarded
  -- An outstanding view is distinct from either completed outcome.
  | .inUse => .inr .inUse

-- Extracting a caller abort returns its exact original value.
theorem datastore_abort_preserved (value : Abort) :
    splitDatastoreAbort (Cause := Cause) (Conflict := Conflict) (.abort value) = .inl value := by
  -- No reconstruction or cloning touches the caller's value.
  rfl

-- Removing the abort carrier does not add storage context to a raw relay.
theorem datastore_engine_preserved (cause : Cause) :
    splitDatastoreAbort (Conflict := Conflict) (Abort := Abort) (.engine cause) = .inr (.engine cause) := by
  -- The same engine constructor holds the same cause.
  rfl

-- Removing the abort carrier does not strip legacy storage context.
theorem datastore_context_preserved (cause : Cause) :
    splitDatastoreAbort (Conflict := Conflict) (Abort := Abort) (.storage cause) = .inr (.storage cause) := by
  -- The old contextual constructor is retained.
  rfl

-- A facade keeps its old storage error or the original datastore context.
inductive StoreFacadeFailure (Cause Conflict : Type) where
  -- This is the storage wrapper used before a raw relay became datastore-owned.
  | storage (cause : Cause)
  -- Existing datastore-owned lifecycle errors keep their original wrapper.
  | datastore (cause : DatastoreFailure Cause Conflict Empty)
  -- This is the facade's existing retry-exhaustion classification.
  | exhausted (conflict : Conflict)

-- Normalize only the new raw relays and retry carrier at the facade boundary.
def normalizeDatastore (error : DatastoreFailure Cause Conflict Empty) :
    StoreFacadeFailure Cause Conflict :=
  -- Preserve all prior facade shapes by the error's recorded origin.
  match error with
  -- A new raw relay restores the old direct storage cause.
  | .engine cause => .storage cause
  -- Exhaustion restores its old typed conflict classification.
  | .exhausted conflict => .exhausted conflict
  -- A default error cannot contain a caller abort.
  | .abort impossible => nomatch impossible
  -- Every existing lifecycle error remains wrapped as a datastore error.
  | other => .datastore other

-- A newly owned engine relay reaches the facade's original storage variant.
theorem datastore_relay_restores_storage (cause : Cause) :
    normalizeDatastore (Conflict := Conflict) (.engine cause) = .storage cause := by
  -- Only the new relay constructor receives this normalization.
  rfl

-- An older storage-context error keeps its existing datastore wrapper.
theorem datastore_existing_context_stays_nested (cause : Cause) :
    normalizeDatastore (Conflict := Conflict) (.storage cause) = .datastore (.storage cause) := by
  -- Its context is never stripped to make a new relay convenient.
  rfl


-- A shared failure keeps its original cause and the facts used for comparisons.
structure SharedFailureSnapshot (Cause Key : Type) where
  -- Rust shares this same cause through Arc rather than reconstructing it.
  cause : Cause
  -- These immutable facts were captured once when the failure was made.
  key : Key

-- A clone keeps the same cause and the same captured facts.
def cloneFailureSnapshot (failure : SharedFailureSnapshot Cause Key) : SharedFailureSnapshot Cause Key :=
  -- Copying the record models sharing the cause and copying its frozen key.
  { cause := failure.cause, key := failure.key }

-- Cloning a failure retains its exact original cause.
theorem shared_failure_clone_retains_cause (failure : SharedFailureSnapshot Cause Key) :
    (cloneFailureSnapshot failure).cause = failure.cause := by
  -- The cloned record takes this field directly from the original record.
  rfl

-- Cloning a failure retains all the facts used for equality.
theorem shared_failure_clone_retains_key (failure : SharedFailureSnapshot Cause Key) :
    (cloneFailureSnapshot failure).key = failure.key := by
  -- The key is copied as a whole and is never recomputed from mutable display text.
  rfl

-- Two failures compare equal exactly when their frozen facts are equal.
def sameFailureKey (left right : SharedFailureSnapshot Cause Key) : Prop :=
  -- The source itself need not implement equality or cloning.
  left.key = right.key

-- Comparing two frozen keys gives the same answer in either direction.
theorem same_failure_key_symmetric (left right : SharedFailureSnapshot Cause Key)
    (same : sameFailureKey left right) : sameFailureKey right left := by
  -- Ordinary equality of immutable keys is symmetric.
  exact same.symm

-- If two comparisons agree through a middle key, the outer keys agree too.
theorem same_failure_key_transitive (left middle right : SharedFailureSnapshot Cause Key)
    (first : sameFailureKey left middle) (second : sameFailureKey middle right) :
    sameFailureKey left right := by
  -- Ordinary equality of immutable keys is transitive.
  exact first.trans second


-- A returned value can contain results behind ordinary containers.
inductive ReturnedCarrier where
  -- This value carries no result to check.
  | value
  -- A domain value's fields and variants do not make its producer fallible.
  | boundary
  -- Check this error's ownership, then inspect the successful value too.
  | result (owned callerSupported : Bool) (success : ReturnedCarrier)
  -- An Option, collection, reference or future delivers the contained value.
  | carrier (contained : ReturnedCarrier)
  -- A tuple or map can deliver result values in more than one position.
  | pair (left right : ReturnedCarrier)

-- Every delivered result must own its error or preserve a genuine caller contract.
def allReturnedErrorsOwned (shape : ReturnedCarrier) : Bool :=
  -- Inspect only delivered values, never the error's own generic payload.
  match shape with
  -- No result means no public error boundary to violate.
  | .value => true
  -- Domain data has no function-result ownership requirement of its own.
  | .boundary => true
  -- An owned outer result does not excuse a foreign result inside its success.
  | .result owned callerSupported success =>
    -- Both this error and the remaining successful value must satisfy the rule.
    (owned || callerSupported) && allReturnedErrorsOwned success
  -- Wrapping the returned value does not change its ownership requirement.
  | .carrier contained => allReturnedErrorsOwned contained
  -- Both positions must satisfy the ownership requirement.
  | .pair left right => allReturnedErrorsOwned left && allReturnedErrorsOwned right

-- A container cannot change whether its contained results comply.
theorem returned_carrier_preserves_requirement (shape : ReturnedCarrier) :
    allReturnedErrorsOwned (.carrier shape) = allReturnedErrorsOwned shape := by
  -- Removing the container exposes exactly the same check.
  rfl

-- An owned outer result cannot hide a foreign result in its successful collection.
theorem owned_outer_result_cannot_hide_foreign_result :
    allReturnedErrorsOwned (.result true false (.carrier (.result false false .value))) = false := by
  -- The inner foreign error fails its check even though the outer error is owned.
  rfl

-- Genuine caller-carried errors stay exempt when returned through a container.
theorem returned_carrier_retains_caller_contract :
    allReturnedErrorsOwned (.carrier (.result false true .value)) = true := by
  -- The container retains the caller support established by the compiler.
  rfl

-- A compliant tuple position cannot conceal a foreign result in its other position.
theorem returned_pair_checks_both_positions :
    allReturnedErrorsOwned (.pair (.result true false .value) (.result false false .value)) = false := by
  -- Both tuple positions are checked independently.
  rfl


-- A caller can supply a value, a stored field or a callback's produced value.
inductive InputProvenance where
  -- A real value of this parameter can carry its caller's refusal.
  | value (parameter : Nat)
  -- A marker mentions a type without storing a value of it.
  | marker (parameter : Nat)
  -- A receiver stores its real field, including a private field.
  | field (contents : InputProvenance)
  -- An object-safe caller contract supplies its argument or associated output.
  | contract (contents : InputProvenance)
  -- A callback consumes one shape and produces another shape.
  | callback (argument output : InputProvenance)

-- Only real stored values and callback outputs establish input provenance.
def supportsParameter (parameter : Nat) : InputProvenance -> Bool
  -- A real value supports the parameter it carries.
  | .value carried => parameter == carried
  -- Mentioning a parameter in PhantomData cannot manufacture an exception.
  | .marker _ => false
  -- A receiver forwards provenance from the value it stores.
  | .field contents => supportsParameter parameter contents
  -- Future outputs and caller-selected trait arguments remain real contracts.
  | .contract contents => supportsParameter parameter contents
  -- A callback's input cannot manufacture a produced refusal.
  | .callback _ output => supportsParameter parameter output

-- A marker receiver cannot justify returning an arbitrary generic error.
theorem marker_receiver_does_not_support (parameter : Nat) :
    supportsParameter parameter (.field (.marker parameter)) = false := by
  -- Neither the field nor the marker stores a value of that parameter.
  rfl

-- The original stored-error receiver remains a genuine caller contract.
theorem stored_error_receiver_supports (parameter : Nat) :
    supportsParameter parameter (.field (.value parameter)) = true := by
  -- The field really stores this parameter's value.
  simp [supportsParameter]

-- A written alias can describe a delivered result only at the same resolved type.
def spellingMatches (delivered written : Nat) (delivers spellsOwnedAlias : Bool) : Bool :=
  -- Comparing types comes before checking the alias's spelling.
  delivers && delivered == written && spellsOwnedAlias

-- A result mentioned in a sender cannot steal another type's return span.
theorem different_type_cannot_supply_spelling (delivered written : Nat)
    (different : Not (delivered = written)) (delivers spellsOwnedAlias : Bool) :
    spellingMatches delivered written delivers spellsOwnedAlias = false := by
  -- A failed type comparison rejects either spelling.
  simp [spellingMatches, different]

-- A sender's spelling is irrelevant even when its error type happens to match.
theorem non_delivering_spelling_is_ignored (sameType : Nat) (spellsOwnedAlias : Bool) :
    spellingMatches sameType sameType false spellsOwnedAlias = false := by
  -- The value is consumed by the sender rather than delivered by the API.
  simp [spellingMatches]

-- An erased caller future still supplies its original generic refusal.
theorem caller_object_contract_supports (parameter : Nat) :
    supportsParameter parameter (.contract (.value parameter)) = true := by
  -- Erasing the implementation type does not erase its associated error contract.
  simp [supportsParameter]

-- A callback that merely consumes an error cannot manufacture one as output.
theorem consuming_callback_does_not_supply (parameter : Nat) :
    supportsParameter parameter (.callback (.value parameter) (.marker parameter)) = false := by
  -- Only the callback output establishes this provenance.
  rfl

-- A written argument can carry a value or only choose what produces it.
inductive ParameterRole where
  -- This argument occurs at a position that supplies a value.
  | delivered
  -- This argument only labels the returned value.
  | marker
  -- This argument selects an implementation rather than supplying its output.
  | selector

-- Only a parameter in a delivered position supplies output spelling.
def suppliesSpelling (role : ParameterRole) : Bool :=
  match role with
  -- An explicitly carried result gives the spelling of that result.
  | .delivered => true
  -- Non-value arguments do not describe any delivered result type.
  | .marker => false
  -- Selecting a future is not the same as writing its output.
  | .selector => false

-- Choosing a future implementation cannot overwrite the output spelling.
theorem future_selector_does_not_supply_spelling :
    suppliesSpelling .selector = false := by
  -- The compiler checks the selected output's error owner separately.
  rfl

-- A transparent carrier argument supplies the spelling of its delivered value.
theorem delivered_parameter_supplies_spelling :
    suppliesSpelling .delivered = true := by
  -- The carrier keeps the position of its value parameter observable.
  rfl

-- Splitting a tag and passive protocol event is an infallible value operation.
theorem passive_event_split_has_no_error_boundary :
    allReturnedErrorsOwned (.pair .value .boundary) = true := by
  -- Neither value introduces a returned Result.
  rfl

-- A batch's explicit Result remains an operation failure through plain wrappers.
theorem batch_result_still_requires_owner :
    allReturnedErrorsOwned (.carrier (.pair .value (.result false false .value))) = false := by
  -- The foreign error remains visible inside the standard batch wrapper.
  rfl

-- A signer failure carries the same wire message and retry classification.
structure SigningFailureSnapshot where
  -- Existing message bytes are retained rather than prefixed at this boundary.
  message : String
  -- Only this flag lets the existing input service retry the failure.
  transient : Bool

-- Core can carry a specific signer refusal or another pre-existing core error.
inductive CoreSigningSnapshot where
  -- A typed signer failure keeps both of its original fields.
  | signer (failure : SigningFailureSnapshot)
  -- Other core failures stay permanent in the signing input service.
  | legacy (message : String)
  -- A successfully signed payload prevents retrying the complete wave.
  | signed

-- The input service reads retryability only from the typed signer variant.
def signingRetryable : CoreSigningSnapshot -> Bool
  -- This is the exact pre-existing transient bit.
  | .signer failure => failure.transient
  -- The old default converted these failures to permanent refusals.
  | .legacy _ => false
  -- Signed bytes are retained rather than sent to the signer again.
  | .signed => false

-- Wrapping a signer cause retains its retry classification.
theorem typed_signer_retains_retryability (failure : SigningFailureSnapshot) :
    signingRetryable (.signer failure) = failure.transient := by
  -- The wrapper changes no flag.
  rfl

-- Typed legacy core errors keep the default signer's permanent classification.
theorem legacy_signer_stays_permanent (message : String) :
    signingRetryable (.legacy message) = false := by
  -- Keeping the typed cause does not turn a legacy refusal into a retry.
  rfl

-- A wave can retry as a whole only when it is non-empty and every item is transient.
def signingWaveRetryable (failures : List CoreSigningSnapshot) : Bool :=
  -- A successful or permanent item makes the existing all-transient test fail.
  !failures.isEmpty && failures.all signingRetryable

-- A wave of wrapped signer failures makes the same all-transient decision.
theorem typed_wave_retains_retryability (failures : List SigningFailureSnapshot) :
    signingWaveRetryable (failures.map CoreSigningSnapshot.signer) =
      (!failures.isEmpty && failures.all SigningFailureSnapshot.transient) := by
  -- Mapping adds only the typed wrapper to each original failure.
  cases failures with
  -- No signatures means no whole-wave retry.
  | nil => rfl
  -- Mapping each cause preserves the all-transient predicate and non-emptiness.
  | cons failure remaining =>
    simp [signingWaveRetryable, signingRetryable, List.all_map, Function.comp_def]

-- A scope can finish with a typed result or with a malformed caller box.
inductive ScopeExit where
  -- The caller must provide its own transaction when the store has no scope.
  | noScope
  -- The write committed and its value, reload and edge refresh completed.
  | committed
  -- The caller refused the write before any commit.
  | refused
  -- A durable commit returned a box of the wrong value type.
  | badValue
  -- The write did not commit but returned a box of the wrong refusal type.
  | badRefusal
  -- A commit conflict left nothing to publish from this attempt.
  | conflict
  -- A store failure leaves the existing guard armed conservatively.
  | storeFailure

-- These are the outer error categories after downcasts move into the facade.
inductive ScopeReply where
  -- The store cannot run this attempt itself.
  | noScope
  -- The committed value has the expected caller type.
  | committed
  -- The caller's unchanged typed refusal ends the attempt.
  | refused
  -- A value type error may follow a durable commit.
  | valueTypeError
  -- A refusal type error cannot follow a commit.
  | refusalTypeError
  -- The store reports its existing conflict unchanged.
  | conflict
  -- The store reports its existing other failure unchanged.
  | storeFailure

-- The old DB code withdraws only while its guard is still armed.
def oldScopeWithdraws : ScopeExit -> Bool
  -- A value mismatch can follow a commit; a store failure stays conservative.
  | .badValue | .storeFailure => true
  -- Every other completed branch disarms the guard as before.
  | _ => false

-- Moving the type boundary preserves which kind of outcome occurred.
def scopeReply : ScopeExit -> ScopeReply
  -- The store still has no owned scope.
  | .noScope => .noScope
  -- The same committed tuple is decoded by the facade.
  | .committed => .committed
  -- The same refusal is decoded by the facade.
  | .refused => .refused
  -- A wrong committed tuple becomes the dedicated value type error.
  | .badValue => .valueTypeError
  -- A wrong refusal becomes the dedicated refusal type error.
  | .badRefusal => .refusalTypeError
  -- Conflict detection still uses the store's conflict variant.
  | .conflict => .conflict
  -- Other store failures retain their existing guard treatment.
  | .storeFailure => .storeFailure

-- The DB distinguishes the two type errors before its guard is dropped.
def typedScopeWithdraws : ScopeReply -> Bool
  -- These branches still leave the guard armed.
  | .valueTypeError | .storeFailure => true
  -- Refusal type errors disarm it because nothing committed.
  | _ => false

-- Moving the downcast preserves the edge snapshot decision on every exit.
theorem scope_facade_preserves_guard (outcome : ScopeExit) :
    typedScopeWithdraws (scopeReply outcome) = oldScopeWithdraws outcome := by
  -- Every possible exit keeps its original armed or disarmed state.
  cases outcome <;> rfl

-- An ownership claim either wins the guard or finds its existing owner.
inductive ClaimOutcome where
  -- The caller owns the CID and will release its guard.
  | owned
  -- The caller waits for the same existing owner's Event.
  | busy

-- The old Result represented the same two non-failing ownership outcomes.
def oldClaimOwns : Bool -> Bool :=
  -- True was Ok(guard); false was Err(wait).
  fun won => won

-- The new domain outcome records that ownership decision explicitly.
def namedClaim : Bool -> ClaimOutcome
  -- No failure or coordination step is added by this name.
  | true => .owned
  -- Waiting still means an existing owner holds the CID.
  | false => .busy

-- The same guard is retained only by the owner.
def namedClaimOwns : ClaimOutcome -> Bool
  -- This caller won the original atomic insertion.
  | .owned => true
  -- This caller still awaits the original owner's release.
  | .busy => false

-- Renaming the outcomes cannot create a second owner or change a waiter.
theorem claim_naming_keeps_owner (won : Bool) :
    namedClaimOwns (namedClaim won) = oldClaimOwns won := by
  -- Both possible outcomes preserve the original atomic decision.
  cases won <;> rfl

end ErrorBoundary
