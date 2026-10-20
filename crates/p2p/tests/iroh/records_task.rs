//! The records task: signs and announces our record, answers peers, asks a
//! peer whose pointer is ahead, and points collection joins at advertisers.

use std::cell::RefCell;
use std::rc::Rc;

use bytes::Bytes;
use iroh::{EndpointId, SecretKey};
use n0_future::time::{Duration, Instant};
use rapidhash::{HashSetExt, RapidHashSet};

use super::*;

const UNIX: u64 = 2_000_000_000;

#[derive(Debug, PartialEq)]
enum Call {
    Announce(SignedRecord),
    Pointer(Pointer),
    Send(EndpointId, RecordsMessage),
    Resolve(EndpointId),
    Hint(String, Vec<EndpointId>),
}

#[derive(Clone, Default)]
struct FakeNet(Rc<RefCell<Vec<Call>>>);

impl FakeNet {
    fn take(&self) -> Vec<Call> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

impl Net for FakeNet {
    fn announce(&self, record: Bytes) {
        let signed = postcard::from_bytes(&record).expect("announced record decodes");
        self.0.borrow_mut().push(Call::Announce(signed));
    }
    fn set_pointer(&self, pointer: Pointer) {
        self.0.borrow_mut().push(Call::Pointer(pointer));
    }
    fn send(&self, peer: EndpointId, message: RecordsMessage) {
        self.0.borrow_mut().push(Call::Send(peer, message));
    }
    fn resolve(&self, peer: EndpointId) {
        self.0.borrow_mut().push(Call::Resolve(peer));
    }
    fn join_hint(&self, topic: String, peers: Vec<EndpointId>) {
        self.0.borrow_mut().push(Call::Hint(topic, peers));
    }
}

fn profile(secret: Option<NetworkSecret>, public: &[&str]) -> ServiceProfile {
    let mut public_collections = RapidHashSet::new();
    for id in public {
        public_collections.insert((*id).to_owned());
    }
    ServiceProfile {
        secret,
        role: Role::Replica,
        public_collections,
        offers: vec![Service::Kms],
    }
}

fn announced(calls: &[Call]) -> Vec<ServiceRecord> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::Announce(signed) => Some(verify(signed, UNIX).expect("own record verifies")),
            _ => None,
        })
        .collect()
}

fn records_with(net: &FakeNet) -> (Records<FakeNet>, Instant) {
    (Records::new(SecretKey::generate(), net.clone()), Instant::now())
}

#[test]
fn nothing_is_announced_before_the_profile_is_set() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);

    records.on_input(Input::Collection { id: "users".into(), held: true }, now, UNIX);
    records.renew(now, UNIX);

    assert!(net.take().is_empty());
}

#[test]
fn a_profile_signs_points_and_announces_once() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);

    records.on_input(Input::Profile(profile(None, &[])), now, UNIX);
    records.renew(now, UNIX);
    let calls = net.take();
    records.renew(now + Duration::from_secs(60), UNIX + 60);

    let own = announced(&calls);
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].services, vec![Service::Kms]);
    let Some(Call::Pointer(pointer)) = calls.first() else {
        panic!("the pointer is set first: {calls:?}");
    };
    assert_eq!(pointer.seq, own[0].seq);
    assert!(net.take().is_empty(), "an unchanged record is not announced again");
}

#[test]
fn collections_are_advertised_blinded_public_or_not_at_all() {
    let secret = NetworkSecret::new([3; 32]);
    for (secret, expected) in [
        (
            Some(secret.clone()),
            vec![
                Service::Collection { key: blind(&secret, "posts"), role: Role::Replica },
                Service::Collection { key: CollectionKey::Plain("users".into()), role: Role::Replica },
            ],
        ),
        (
            None,
            vec![Service::Collection { key: CollectionKey::Plain("users".into()), role: Role::Replica }],
        ),
    ] {
        let net = FakeNet::default();
        let (mut records, now) = records_with(&net);
        records.on_input(Input::Profile(profile(secret, &["users"])), now, UNIX);
        records.on_input(Input::Collection { id: "posts".into(), held: true }, now, UNIX);
        records.on_input(Input::Collection { id: "users".into(), held: true }, now, UNIX);

        records.renew(now, UNIX);

        let own = announced(&net.take());
        assert_eq!(own[0].services[1..], expected[..]);
    }
}

#[test]
fn a_change_is_announced_with_a_higher_seq_after_the_interval() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(None, &["users"])), now, UNIX);
    records.renew(now, UNIX);
    let first = announced(&net.take()).remove(0);

    records.on_input(Input::Collection { id: "users".into(), held: true }, now, UNIX);
    records.renew(now + Duration::from_secs(1), UNIX + 1);
    assert!(net.take().is_empty(), "announcements are rate limited");
    records.renew(now + Duration::from_secs(30), UNIX + 30);

    let second = announced(&net.take()).remove(0);
    assert!(second.seq > first.seq);
    assert_eq!(second.services.len(), 2);
}

#[test]
fn a_record_close_to_expiry_is_signed_again() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(None, &[])), now, UNIX);
    records.renew(now, UNIX);
    let first = announced(&net.take()).remove(0);

    let later = first.expires_at - 29 * 60;
    records.renew(now + Duration::from_secs(later - UNIX), later);

    let renewed = net.take();
    assert_eq!(renewed.len(), 2, "pointer and announcement: {renewed:?}");
}

