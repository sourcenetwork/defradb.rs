/-!
# A PushLog signed for Go's protocol and for Rust's

The story, in plain words. A Rust node signs a PushLog request before it
knows which protocol the receiver speaks. A request may carry a capability,
a field only Rust knows. Go checks a signature by re-encoding only the
fields it knows, so a request whose signature covers the capability fails
Go's check. The sender therefore signs two forms: one without the
capability for Go's protocol, and one with it for Rust's. What must never
happen: Go is handed a form it cannot verify, or Rust is handed a form that
lost the capability. The two iroh stream flags are Rust-only in the same
way; libp2p leaves them false, and Go's signed form clears them too.

Rust: `PushLogOffer::sign` and `PushLogOffer::for_protocol` in
`crates/database/p2p/src/two_stream/push_offer.rs`.
-/
namespace PushLogOffer

-- A request: the fields Go knows (one number stands for all of them), the
-- message ID, and the three Rust-only fields.
structure Request where
  goFields : Nat
  messageId : Nat
  cap : Option Nat
  sameStreamReply : Bool
  retryAfter : Bool
deriving DecidableEq

-- What a signature covers: Rust encodes every field the request carries.
def rustEncoding (r : Request) : Nat × Nat × Option Nat × Bool × Bool :=
  (r.goFields, r.messageId, r.cap, r.sameStreamReply, r.retryAfter)

-- What Go re-encodes to check a signature: only its own fields, as if the
-- three Rust-only fields were absent.
def goEncoding (r : Request) : Nat × Nat × Option Nat × Bool × Bool :=
  (r.goFields, r.messageId, none, false, false)

-- A signed request: the request and the encoding its signature covers.
structure Signed where
  request : Request
  signedOver : Nat × Nat × Option Nat × Bool × Bool

-- Signing covers the request exactly as it is sent.
def sign (r : Request) : Signed := ⟨r, rustEncoding r⟩

-- Go accepts a signature over what it re-encodes.
def goVerifies (s : Signed) : Prop := s.signedOver = goEncoding s.request

-- Rust accepts a signature over what it received.
def rustVerifies (s : Signed) : Prop := s.signedOver = rustEncoding s.request

-- The two forms: Go's has every Rust-only field cleared before signing;
-- Rust's exists only when there is a capability to carry and retains the
-- other Rust-only fields as well.
structure Offer where
  goCompatible : Signed
  extended : Option Signed

-- Build both forms from one request, so they share its message ID.
def offer (r : Request) : Offer :=
  { goCompatible := sign { r with cap := none, sameStreamReply := false, retryAfter := false }
    extended := r.cap.map fun _ => sign r }

-- The form written once the protocol is known: Rust's form on Rust's
-- protocol when there is one, Go's form otherwise.
def forProtocol (o : Offer) (rustProtocol : Bool) : Signed :=
  match o.extended, rustProtocol with
  | some extended, true => extended
  | _, _ => o.goCompatible

-- Go verifies whatever is sent on Go's protocol.
theorem go_verifies_go_protocol (r : Request) :
    goVerifies (forProtocol (offer r) false) := by
  cases h : r.cap <;> simp [forProtocol, offer, goVerifies, sign, rustEncoding, goEncoding, h]

-- Go's protocol never carries the capability.
theorem go_protocol_has_no_capability (r : Request) :
    (forProtocol (offer r) false).request.cap = none := by
  cases h : r.cap <;> simp [forProtocol, offer, sign, h]

-- Go's protocol never carries any of the Rust-only fields.
theorem go_protocol_has_only_go_fields (r : Request) :
    (forProtocol (offer r) false).request.sameStreamReply = false
      ∧ (forProtocol (offer r) false).request.retryAfter = false := by
  cases h : r.cap <;> simp [forProtocol, offer, sign, h]

-- Rust's protocol keeps the capability the request had.
theorem rust_protocol_keeps_capability (r : Request) :
    (forProtocol (offer r) true).request.cap = r.cap := by
  cases h : r.cap <;> simp [forProtocol, offer, sign, h]

-- Rust verifies whichever form it is sent.
theorem rust_verifies_either (r : Request) (p : Bool) :
    rustVerifies (forProtocol (offer r) p) := by
  cases h : r.cap <;> cases p <;> simp [forProtocol, offer, rustVerifies, sign, h]

-- Both forms carry the request's message ID, so the reply matches either.
theorem same_message_id (r : Request) (p : Bool) :
    (forProtocol (offer r) p).request.messageId = r.messageId := by
  cases h : r.cap <;> cases p <;> simp [forProtocol, offer, sign, h]

-- The old sender signed the request as it stood: with a capability, Go
-- cannot verify it.
theorem signing_the_capability_fails_go (r : Request) (c : Nat) (h : r.cap = some c) :
    ¬ goVerifies (sign r) := by
  simp [goVerifies, sign, rustEncoding, goEncoding, h]

end PushLogOffer
