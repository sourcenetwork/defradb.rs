//! Whether a delegation holds at the node's time, judged as vera.rs judges it
//! at block time.
//!
//! vera.rs acts under a delegation at block time `t` only when `nbf ≤ t ≤
//! exp`, and under one bound to a request only while its operation identity is
//! open: `t` before its expiry, and that expiry no more than
//! [`MAX_OPERATION_TTL`] past `t`. A delegation outside those bounds at the
//! node's time is refused with the bounds, the time and why, so its signer can
//! issue it again at the provider's time. Modeled by
//! proofs/lean/VeraDelegationWindow.lean.

use acp::WindowRefusal;
use vera_crypto::jwt::JwtClaims;
use vera_crypto::operation::MAX_OPERATION_TTL;

/// `Ok` when `claims` hold at `now`, else the window they hold in and why
/// they do not.
///
/// A start after `now` proves the signer's clock ahead by at least the gap:
/// no signer writes a start later than its own clock. An end at or before
/// `now` proves nothing about the clock, since a delegation held too long
/// ends the same way, so none is claimed.
pub(super) fn within_window(claims: &JwtClaims, now: u64) -> Result<(), acp::DelegationWindow> {
    let request_expiry = claims
        .request
        .as_ref()
        .map(|request| request.id.expires_at());
    let refusal = if now < claims.nbf {
        WindowRefusal::NotYetValid {
            ahead_by_at_least: claims.nbf - now,
        }
    } else if now > claims.exp || request_expiry.is_some_and(|expiry| now >= expiry) {
        WindowRefusal::Expired
    } else if request_expiry.is_some_and(|expiry| expiry - now > MAX_OPERATION_TTL) {
        WindowRefusal::OutlastsRequest
    } else {
        return Ok(());
    };
    Err(acp::DelegationWindow {
        not_before: claims.nbf,
        expires_at: claims.exp,
        provider_time: now,
        refusal,
    })
}