#[test]
fn a_full_record_leaves_collections_out_instead_of_breaking_a_limit() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(Some(NetworkSecret::new([1; 32])), &[])), now, UNIX);
    for n in 0..200 {
        records.on_input(Input::Collection { id: format!("c{n}"), held: true }, now, UNIX);
    }

    records.renew(now, UNIX);

    let calls = net.take();
    let Some(Call::Announce(signed)) = calls.iter().find(|call| matches!(call, Call::Announce(_))) else {
        panic!("announced: {calls:?}");
    };
    assert!(signed.body.len() <= MAX_RECORD_BYTES);
    assert!(verify(signed, UNIX).expect("verifies").services.len() <= MAX_SERVICES);
}

#[test]
fn peers_get_our_record_and_records_we_hold() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peer = SecretKey::generate().public();
    let other = SecretKey::generate();
    let other_record = sign(
        &other,
        &ServiceRecord {
            node: *other.public().as_bytes(),
            seq: 1,
            expires_at: UNIX + 60,
            services: vec![Service::Collection { key: CollectionKey::Plain("users".into()), role: Role::Archive }],
        },
    );
    records.on_input(Input::Profile(profile(None, &[])), now, UNIX);
    records.renew(now, UNIX);
    let own = net.take().into_iter().find_map(|call| match call {
        Call::Announce(signed) => Some(signed),
        _ => None,
    });
    let bytes = Bytes::from(postcard::to_allocvec(&other_record).unwrap());
    records.on_input(Input::Announced(bytes), now, UNIX);

    let me = *SecretKey::generate().public().as_bytes();
    for (ask, reply) in [
        (RecordsMessage::Ask { node: *other.public().as_bytes() }, Some(vec![other_record.clone()])),
        (RecordsMessage::Who { key: CollectionKey::Plain("users".into()) }, Some(vec![other_record.clone()])),
        (RecordsMessage::Ask { node: me }, None),
    ] {
        records.on_input(Input::Message { from: peer, message: ask }, now, UNIX);
        let expected: Vec<Call> = reply
            .into_iter()
            .map(|records| Call::Send(peer, RecordsMessage::Records(records)))
            .collect();
        assert_eq!(net.take(), expected);
    }

    let my_node = verify(own.as_ref().unwrap(), UNIX).unwrap().node;
    records.on_input(Input::Message { from: peer, message: RecordsMessage::Ask { node: my_node } }, now, UNIX);
    assert_eq!(net.take(), vec![Call::Send(peer, RecordsMessage::Records(vec![own.unwrap()]))]);
}

#[test]
fn a_peer_is_asked_when_its_pointer_is_ahead_and_only_once_per_interval() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peer = SecretKey::generate().public();
    let ask = Call::Send(peer, RecordsMessage::Ask { node: *peer.as_bytes() });

    records.on_input(Input::Connected(peer), now, UNIX);
    assert_eq!(net.take(), vec![Call::Resolve(peer)]);

    records.on_input(Input::Pointer { node: peer, pointer: Some(Pointer { seq: 4, hash: 9 }) }, now, UNIX);
    assert_eq!(net.take(), vec![ask]);
    records.on_input(Input::Pointer { node: peer, pointer: Some(Pointer { seq: 5, hash: 9 }) }, now, UNIX);
    assert!(net.take().is_empty());
}

#[test]
fn a_peer_without_a_pointer_is_asked_only_while_we_hold_nothing_for_it() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peer = SecretKey::generate();
    let node = peer.public();
    let ask = Call::Send(node, RecordsMessage::Ask { node: *node.as_bytes() });

    records.on_input(Input::Pointer { node, pointer: None }, now, UNIX);
    assert_eq!(net.take(), vec![ask]);

    let held = sign(&peer, &ServiceRecord { node: *node.as_bytes(), seq: 1, expires_at: UNIX + 60, services: vec![] });
    records.on_input(Input::Message { from: node, message: RecordsMessage::Records(vec![held]) }, now, UNIX);
    records.on_input(Input::Pointer { node, pointer: None }, now + Duration::from_secs(60), UNIX);
    assert!(net.take().is_empty());
}

#[test]
fn a_collection_join_is_pointed_at_its_advertisers() {
    let secret = NetworkSecret::new([5; 32]);
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let blinded = SecretKey::generate();
    let public = SecretKey::generate();
    records.on_input(Input::Profile(profile(Some(secret.clone()), &[])), now, UNIX);
    for (key, advertised) in [(&blinded, blind(&secret, "users")), (&public, CollectionKey::Plain("users".into()))] {
        let record = sign(
            key,
            &ServiceRecord {
                node: *key.public().as_bytes(),
                seq: 1,
                expires_at: UNIX + 60,
                services: vec![Service::Collection { key: advertised, role: Role::Replica }],
            },
        );
        records.on_input(Input::Message { from: key.public(), message: RecordsMessage::Records(vec![record]) }, now, UNIX);
    }

    records.on_input(Input::Collection { id: "users".into(), held: true }, now, UNIX);

    let hints: Vec<Call> = net.take().into_iter().filter(|call| matches!(call, Call::Hint(..))).collect();
    assert_eq!(hints, vec![Call::Hint("users".into(), vec![blinded.public(), public.public()])]);
    assert_eq!(records.book().len(), 2);
}
