//! The records task: signs and announces our record, answers peers, asks a
//! peer whose pointer is ahead, and points collection joins at advertisers.

use std::cell::RefCell;
use std::rc::Rc;

use bytes::Bytes;
use iroh::{EndpointId, SecretKey};
use nuclei::time::Instant;
use rapidhash::{HashSetExt, RapidHashSet};
use std::time::Duration;

use super::task::Records;
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
    (
        Records::new(SecretKey::generate(), net.clone()),
        Instant::now(),
    )
}

#[test]
fn nothing_is_announced_before_the_profile_is_set() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);

    records.on_input(
        Input::Collection {
            id: "users".into(),
            held: true,
        },
        now,
        UNIX,
    );
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
    assert!(
        net.take().is_empty(),
        "an unchanged record is not announced again"
    );
}

#[test]
fn collections_are_advertised_blinded_public_or_not_at_all() {
    let secret = NetworkSecret::new([3; 32]);
    for (secret, expected) in [
        (
            Some(secret.clone()),
            vec![
                Service::Collection {
                    key: blind(&secret, "posts"),
                    role: Role::Replica,
                },
                Service::Collection {
                    key: CollectionKey::Plain("users".into()),
                    role: Role::Replica,
                },
            ],
        ),
        (
            None,
            vec![Service::Collection {
                key: CollectionKey::Plain("users".into()),
                role: Role::Replica,
            }],
        ),
    ] {
        let net = FakeNet::default();
        let (mut records, now) = records_with(&net);
        records.on_input(Input::Profile(profile(secret, &["users"])), now, UNIX);
        records.on_input(
            Input::Collection {
                id: "posts".into(),
                held: true,
            },
            now,
            UNIX,
        );
        records.on_input(
            Input::Collection {
                id: "users".into(),
                held: true,
            },
            now,
            UNIX,
        );

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

    records.on_input(
        Input::Collection {
            id: "users".into(),
            held: true,
        },
        now,
        UNIX,
    );
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
    records.on_input(
        Input::Profile(profile(Some(NetworkSecret::new([1; 32])), &[])),
        now,
        UNIX,
    );
    for n in 0..200 {
        records.on_input(
            Input::Collection {
                id: format!("c{n}"),
                held: true,
            },
            now,
            UNIX,
        );
    }

    records.renew(now, UNIX);

    let calls = net.take();
    let Some(Call::Announce(signed)) = calls.iter().find(|call| matches!(call, Call::Announce(_)))
    else {
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
            services: vec![Service::Collection {
                key: CollectionKey::Plain("users".into()),
                role: Role::Archive,
            }],
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
        (
            RecordsMessage::Ask {
                node: *other.public().as_bytes(),
            },
            Some(vec![other_record.clone()]),
        ),
        (
            RecordsMessage::Who {
                key: CollectionKey::Plain("users".into()),
            },
            Some(vec![other_record.clone()]),
        ),
        (RecordsMessage::Ask { node: me }, None),
    ] {
        records.on_input(
            Input::Message {
                from: peer,
                message: ask,
            },
            now,
            UNIX,
        );
        let expected: Vec<Call> = reply
            .into_iter()
            .map(|records| Call::Send(peer, RecordsMessage::Records(records)))
            .collect();
        assert_eq!(net.take(), expected);
    }

    let my_node = verify(own.as_ref().unwrap(), UNIX).unwrap().node;
    records.on_input(
        Input::Message {
            from: peer,
            message: RecordsMessage::Ask { node: my_node },
        },
        now,
        UNIX,
    );
    assert_eq!(
        net.take(),
        vec![Call::Send(
            peer,
            RecordsMessage::Records(vec![own.unwrap()])
        )]
    );
}

#[test]
fn a_peer_is_asked_when_its_pointer_is_ahead_and_only_once_per_interval() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peer = SecretKey::generate().public();
    let ask = Call::Send(
        peer,
        RecordsMessage::Ask {
            node: *peer.as_bytes(),
        },
    );

    records.on_input(Input::Connected(peer), now, UNIX);
    assert_eq!(net.take(), vec![Call::Resolve(peer)]);

    records.on_input(
        Input::Pointer {
            node: peer,
            pointer: Some(Pointer { seq: 4, hash: 9 }),
        },
        now,
        UNIX,
    );
    assert_eq!(net.take(), vec![ask]);
    records.on_input(
        Input::Pointer {
            node: peer,
            pointer: Some(Pointer { seq: 5, hash: 9 }),
        },
        now,
        UNIX,
    );
    assert!(net.take().is_empty());
}

