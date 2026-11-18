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
- the node claims more about the signer's clock than the delegation's own
  times prove: a delegation that is not valid yet proves the clock ahead by
  at least the gap to its start, and one that has ended proves nothing, since
  a delegation held past its lifetime by a signer whose clock is right ends
  the same way.

Rust owners: `issue_request_at` in `crates/vera-delegation/src/bearer/request.rs`
and `create_scoped_bearer_token_at` in `crates/vera-delegation/src/bearer.rs`
write the times;
`within_window` in `crates/vera/src/vera_rs/bearer/window.rs` is the node's
check and names the refusal, a `WindowRefusal` of
`crates/acp/src/delegation_window.rs`. The verifier mirrored is `verify_bearer_token`,
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

/-- Why the node refuses a request-bound delegation at time `t`, mirroring
`WindowRefusal`. Only a delegation that is not valid yet carries a number. -/
inductive Refusal where
  /-- It starts after `t`; the signer's clock is ahead by at least this much. -/
  | notYetValid (aheadByAtLeast : Nat)
  /-- It has ended: nothing is claimed about the clock. -/
  | expired
  /-- It stays open longer past `t` than vera.rs keeps a request open. -/
  | outlastsRequest
  deriving DecidableEq

/-- The node's classification of a refused delegation (`within_window`):
first whether it has started, then whether it has ended. -/
def classify (claims : Claims) (t : Nat) : Refusal :=
  -- A start after `t`: the gap, and nothing added for a set-back start.
  if t < claims.nbf then .notYetValid (claims.nbf - t)
  -- An end at or before `t` (the operation identity closes at `exp`).
  else if claims.exp ≤ t then .expired
  -- Otherwise it reaches too far.
  else .outlastsRequest

/-- A signer whose clock read `c` cannot have written a start later than `c`.
Whatever it did with its start (set it back, as DefraDB does, or not), the
lead the node reports never exceeds the signer's true lead `c - t`. -/
theorem reported_lead_is_a_lower_bound (claims : Claims) (c t : Nat)
    (honest : claims.nbf ≤ c) (early : t < claims.nbf) :
    ∃ lead, classify claims t = .notYetValid lead ∧ lead ≤ c - t := by
  -- The reported lead is the gap to the start, which lies at or before `c`.
  refine ⟨claims.nbf - t, ?_, ?_⟩
  · simp [classify, early]
  · omega

/-- A signer that does not set its start back, writing at `c` ahead of `t`,
is reported exactly `c - t` ahead: the allowance is never added. -/
theorem unset_start_is_reported_exactly (c lifetime t : Nat) (early : t < c) :
    classify (issueWithoutAllowance c lifetime) t = .notYetValid (c - t) := by
  -- Its start is `c` itself.
  simp [classify, issueWithoutAllowance, early]

/-- DefraDB's delegation from a signer more than the allowance ahead is
reported the true lead less the allowance: true, if not all of it. -/
theorem set_back_start_is_reported_less_the_allowance (c lifetime t : Nat)
    (early : t + allowance < c) :
    classify (issue c lifetime) t = .notYetValid (c - allowance - t) := by
  -- Its start is `c - 30`, still after `t`.
  have : t < c - allowance := by simp only [allowance] at *; omega
  simp [classify, issue, this]

/-- A delegation that has ended is reported expired, with no number, whatever
the clock that wrote it: the same report for a right clock whose delegation
was held past its lifetime as for a clock that was behind. -/
theorem expired_claims_nothing_about_the_clock (c lifetime t : Nat)
    (started : c - allowance ≤ t) (late : c + lifetime ≤ t) :
    classify (issue c lifetime) t = .expired := by
  -- It has started and it has ended.
  have h1 : ¬ t < c - allowance := by omega
  simp [classify, issue, h1, late]

/-- The refusal that reaches too far is a delegation open more than an open
request past `t`, among those vera.rs refuses. -/
theorem outlasting_reaches_too_far (claims : Claims) (t : Nat)
    (refused : ¬ requestHolds claims t) (shaped : wellFormed claims)
    (kind : classify claims t = .outlastsRequest) : t + maxTtl < claims.exp := by
  -- Neither early nor ended, so only the open-request bound can have failed.
  unfold classify at kind
  by_cases early : t < claims.nbf
  · simp [early] at kind
  · by_cases ended : claims.exp ≤ t
    · simp [early, ended] at kind
    · simp only [requestHolds, plainHolds, wellFormed, maxTtl] at refused shaped ⊢
      omega

/-- Whatever the refusal, writing the delegation again at the provider's time
it reports cures it: written there, it is honoured at every block time of its
lifetime. -/
theorem rewritten_at_reported_time_holds (p lifetime t : Nat) (positive : 0 < lifetime)
    (bounded : lifetime ≤ maxTtl) (started : p ≤ t) (within : t < p + lifetime) :
    requestHolds (issue p lifetime) t :=
  -- What was refused does not matter: the new one is written at `p`.
  written_at_provider_time_holds p lifetime t positive bounded started within

end VeraDelegationWindow
