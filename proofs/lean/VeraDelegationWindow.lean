/-!
When a vera.rs delegation is honoured, and what a clock that is off does to it.

Who acts: a signer, whose clock may be off, writes a delegation; a node checks
it before it sends anything; vera.rs checks it again at the time of the block
that executes it, the provider's time.

A delegation names three times, in seconds: `nbf` (not before), `iat` (issued
at) and `exp` (expires at). vera.rs takes a token only when `nbf ≤ iat < exp`,
and acts under it at block time `t` only when `nbf ≤ t ≤ exp`. A delegation
bound to one request also names an operation identity that expires at `exp`:
vera.rs refuses it once `t ≥ exp`, and refuses it while `exp` is more than 600
seconds past `t`.

The signer writes a request-bound delegation at its own clock `c` with `exp =
c + lifetime` and `nbf = iat = c - allowance`: set back by the clock skew
allowance of 30 seconds that a plain delegation always had (a plain one keeps
`iat = c`). vera.rs's own client writes an operation token with `nbf = iat`
and checks its operation identity at `iat`, so both move together.

What must never happen:
- a signer whose clock is ahead of the provider's by no more than the
  allowance writes a delegation the provider refuses;
- a delegation written at the provider's time is refused at any block time
  within its lifetime;
- the node calls a clock off when the signer's clock is within the allowance,
  or fails to call it off when it is not.

Rust owners: `issue_request_at` in `crates/vera/src/vera_rs/bearer/request.rs`
and `issue_at` in `crates/vera/src/vera_rs/bearer.rs` write the times;
`within_window` in `crates/vera/src/vera_rs/bearer/window.rs` is the node's
check; `DelegationWindow::clock_is_off` in `crates/acp/src/delegation_window.rs`
names the clock. The verifier mirrored is `verify_bearer_token`,
`JwtClaims::authorize` and `OperationId::validate` in vera.rs `vera-crypto`.

Times are natural numbers, as Unix seconds are; a subtraction that would go
below zero stops at zero, as Rust's `saturating_sub` does.
-/
namespace VeraDelegationWindow

/-- The clock skew allowance, in seconds (`CLOCK_SKEW_ALLOWANCE`). -/
def allowance : Nat := 30

/-- The longest vera.rs keeps a request open, in seconds (`MAX_OPERATION_TTL`). -/
def maxTtl : Nat := 600

/-- The longest lifetime a request-bound delegation is written with
(`MAX_REQUEST_LIFETIME`): the open request less the allowance. -/
def maxRequestLifetime : Nat := maxTtl - allowance

/-- The three times a delegation names. -/
structure Claims where
  /-- Not before. -/
  nbf : Nat
  /-- Issued at. -/
  iat : Nat
  /-- Expires at. -/
  exp : Nat

/-- The request-bound delegation a signer writes at its clock `c` for
`lifetime` seconds. -/
def issue (c lifetime : Nat) : Claims :=
  -- Started and issued the allowance before now, ending a lifetime from now.
  { nbf := c - allowance, iat := c - allowance, exp := c + lifetime }

/-- The plain delegation a signer writes at its clock `c`. -/
def issuePlain (c lifetime : Nat) : Claims :=
  -- Started the allowance before now, issued now, ending a lifetime from now.
  { nbf := c - allowance, iat := c, exp := c + lifetime }

/-- The request-bound delegation as it was written before the allowance. -/
def issueWithoutAllowance (c lifetime : Nat) : Claims :=
  -- Not before the moment it was written.
  { nbf := c, iat := c, exp := c + lifetime }

/-- vera.rs's shape check on a token: `nbf ≤ iat < exp`. -/
def wellFormed (claims : Claims) : Prop :=
  -- It cannot start after it was issued, and it ends after it was issued.
  claims.nbf ≤ claims.iat ∧ claims.iat < claims.exp

/-- vera.rs's time check on a plain delegation at block time `t`. -/
def plainHolds (claims : Claims) (t : Nat) : Prop :=
  -- Well formed, started, and not yet over.
  wellFormed claims ∧ claims.nbf ≤ t ∧ t ≤ claims.exp