#[test]
fn connected_peers_are_looked_up_again_in_rotation_until_they_leave() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peers: Vec<EndpointId> = (0..6).map(|_| SecretKey::generate().public()).collect();
    for peer in &peers {
        records.on_input(Input::Connected(*peer), now, UNIX);
    }
    records.on_input(Input::Connected(peers[0]), now, UNIX);
    net.take();

    records.rotate(now);
    let first: Vec<Call> = peers[..4].iter().map(|peer| Call::Resolve(*peer)).collect();
    assert_eq!(net.take(), first);
    records.rotate(now + Duration::from_secs(9));
    assert!(net.take().is_empty());

    records.on_input(Input::Disconnected(peers[5]), now, UNIX);
    records.rotate(now + Duration::from_secs(10));
    let second: Vec<Call> = [peers[4], peers[0], peers[1], peers[2]]
        .iter()
        .map(|peer| Call::Resolve(*peer))
        .collect();
    assert_eq!(net.take(), second);
}

#[test]
fn a_peer_without_a_pointer_is_asked_only_while_we_hold_nothing_for_it() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let peer = SecretKey::generate();
    let node = peer.public();
    let ask = Call::Send(
        node,
        RecordsMessage::Ask {
            node: *node.as_bytes(),
        },
    );

    records.on_input(
        Input::Pointer {
            node,
            pointer: None,
        },
        now,
        UNIX,
    );
    assert_eq!(net.take(), vec![ask]);

    let held = sign(
        &peer,
        &ServiceRecord {
            node: *node.as_bytes(),
            seq: 1,
            expires_at: UNIX + 60,
            services: vec![],
        },
    );
    records.on_input(
        Input::Message {
            from: node,
            message: RecordsMessage::Records(vec![held]),
        },
        now,
        UNIX,
    );
    records.on_input(
        Input::Pointer {
            node,
            pointer: None,
        },
        now + Duration::from_secs(60),
        UNIX,
    );
    assert!(net.take().is_empty());
}

#[test]
fn a_collection_join_is_pointed_at_its_advertisers() {
    let secret = NetworkSecret::new([5; 32]);
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    let blinded = SecretKey::generate();
    let public = SecretKey::generate();
    records.on_input(
        Input::Profile(profile(Some(secret.clone()), &[])),
        now,
        UNIX,
    );
    for (key, advertised) in [
        (&blinded, blind(&secret, "users")),
        (&public, CollectionKey::Plain("users".into())),
    ] {
        let record = sign(
            key,
            &ServiceRecord {
                node: *key.public().as_bytes(),
                seq: 1,
                expires_at: UNIX + 60,
                services: vec![Service::Collection {
                    key: advertised,
                    role: Role::Replica,
                }],
            },
        );
        records.on_input(
            Input::Message {
                from: key.public(),
                message: RecordsMessage::Records(vec![record]),
            },
            now,
            UNIX,
        );
    }

    records.on_input(
        Input::Collection {
            id: "users".into(),
            held: true,
        },
        now,
        UNIX,
    );

    let hints: Vec<Call> = net
        .take()
        .into_iter()
        .filter(|call| matches!(call, Call::Hint(..)))
        .collect();
    assert_eq!(
        hints,
        vec![Call::Hint(
            "users".into(),
            vec![blinded.public(), public.public()]
        )]
    );
}

fn hold(records: &mut Records<FakeNet>, now: Instant, key: &SecretKey, services: Vec<Service>) {
    let record = sign(
        key,
        &ServiceRecord {
            node: *key.public().as_bytes(),
            seq: 1,
            expires_at: UNIX + 60,
            services,
        },
    );
    records.on_input(
        Input::Message {
            from: key.public(),
            message: RecordsMessage::Records(vec![record]),
        },
        now,
        UNIX,
    );
}

fn holds_users(role: Role) -> Service {
    Service::Collection {
        key: CollectionKey::Plain("users".into()),
        role,
    }
}

fn providers(
    records: &mut Records<FakeNet>,
    now: Instant,
    ask: Vec<EndpointId>,
) -> Vec<EndpointId> {
    let (reply, mut answer) = futures::channel::oneshot::channel();
    records.on_input(
        Input::Providers {
            id: "users".into(),
            ask,
            reply,
        },
        now,
        UNIX,
    );
    answer
        .try_recv()
        .ok()
        .flatten()
        .expect("answered at once")
        .expect("lookup succeeds")
}

#[test]
fn providers_come_replicas_first_then_archives_then_readers() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(None, &["users"])), now, UNIX);
    let (reader, archive, replica, other) = (
        SecretKey::generate(),
        SecretKey::generate(),
        SecretKey::generate(),
        SecretKey::generate(),
    );
    hold(&mut records, now, &reader, vec![holds_users(Role::Reader)]);
    hold(
        &mut records,
        now,
        &archive,
        vec![holds_users(Role::Archive)],
    );
    hold(
        &mut records,
        now,
        &replica,
        vec![holds_users(Role::Replica)],
    );
    hold(
        &mut records,
        now,
        &other,
        vec![Service::Collection {
            key: CollectionKey::Plain("posts".into()),
            role: Role::Replica,
        }],
    );
    net.take();

    assert_eq!(
        providers(&mut records, now, vec![other.public()]),
        vec![replica.public(), archive.public(), reader.public()]
    );
    assert!(net.take().is_empty(), "a book with providers asks nobody");
}

