import Lean

/-!
An application can wrap the fully assembled HTTP router. The database router
keeps its native authentication; application routes keep their own handlers.
The optional wrapper is constructed once, after merging routes and fallback.
This models a forwarding filter: it either refuses a request or forwards it
to the unchanged inner router. An arbitrary application callback is trusted
code; this model does not prove arbitrary callback bodies preserve access.
Rust owners: crates/http/src/server.rs::Server::router and
crates/defra-node/src/config.rs::HttpConfig::with_router_transform.
-/

namespace HttpRouterComposition
open Lean

-- Each build owns its core, extension merge, optional wrapper and result.
inductive Stage where
  | native | merge | transform | serve
  deriving DecidableEq, BEq

-- An absent callback leaves only the original assembly steps.
def trace (configured : Bool) : List Stage :=
  [.native, .merge] ++ (if configured then [.transform] else []) ++ [.serve]

-- One callback runs after composition and before any request can be served.
theorem configured_order :
    trace true = [.native, .merge, .transform, .serve] := by
  rfl

-- Rebuilding a router calls its constructor once, never once per route.
theorem constructor_once (configured : Bool) :
    (trace configured).count .transform = if configured then 1 else 0 := by
  cases configured <;> rfl

-- The native test supplies its real inner response, not a second auth policy.
structure Input where
  configured : Bool
  allowed : Bool
  innerStatus : Nat
  deriving FromJson, ToJson

-- A forwarding filter can refuse; otherwise the inner owner decides.
def project (input : Input) : Nat :=
  if input.configured && !input.allowed then 421 else input.innerStatus

-- Disabling the optional filter preserves every native or extension response.
theorem absent_preserves (input : Input) (h : input.configured = false) :
    project input = input.innerStatus := by
  simp [project, h]

-- An accepted request preserves the original authentication outcome exactly.
theorem forwarding_preserves (input : Input) (h : input.allowed = true) :
    project input = input.innerStatus := by
  simp [project, h]

-- A configured refusal applies equally to core, extension and fallback paths.
theorem refuses_every_inner_status (input : Input)
    (configured : input.configured = true) (refused : input.allowed = false) :
    project input = 421 := by
  simp [project, configured, refused]

-- Expectations are computed from the native observations sent on stdin.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("expectedStatus", toJson (project input))]

end HttpRouterComposition

-- The conformance test executes this owner against the router it constructed.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= HttpRouterComposition.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
