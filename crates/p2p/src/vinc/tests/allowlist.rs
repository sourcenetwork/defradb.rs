//! Admission: the inbound allowlist, `allow_peer`, `deny_peer`, and lifting a
//! revocation.

use std::time::Duration;

use rapidhash::RapidHashSet;

use super::{connect, run, TestNode, IO_TIMEOUT};
use crate::transport::{P2PTransport, PeerId};
use crate::vinc::{AdmissionAuthority, VincAllowlistConfig, VincTransport};

fn explicit(peers: &[&PeerId]) -> VincAllowlistConfig {
    VincAllowlistConfig::Explicit(
        peers
            .iter()
            .map(|peer| peer.to_string())
            .collect::<RapidHashSet<_>>(),
    )
}

/// `dialer` cannot reach `server`: the dial fails, or the session is closed
/// before either side sees it.
async fn assert_never_connects(dialer: &VincTransport, server: &VincTransport) {
    let addrs = server.listen_addresses().await.unwrap();
    let _ = dialer.dial(server.local_peer_id(), addrs).await;
    crate::rt::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !server
            .connected_peers()
            .await
            .unwrap()
            .contains(dialer.local_peer_id()),
        "the server took a connection it should have refused"
    );
}

async fn wait_until_gone(node: &VincTransport, peer: &PeerId) {
    crate::rt::time::timeout(IO_TIMEOUT, async {
        while node.connected_peers().await.unwrap().contains(peer) {
            crate::rt::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the connection ends");
}

#[test]
fn refuses_a_peer_not_on_the_allowlist() {
    run(async {
        let server = TestNode::with_allowlist(explicit(&[])).await;
        let dialer = TestNode::spawn().await;
        assert_never_connects(&dialer.transport, &server.transport).await;
        dialer.shutdown().await;
        server.shutdown().await;
    });
}

#[test]
fn accepts_a_peer_on_the_allowlist() {
    run(async {
        let dialer = TestNode::spawn().await;
        let server = TestNode::with_allowlist(explicit(&[&dialer.id])).await;
        connect(&dialer.transport, &server.transport).await;
        dialer.shutdown().await;
        server.shutdown().await;
    });
}

#[test]
fn allow_peer_authorizes_a_peer_added_at_runtime() {
    run(async {
        let server = TestNode::with_allowlist(explicit(&[])).await;
        let dialer = TestNode::spawn().await;
        server
            .transport
            .allow_peer(&dialer.id, AdmissionAuthority::admit_only())
            .await
            .unwrap();
        connect(&dialer.transport, &server.transport).await;
        dialer.shutdown().await;
        server.shutdown().await;
    });
}

/// vinc caches the access hook's deny for 30 s, so a peer refused once is
/// refused again on its next dial even after `allow_peer`. Ignored until
/// vinc can drop a cached answer (see the facade gaps).
#[test]
fn allow_peer_admits_a_peer_the_allowlist_refused_before() {
    run(async {
        let server = TestNode::with_allowlist(explicit(&[])).await;
        let dialer = TestNode::spawn().await;
        assert_never_connects(&dialer.transport, &server.transport).await;
        server
            .transport
            .allow_peer(&dialer.id, AdmissionAuthority::admit_only())
            .await
            .unwrap();
        connect(&dialer.transport, &server.transport).await;
        dialer.shutdown().await;
        server.shutdown().await;
    });
}

#[test]
fn deny_peer_closes_an_already_open_connection() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        b.transport.deny_peer(&a.id).await.unwrap();
        assert!(
            !b.transport.connected_peers().await.unwrap().contains(&a.id),
            "the deny ends the session within the call"
        );
        wait_until_gone(&a.transport, &b.id).await;
        assert!(b.transport.is_peer_revoked(&a.id).await.unwrap());
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn deny_peer_under_accept_all_bars_only_that_peer() {
    run(async {
        let server = TestNode::spawn().await;
        let barred = TestNode::spawn().await;
        let bystander = TestNode::spawn().await;
        server.transport.deny_peer(&barred.id).await.unwrap();
        assert_never_connects(&barred.transport, &server.transport).await;
        connect(&bystander.transport, &server.transport).await;
        barred.shutdown().await;
        bystander.shutdown().await;
        server.shutdown().await;
    });
}

#[test]
fn deny_peer_refuses_our_own_outbound_dial_to_the_revoked_peer() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        a.transport.deny_peer(&b.id).await.unwrap();
        let dialed = a
            .transport
            .dial(&b.id, b.transport.listen_addresses().await.unwrap())
            .await;
        assert!(dialed.is_err(), "a revoked peer is not dialed");
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn a_caller_without_revoke_authority_cannot_undo_a_revocation() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        a.transport.deny_peer(&b.id).await.unwrap();
        let undone = a
            .transport
            .allow_peer(&b.id, AdmissionAuthority::admit_only())
            .await;
        assert!(undone.is_err());
        assert!(a.transport.is_peer_revoked(&b.id).await.unwrap());
        a.shutdown().await;
        b.shutdown().await;
    });
}

/// After a deny, `allow_peer` with full authority lifts the bar, and the
/// peer may dial in again. vinc keeps the deny it cached when the session
/// was closed for 30 s, so the reconnect is measured: an immediate dial, and
/// one after the cache lapses.
#[test]
fn deny_peer_then_allow_peer_again_permits_reconnection() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        b.transport.deny_peer(&a.id).await.unwrap();
        wait_until_gone(&a.transport, &b.id).await;
        b.transport
            .allow_peer(&a.id, AdmissionAuthority::full())
            .await
            .unwrap();
        assert!(!b.transport.is_peer_revoked(&a.id).await.unwrap());
        // vinc's 30 s cached answer: allow up to 35 s for the first dial that
        // takes.
        let started = std::time::Instant::now();
        let reconnected = crate::rt::time::timeout(Duration::from_secs(35), async {
            loop {
                let addrs = b.transport.listen_addresses().await.unwrap();
                if a.transport.dial(&b.id, addrs).await.is_ok()
                    && b.transport
                        .poll_until_connected(&a.id, Duration::from_millis(500))
                        .await
                        .is_ok()
                {
                    return;
                }
                crate::rt::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await;
        eprintln!(
            "vinc reconnect after deny_peer then allow_peer took {:?}",
            started.elapsed()
        );
        assert!(reconnected.is_ok(), "the peer never got back in");
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn poll_until_connected_wakes_on_the_connection_and_times_out_without_one() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        assert!(a
            .transport
            .poll_until_connected(&b.id, Duration::from_millis(200))
            .await
            .is_err());
        connect(&a.transport, &b.transport).await;
        a.transport
            .poll_until_connected(&b.id, IO_TIMEOUT)
            .await
            .unwrap();
        a.shutdown().await;
        b.shutdown().await;
    });
}

/// A peer that dials while it is revoked is refused, and vinc caches that
/// answer for 30 s. After `allow_peer` lifts the bar, the peer's next dial
/// within those 30 s should get in; vinc refuses it on the cached deny.
/// Ignored until vinc can drop a cached answer (see the facade gaps).
#[test]
fn allow_peer_admits_a_peer_that_dialed_while_revoked() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        b.transport.deny_peer(&a.id).await.unwrap();
        assert_never_connects(&a.transport, &b.transport).await;
        b.transport
            .allow_peer(&a.id, AdmissionAuthority::full())
            .await
            .unwrap();
        connect(&a.transport, &b.transport).await;
        a.shutdown().await;
        b.shutdown().await;
    });
}