#[test]
fn an_empty_lookup_asks_connected_peers_who_holds_the_collection() {
    let secret = NetworkSecret::new([5; 32]);
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(
        Input::Profile(profile(Some(secret.clone()), &[])),
        now,
        UNIX,
    );
    net.take();
    let peer = SecretKey::generate().public();

    assert!(providers(&mut records, now, vec![peer]).is_empty());
    let asked = net.take();
    assert!(providers(&mut records, now + Duration::from_secs(1), vec![peer]).is_empty());
    assert!(
        net.take().is_empty(),
        "asked once per interval, not once per lookup"
    );
    assert!(providers(&mut records, now + Duration::from_secs(31), vec![peer]).is_empty());
    assert_eq!(
        net.take().len(),
        2,
        "asked again once the interval has passed"
    );
    assert_eq!(
        asked,
        vec![
            Call::Send(
                peer,
                RecordsMessage::Who {
                    key: blind(&secret, "users")
                }
            ),
            Call::Send(
                peer,
                RecordsMessage::Who {
                    key: CollectionKey::Plain("users".into())
                }
            ),
        ]
    );
}

#[test]
fn a_who_query_is_answered_with_our_own_record_when_we_hold_the_key() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(None, &["users"])), now, UNIX);
    records.on_input(
        Input::Collection {
            id: "users".into(),
            held: true,
        },
        now,
        UNIX,
    );
    records.renew(now, UNIX);
    let own = announced(&net.take())
        .pop()
        .expect("our record is announced");
    let asker = SecretKey::generate().public();

    records.on_input(
        Input::Message {
            from: asker,
            message: RecordsMessage::Who {
                key: CollectionKey::Plain("users".into()),
            },
        },
        now,
        UNIX,
    );

    let sent: Vec<ServiceRecord> = net
        .take()
        .into_iter()
        .flat_map(|call| match call {
            Call::Send(to, RecordsMessage::Records(records)) if to == asker => records,
            _ => Vec::new(),
        })
        .map(|signed| verify(&signed, UNIX).expect("verifies"))
        .collect();
    assert_eq!(sent, vec![own]);
}

#[test]
fn replicator_offers_list_only_nodes_that_offer_and_hold_the_collection() {
    let net = FakeNet::default();
    let (mut records, now) = records_with(&net);
    records.on_input(Input::Profile(profile(None, &["users"])), now, UNIX);
    let (offers, offers_elsewhere, holds_only) = (
        SecretKey::generate(),
        SecretKey::generate(),
        SecretKey::generate(),
    );
    hold(
        &mut records,
        now,
        &offers,
        vec![Service::Replicator, holds_users(Role::Replica)],
    );
    hold(
        &mut records,
        now,
        &offers_elsewhere,
        vec![Service::Replicator],
    );
    hold(
        &mut records,
        now,
        &holds_only,
        vec![holds_users(Role::Replica)],
    );

    let lookup = |records: &mut Records<FakeNet>, id: Option<&str>| {
        let (reply, mut answer) = futures::channel::oneshot::channel();
        records.on_input(
            Input::Replicators {
                id: id.map(str::to_owned),
                reply,
            },
            now,
            UNIX,
        );
        let mut nodes = answer
            .try_recv()
            .ok()
            .flatten()
            .expect("answered")
            .expect("succeeds");
        nodes.sort();
        nodes
    };

    assert_eq!(lookup(&mut records, Some("users")), vec![offers.public()]);
    let mut every = vec![offers.public(), offers_elsewhere.public()];
    every.sort();
    assert_eq!(lookup(&mut records, None), every);
}

#[nuclei::test]
async fn changes_queue_past_a_full_hint_lane_and_come_out_first() {
    let handle = RecordsHandle::new();
    let peer = SecretKey::generate().public();
    // No task reads the handle, so hints fill their lane and the rest drop.
    for _ in 0..2048 {
        handle.push(Input::Connected(peer));
    }
    // A change never waits for room, however many queue behind the hints.
    for n in 0..2048 {
        handle.push_change(Input::Collection {
            id: n.to_string(),
            held: true,
        });
    }

    for n in 0..2048 {
        match nuclei::time::timeout(Duration::from_secs(5), handle.next()).await {
            Ok(Some(Input::Collection { id, held: true })) => assert_eq!(id, n.to_string()),
            _ => panic!("change {n} was not read first"),
        }
    }
    let mut hints = 0;
    handle.close();
    while let Some(input) = handle.next().await {
        assert!(matches!(input, Input::Connected(p) if p == peer));
        hints += 1;
    }
    assert!(hints < 2048, "a full hint lane drops what does not fit");
}
