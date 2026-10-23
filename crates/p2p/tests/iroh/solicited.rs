//! Admitting a reply stream only while a reply is expected from that peer on
//! that tag (`SolicitedReplies.tla`).

use iroh::SecretKey;

use super::*;

const SE: &[u8] = b"/se/resp";
const MANAGE: &[u8] = b"/manage/resp";
const WINDOW: u64 = REPLY_WINDOW.as_millis() as u64;

fn peer() -> EndpointId {
    SecretKey::generate().public()
}

#[test]
fn a_reply_nobody_asked_for_is_refused() {
    let solicited = Solicited::new();

    assert!(!solicited.take_at(peer(), SE, 0));
}

#[test]
fn each_request_admits_one_reply() {
    let solicited = Solicited::new();
    let peer = peer();
    solicited.expect_at(peer, SE, 0);
    solicited.expect_at(peer, SE, 0);

    assert!(solicited.take_at(peer, SE, 1));
    assert!(solicited.take_at(peer, SE, 1));
    assert!(
        !solicited.take_at(peer, SE, 1),
        "a third reply was never asked for"
    );
}

#[test]
fn an_expectation_is_for_one_peer_and_one_tag() {
    let solicited = Solicited::new();
    let asked = peer();
    solicited.expect_at(asked, SE, 0);

    assert!(!solicited.take_at(peer(), SE, 1));
    assert!(!solicited.take_at(asked, MANAGE, 1));
    assert!(solicited.take_at(asked, SE, 1));
}

#[test]
fn a_closed_window_refuses_and_its_leftovers_do_not_carry_over() {
    let solicited = Solicited::new();
    let peer = peer();
    solicited.expect_at(peer, SE, 0);
    solicited.expect_at(peer, SE, 0);

    assert!(
        !solicited.take_at(peer, SE, WINDOW),
        "the window has closed"
    );

    solicited.expect_at(peer, SE, WINDOW);
    assert!(solicited.take_at(peer, SE, WINDOW + 1));
    assert!(
        !solicited.take_at(peer, SE, WINDOW + 1),
        "the two expired expectations must not admit replies in the new window"
    );
}

#[test]
fn a_new_request_moves_the_window_on() {
    let solicited = Solicited::new();
    let peer = peer();
    solicited.expect_at(peer, SE, 0);
    solicited.expect_at(peer, SE, WINDOW - 1);

    assert!(solicited.take_at(peer, SE, WINDOW + 1));
    assert!(solicited.take_at(peer, SE, WINDOW + 1));
}

#[test]
fn pruning_drops_only_closed_windows() {
    let solicited = Solicited::new();
    let old = peer();
    let live = peer();
    solicited.expect_at(old, SE, 0);
    solicited.expect_at(live, SE, WINDOW);

    solicited.prune(WINDOW + 1);

    assert_eq!(solicited.entries.len(), 1);
    assert!(solicited.take_at(live, SE, WINDOW + 1));
}

#[test]
fn a_full_table_is_pruned_on_the_next_request() {
    let solicited = Solicited::new();
    for _ in 0..PRUNE_AT {
        solicited.expect_at(peer(), SE, 0);
    }
    let next = peer();

    solicited.expect_at(next, SE, WINDOW);

    assert_eq!(solicited.entries.len(), 1);
    assert!(solicited.take_at(next, SE, WINDOW + 1));
}