/-- vera.rs's time checks on a request-bound delegation at block time `t`: the
plain ones, and its operation identity's. -/
def requestHolds (claims : Claims) (t : Nat) : Prop :=
  -- The plain checks.
  plainHolds claims t ∧
  -- The operation identity is still open at `t`...
  t < claims.exp ∧
  -- ...and does not reach further than an open request may.
  claims.exp ≤ t + maxTtl

/-- A delegation written with a positive lifetime is well formed. -/
theorem issued_is_well_formed (c lifetime : Nat) (positive : 0 < lifetime) :
    wellFormed (issue c lifetime) := by
  -- `c - 30 ≤ c - 30` and `c - 30 < c + lifetime`.
  simp only [wellFormed, issue]
  omega

/-- vera.rs's own client's checks on an operation token: it starts when it is
issued, ends after, and its operation identity is open at its issue time. -/
def builderAccepts (claims : Claims) : Prop :=
  -- `nbf = iat < exp`, and the identity reaches no further than an open request.
  claims.nbf = claims.iat ∧ claims.iat < claims.exp ∧ claims.exp ≤ claims.iat + maxTtl

/-- Every request-bound delegation written with a lifetime vera.rs's builder
allows is one that builder would write: the allowance is spent inside the
open request, not added to it. -/
theorem issued_is_what_veras_builder_writes (c lifetime : Nat) (positive : 0 < lifetime)
    (bounded : lifetime ≤ maxRequestLifetime) (sane : allowance ≤ c) :
    builderAccepts (issue c lifetime) := by
  -- Unfold and solve the arithmetic.
  simp only [builderAccepts, issue, maxRequestLifetime, allowance, maxTtl] at *
  -- The start is the issue time by construction; the rest is arithmetic.
  exact ⟨trivial, by omega, by omega⟩

/-- Exactly when vera.rs acts under a request-bound delegation written at `c`:
from the allowance before `c`, until the end of its lifetime, and while its
end is no more than an open request past the block time. -/
theorem request_holds_iff (c lifetime t : Nat) :
    requestHolds (issue c lifetime) t ↔
      c - allowance ≤ t ∧ t < c + lifetime ∧ c + lifetime ≤ t + maxTtl := by
  -- Unfold every check and solve the arithmetic.
  simp only [requestHolds, plainHolds, wellFormed, issue, allowance, maxTtl]
  omega

/-- A request-bound delegation written at the provider's time `p` is honoured
at every block time in its lifetime: a node that hands its signer the
provider's time takes the signer's clock out of the question. -/
theorem written_at_provider_time_holds (p lifetime t : Nat) (positive : 0 < lifetime)
    (bounded : lifetime ≤ maxTtl) (started : p ≤ t) (within : t < p + lifetime) :
    requestHolds (issue p lifetime) t := by
  -- By the exact rule above.
  rw [request_holds_iff p lifetime t]
  simp only [allowance, maxTtl] at *
  omega

/-- A signer whose clock is ahead of the provider's by `ahead` seconds, no more
than the allowance, writes a delegation the provider honours on arrival, for
any lifetime it may write. -/
theorem ahead_within_allowance_holds (t ahead lifetime : Nat) (positive : 0 < lifetime)
    (bounded : lifetime ≤ maxRequestLifetime) (small : ahead ≤ allowance) :
    requestHolds (issue (t + ahead) lifetime) t := by
  -- By the exact rule, at `c = t + ahead`.
  rw [request_holds_iff (t + ahead) lifetime t]
  simp only [maxRequestLifetime, allowance, maxTtl] at *
  omega

/-- A signer whose clock is ahead by more than the allowance writes a delegation
the provider does not yet honour. -/
theorem ahead_beyond_allowance_refused (t ahead lifetime : Nat)
    (large : allowance < ahead) : ¬ requestHolds (issue (t + ahead) lifetime) t := by
  -- It starts after `t`.
  simp only [requestHolds, plainHolds, issue, allowance] at *
  omega

/-- A signer whose clock is behind by at least the lifetime writes a delegation
that is over before the provider sees it. -/
theorem behind_by_the_lifetime_refused (c lifetime t : Nat) (late : c + lifetime ≤ t) :
    ¬ requestHolds (issue c lifetime) t := by
  -- Its operation identity is closed at `t`.
  simp only [requestHolds, issue] at *
  omega

