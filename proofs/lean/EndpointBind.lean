import Lean

/-!
An explicit listener address replaces preset sockets, including the other IP
family. A port without an address keeps wildcard IPv4 and optional IPv6. With
neither option the Minimal preset remains unchanged. Port zero asks the OS for
an ephemeral port; native conformance observes its actual nonzero assignment.
The Minimal preset's IPv4-required/IPv6-optional contract is an external premise.
Rust owner: crates/p2p/src/iroh/endpoint_config.rs::apply_bind_config.
-/

namespace EndpointBind
open Lean

-- The native IP parser supplies the address text and its address family.
structure Address where
  ip : String
  ipv6 : Bool
  deriving DecidableEq, FromJson, ToJson

-- Both options preserve their absent-versus-present representation.
structure Request where
  addr : Option Address
  port : Option Nat
  deriving FromJson, ToJson

-- A required binding must exist; an optional IPv6 binding may be unavailable.
structure Socket where
  address : Address
  port : Nat
  required : Bool
  deriving DecidableEq, ToJson

-- The preset asks for an ephemeral wildcard socket in each supported family.
def wildcards (port : Nat) : List Socket :=
  [{ address := { ip := "0.0.0.0", ipv6 := false }, port, required := true },
   { address := { ip := "::", ipv6 := true }, port, required := false }]

-- An explicit address replaces, rather than extends, the preset transport set.
def plan (request : Request) : List Socket :=
  match request.addr with
  | some address => [{ address, port := request.port.getD 0, required := true }]
  | none => wildcards (request.port.getD 0)

-- No socket with another address or address family survives explicit selection.
theorem explicit_replaces (request : Request) (address : Address)
    (h : request.addr = some address) :
    plan request = [{ address, port := request.port.getD 0, required := true }] := by
  -- The explicit branch returns exactly the requested binding.
  simp [plan, h]

-- Every explicit socket keeps the requested address and port selection.
theorem explicit_preserves (request : Request) (address : Address) (socket : Socket)
    (h : request.addr = some address) (member : socket ∈ plan request) :
    socket.address = address ∧ socket.port = request.port.getD 0 ∧ socket.required = true := by
  -- Membership in the singleton identifies the entire socket.
  simp [plan, h] at member
  subst socket
  exact ⟨rfl, rfl, rfl⟩

-- Omitting both options leaves the Minimal preset's bindings intact.
theorem absent_preserves_defaults : plan { addr := none, port := none } = wildcards 0 := by
  -- The absent port uses the preset's ephemeral request.
  rfl

-- A port-only request applies that port to both wildcard families.
theorem port_only_preserves_dual_stack (port : Nat) :
    plan { addr := none, port := some port } = wildcards port := by
  -- Both wildcard records are produced by the same owner.
  rfl

-- Actual native inputs are decoded and evaluated, not replaced by fixture expectations.
def evaluate (input : Json) : Except String Json := do
  let requests : Array Request ← fromJson? input
  pure <| toJson <| requests.map fun request =>
    Json.mkObj [("input", toJson request), ("expected", toJson (plan request))]

end EndpointBind

-- The native binding supplies its actual requests on stdin and reads model results.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  let result := Lean.Json.parse input >>= EndpointBind.evaluate
  match result with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
