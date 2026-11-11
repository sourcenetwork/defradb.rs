import Lean

/-!
An authenticated client opens one signing connection. The connection holds one
of a bounded number of slots, and its token must still be unexpired when the
request starts. Its lifetime ends no later than that token's expiration or the
server's shorter session deadline. Releasing a slot restores capacity.
TLA+ checks the races between opening and ending different connections.
Rust owners: crates/http/src/client_signing.rs and router/state.rs.
-/

namespace ClientSigningAdmission
open Lean

-- A connection may open only while a slot remains.
def admitted (used limit : Nat) : Bool := used < limit

-- A token is usable strictly before its expiration instant.
def unexpired (now expiration : Nat) : Bool := now < expiration

-- A session cannot outlive either its configured limit or its bearer token.
def lifetime (now expiration limit : Nat) : Nat :=
  if unexpired now expiration then min (expiration - now) limit else 0

-- A full admission budget refuses another connection.
theorem full_refused (used limit : Nat) (full : limit ≤ used) : admitted used limit = false := by
  simp [admitted, Nat.not_lt.mpr full]

-- An already expired token has no session lifetime.
theorem expired_has_no_lifetime (now expiration limit : Nat) (expired : expiration ≤ now) :
    lifetime now expiration limit = 0 := by
  simp [lifetime, unexpired, Nat.not_lt.mpr expired]

-- A permitted connection never lasts beyond its configured deadline.
theorem lifetime_bounded (now expiration limit : Nat) : lifetime now expiration limit ≤ limit := by
  simp only [lifetime]
  split
  · exact Nat.min_le_right _ _
  · exact Nat.zero_le _

-- A permitted connection never lasts beyond the token's remaining validity.
theorem lifetime_before_expiration (now expiration limit : Nat) :
    lifetime now expiration limit ≤ expiration - now := by
  simp only [lifetime]
  split
  · exact Nat.min_le_left _ _
  · exact Nat.zero_le _

-- The implementation test sends its real observations through this owner.
structure Input where
  used : Nat
  limit : Nat
  now : Nat
  expiration : Nat
  duration : Nat
  deriving FromJson, ToJson

-- Generate the expected admission, expiry and deadline for each input.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("admitted", toJson (admitted input.used input.limit)),
     ("unexpired", toJson (unexpired input.now input.expiration)),
     ("lifetime", toJson (lifetime input.now input.expiration input.duration))]

end ClientSigningAdmission

-- Tests use the model, rather than a second implementation of its decisions.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSigningAdmission.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
