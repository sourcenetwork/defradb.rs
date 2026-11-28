use super::car_serve_admission::{ServeAdmission, ServeOffer};

type Lane = ServeAdmission<&'static str, u32, u64>;

#[test]
fn a_free_slot_admits_at_once_whatever_the_peer_already_holds() {
    let mut lane = Lane::new(2);
    assert_eq!(lane.offer("a", 1, 10), ServeOffer::Admitted(1));
    assert_eq!(lane.offer("a", 2, 10), ServeOffer::Admitted(2));
    assert_eq!(lane.active(), 2);
    assert_eq!(lane.waiting(), 0);
}

#[test]
fn a_full_lane_holds_the_request_until_a_slot_frees() {
    let mut lane = Lane::new(1);
    assert_eq!(lane.offer("a", 1, 10), ServeOffer::Admitted(1));
    assert_eq!(lane.offer("b", 2, 10), ServeOffer::Waiting);
    assert_eq!(lane.release(&"a"), Some(("b", 2)));
    assert_eq!(lane.active(), 1);
    assert_eq!(lane.waiting(), 0);
    assert_eq!(lane.release(&"b"), None);
    assert_eq!(lane.active(), 0);
}

#[test]
fn a_freed_slot_goes_to_the_peer_holding_the_fewest_serves() {
    let mut lane = Lane::new(4);
    for item in 0..4 {
        assert_eq!(lane.offer("hog", item, 10), ServeOffer::Admitted(item));
    }
    assert_eq!(lane.offer("hog", 10, 10), ServeOffer::Waiting);
    assert_eq!(lane.offer("spoke", 20, 10), ServeOffer::Waiting);
    assert_eq!(
        lane.release(&"hog"),
        Some(("spoke", 20)),
        "the peer holding no serve goes before the older request of the peer holding three"
    );
    assert_eq!(lane.release(&"hog"), Some(("hog", 10)));
}

#[test]
fn equal_holders_are_served_oldest_first() {
    let mut lane = Lane::new(4);
    for item in 0..4 {
        assert_eq!(lane.offer("x", item, 10), ServeOffer::Admitted(item));
    }
    assert_eq!(lane.offer("b", 1, 10), ServeOffer::Waiting);
    assert_eq!(lane.offer("c", 2, 10), ServeOffer::Waiting);
    assert_eq!(lane.release(&"x"), Some(("b", 1)));
}

#[test]
fn the_room_is_bounded_overall_and_per_peer() {
    let mut lane = Lane::new(4);
    for item in 0..4 {
        assert_eq!(lane.offer("x", item, 10), ServeOffer::Admitted(item));
    }
    assert_eq!(lane.offer("a", 10, 10), ServeOffer::Waiting);
    assert_eq!(
        lane.offer("a", 11, 10),
        ServeOffer::Shed(11),
        "one peer fills at most a quarter of the room"
    );
    assert_eq!(lane.offer("b", 12, 10), ServeOffer::Waiting);
    assert_eq!(lane.offer("c", 13, 10), ServeOffer::Waiting);
    assert_eq!(lane.offer("d", 14, 10), ServeOffer::Waiting);
    assert_eq!(lane.offer("e", 15, 10), ServeOffer::Shed(15));
    assert_eq!(lane.waiting(), 4);
}

#[test]
fn an_expired_wait_is_returned_to_be_answered_busy() {
    let mut lane = Lane::new(4);
    for item in 0..4 {
        assert_eq!(lane.offer("x", item, 0), ServeOffer::Admitted(item));
    }
    assert_eq!(lane.offer("a", 10, 5), ServeOffer::Waiting);
    assert_eq!(lane.offer("b", 11, 7), ServeOffer::Waiting);
    assert_eq!(lane.next_deadline(), Some(5));
    assert_eq!(lane.pop_expired(4), None);
    assert_eq!(lane.pop_expired(5), Some(10));
    assert_eq!(lane.pop_expired(5), None);
    assert_eq!(lane.next_deadline(), Some(7));
    assert_eq!(lane.pop_expired(9), Some(11));
    assert_eq!(lane.next_deadline(), None);
    assert_eq!(lane.active(), 4, "an expired wait never held a slot");
}

#[test]
fn a_zero_capacity_is_one_slot() {
    let mut lane = Lane::new(0);
    assert_eq!(lane.capacity(), 1);
    assert_eq!(lane.offer("a", 1, 10), ServeOffer::Admitted(1));
    assert_eq!(lane.offer("b", 2, 10), ServeOffer::Waiting);
}