/-- The longest lifetime is the longest that keeps the previous promise: one
second more, and a signer one allowance ahead is refused. -/
theorem longer_than_the_longest_lifetime_refused_ahead (t : Nat) :
    ¬ requestHolds (issue (t + allowance) (maxRequestLifetime + 1)) t := by
  -- Its end lies more than an open request past `t`.
  simp only [requestHolds, issue, maxRequestLifetime, allowance, maxTtl]
  omega

/-- The plain delegation, written the same way, is honoured on arrival from a
signer ahead by no more than the allowance. -/
theorem plain_ahead_within_allowance_holds (t ahead lifetime : Nat) (positive : 0 < lifetime)
    (small : ahead ≤ allowance) : plainHolds (issuePlain (t + ahead) lifetime) t := by
  -- Started by the allowance, not yet over.
  simp only [plainHolds, wellFormed, issuePlain, allowance] at *
  omega

/-- RED: written without the allowance, as request-bound delegations were, a
signer one second ahead is refused. This is the defect the allowance removes. -/
theorem without_allowance_one_second_ahead_refused (t lifetime : Nat) :
    ¬ requestHolds (issueWithoutAllowance (t + 1) lifetime) t := by
  -- It starts one second after `t`.
  simp only [requestHolds, plainHolds, issueWithoutAllowance]
  omega

/-- The signer's clock when it wrote `claims`: the allowance after the start,
or the issue time where that is later (`signer_clock`). -/
def signerClock (claims : Claims) : Nat :=
  -- Both kinds of delegation start the allowance before the signer's clock.
  max claims.iat (claims.nbf + allowance)

/-- Either kind of delegation names its signer's clock exactly. -/
theorem signer_clock_is_the_clock (c lifetime : Nat) (sane : allowance ≤ c) :
    signerClock (issue c lifetime) = c ∧ signerClock (issuePlain c lifetime) = c := by
  -- `max (c - 30) c = c` and `max c c = c`.
  simp only [signerClock, issue, issuePlain, allowance] at *
  omega

/-- How far the signer's clock was from the provider's, in signed seconds:
positive when the signer was ahead. -/
def offset (claims : Claims) (t : Nat) : Int :=
  -- The signer's clock against the provider's time.
  (signerClock claims : Int) - (t : Int)

/-- The node's word on a refused delegation: its signer's clock is off when it
issued more than the allowance away from the provider's time. -/
def clockIsOff (claims : Claims) (t : Nat) : Prop :=
  -- Either way, by more than the allowance.
  allowance < (offset claims t).natAbs

/-- A signer whose clock is within the allowance of the provider's is never told
its clock is off about a delegation it just wrote. -/
theorem within_allowance_is_not_off (t lifetime : Nat) (skew : Int)
    (small : skew.natAbs ≤ allowance) (sane : (allowance : Int) ≤ (t : Int) + skew) :
    ¬ clockIsOff (issue ((t : Int) + skew).toNat lifetime) t := by
  -- The offset is the skew itself.
  simp only [clockIsOff, offset, signerClock, issue, allowance] at *
  omega

/-- A signer whose clock is more than the allowance away is told its clock is
off about a delegation it just wrote. -/
theorem beyond_allowance_is_off (t lifetime : Nat) (skew : Int)
    (large : allowance < skew.natAbs) (sane : (allowance : Int) ≤ (t : Int) + skew) :
    clockIsOff (issue ((t : Int) + skew).toNat lifetime) t := by
  -- The offset is the skew itself.
  simp only [clockIsOff, offset, signerClock, issue, allowance] at *
  omega

/-- A refusal the node calls a clock that is off is cured by writing the
delegation again at the provider's time it reports: written there, it is
honoured at every block time of its lifetime. -/
theorem rewritten_at_reported_time_holds (claims : Claims) (p lifetime t : Nat)
    (_off : clockIsOff claims p) (positive : 0 < lifetime) (bounded : lifetime ≤ maxTtl)
    (started : p ≤ t) (within : t < p + lifetime) : requestHolds (issue p lifetime) t :=
  -- What was refused does not matter: the new one is written at `p`.
  written_at_provider_time_holds p lifetime t positive bounded started within

end VeraDelegationWindow
