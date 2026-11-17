//! Whether a delegation holds at the node's time, judged as vera.rs judges it
//! at block time.
//!
//! vera.rs acts under a delegation at block time `t` only when `nbf ≤ t ≤
//! exp`, and under one bound to a request only while its operation identity is
//! open: `t` before its expiry, and that expiry no more than
//! [`MAX_OPERATION_TTL`] past `t`. A delegation outside those bounds at the
//! node's time is refused with the bounds and the time, so its signer can
//! issue it again at the provider's time. Modeled by
//! proofs/lean/VeraDelegationWindow.lean.

use vera_crypto::jwt::JwtClaims;
use vera_crypto::operation::MAX_OPERATION_TTL;

/// The signer's clock when it issued `claims`. Every delegation DefraDB
/// writes starts the clock skew allowance before it: a plain one is issued at
/// the signer's clock, and one bound to a request is also issued at its start,
/// as vera.rs's own client writes an operation token.
fn signer_clock(claims: &JwtClaims) -> u64 {
    claims
        .nbf
        .saturating_add(acp::CLOCK_SKEW_ALLOWANCE.as_secs())
        .max(claims.iat)
}

/// `Ok` when `claims` hold at `now`, else the window they hold in.
pub(super) fn within_window(claims: &JwtClaims, now: u64) -> Result<(), acp::DelegationWindow> {
    let started = claims.nbf <= now && now <= claims.exp;
    let open = claims.request.as_ref().is_none_or(|request| {
        let expiry = request.id.expires_at();
        now < expiry && expiry - now <= MAX_OPERATION_TTL
    });
    if started && open {
        return Ok(());
    }
    Err(acp::DelegationWindow {
        issued_at: signer_clock(claims),
        not_before: claims.nbf,
        expires_at: claims.exp,
        provider_time: now,
    })
}
