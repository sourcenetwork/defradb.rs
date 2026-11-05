import Acp.OwnerObservation

-- The native tests supply the actual mode and gate facts and ask the Lean owner
-- for their decision; no test-only policy machine computes the expectation.
open Acp.OwnerObservation

-- Decode a Boolean fact supplied by the native test adapter.
def fact (s : String) : Bool := s == "1"

-- Return only the modeled decision name, never document metadata.
def decisionName : AccessDecision → String
  | .allowed => "allowed"
  | .denied => "denied"
  | .unavailable => "unavailable"
  | .invalid => "invalid"

-- Evaluate the exact fact vector the calling test observed or configured.
def main (args : List String) : IO UInt32 := do
  match args with
  | [mode, owner, bound, context, principal, app, read, member, registered, active, node, named] =>
    let evidence : AccessEvidence := ⟨fact owner, fact bound, fact context, fact principal,
      fact app, fact read, fact member, fact registered, fact active, fact node, fact named⟩
    let accessMode := if mode == "read" then AccessMode.delegatedRead else AccessMode.named
    IO.println (decisionName (accessDecision accessMode evidence))
    return 0
  | _ =>
    IO.eprintln "expected mode and eleven authorization facts"
    return 2
