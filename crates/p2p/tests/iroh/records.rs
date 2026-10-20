//! Service records: what a node accepts, what its book keeps and hands out,
//! and when it asks a node for a newer record, as `ServiceRecords.tla` and
//! `proofs/lean/ServiceRecord` describe.

use iroh::SecretKey;
use n0_future::time::{Duration, Instant};

use super::book::Offered;
use super::*;

const NOW: u64 = 1_000_000;

fn record(key: &SecretKey, seq: u64, expires_at: u64, services: Vec<Service>) -> SignedRecord {
    let record = ServiceRecord {
        node: *key.public().as_bytes(),
        seq,
        expires_at,
        services,
    };
    sign(key, &record)
}

fn collection(id: &str) -> Service {
    Service::Collection {
        key: CollectionKey::Plain(id.to_owned()),
        role: Role::Replica,
    }
}

#[test]
fn a_signed_record_verifies_as_signed() {
    let key = SecretKey::generate();
    let signed = record(&key, 1, NOW + 60, vec![Service::Kms, collection("users")]);

    let got = verify(&signed, NOW).unwrap();

    assert_eq!(got.seq, 1);
    assert_eq!(got.node, *key.public().as_bytes());
    assert_eq!(got.services, vec![Service::Kms, collection("users")]);
}

#[test]
fn a_changed_body_fails_its_signature() {
    let key = SecretKey::generate();
    let mut signed = record(&key, 1, NOW + 60, vec![Service::Kms]);
    let forged = record(&key, 1, NOW + 60, vec![Service::Kms, Service::Replicator]);
    signed.body = forged.body;

    assert_eq!(verify(&signed, NOW), Err(Rejection::BadSignature));
}

#[test]
fn a_record_signed_by_another_key_is_refused() {
    let owner = SecretKey::generate();
    let impostor = SecretKey::generate();
    let claimed = ServiceRecord {
        node: *owner.public().as_bytes(),
        seq: 1,
        expires_at: NOW + 60,
        services: vec![Service::Kms],
    };

    assert_eq!(
        verify(&sign(&impostor, &claimed), NOW),
        Err(Rejection::BadSignature)
    );
}

#[test]
fn every_limit_is_enforced() {
    let key = SecretKey::generate();
    let long_name = "x".repeat(MAX_NAME_BYTES + 1);

    assert_eq!(
        verify(&record(&key, 1, NOW, vec![]), NOW),
        Err(Rejection::Expired)
    );
    assert_eq!(
        verify(&record(&key, 1, NOW + MAX_TTL_SECS + 1, vec![]), NOW),
        Err(Rejection::TooLongLived)
    );
    assert_eq!(
        verify(
            &record(&key, 1, NOW + 60, vec![Service::Kms; MAX_SERVICES + 1]),
            NOW
        ),
        Err(Rejection::TooManyServices(MAX_SERVICES + 1))
    );
    assert_eq!(
        verify(
            &record(&key, 1, NOW + 60, vec![Service::Custom { name: long_name }]),
            NOW
        ),
        Err(Rejection::NameTooLong)
    );
    let oversized = SignedRecord {
        body: vec![0; MAX_RECORD_BYTES + 1],
        signature: vec![0; 64],
    };
    assert_eq!(
        verify(&oversized, NOW),
        Err(Rejection::TooLarge(MAX_RECORD_BYTES + 1))
    );
}

#[test]
fn a_record_at_every_limit_is_accepted() {
    let key = SecretKey::generate();
    let name = "x".repeat(MAX_NAME_BYTES);
    let mut services = vec![Service::Kms; MAX_SERVICES - 1];
    services.push(Service::Custom { name });

    assert!(verify(&record(&key, 1, NOW + MAX_TTL_SECS, services), NOW).is_ok());
}

#[test]
fn the_book_keeps_only_newer_records() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);

    assert_eq!(
        book.offer(record(&key, 2, NOW + 60, vec![]), NOW),
        Ok(Offered::Taken)
    );
    assert_eq!(
        book.offer(record(&key, 1, NOW + 60, vec![]), NOW),
        Ok(Offered::Stale)
    );
    assert_eq!(
        book.offer(record(&key, 2, NOW + 90, vec![]), NOW),
        Ok(Offered::Stale)
    );
    assert_eq!(book.live(&node, NOW).unwrap().seq, 2);

    assert_eq!(
        book.offer(record(&key, 3, NOW + 60, vec![]), NOW),
        Ok(Offered::Taken)
    );
    assert_eq!(book.live(&node, NOW).unwrap().seq, 3);
}

#[test]
fn an_invalid_record_leaves_the_book_unchanged() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);
    book.offer(record(&key, 1, NOW + 60, vec![]), NOW).unwrap();

    assert_eq!(
        book.offer(record(&key, 2, NOW, vec![]), NOW),
        Err(Rejection::Expired)
    );
    assert_eq!(book.live(&node, NOW).unwrap().seq, 1);
}

#[test]
fn an_expired_record_is_never_handed_out() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);
    book.offer(record(&key, 1, NOW + 60, vec![collection("users")]), NOW)
        .unwrap();

    assert!(book.live(&node, NOW + 59).is_some());
    assert!(book.live(&node, NOW + 60).is_none());
    assert!(book.signed(&node, NOW + 60).is_none());
    let users = CollectionKey::Plain("users".to_owned());
    assert_eq!(book.who(&users, NOW + 59, 8), vec![node]);
    assert!(book.who(&users, NOW + 60, 8).is_empty());
}

#[test]
fn who_follows_the_newest_record() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);
    let users = CollectionKey::Plain("users".to_owned());
    let posts = CollectionKey::Plain("posts".to_owned());
    book.offer(record(&key, 1, NOW + 60, vec![collection("users")]), NOW)
        .unwrap();

    book.offer(record(&key, 2, NOW + 60, vec![collection("posts")]), NOW)
        .unwrap();

    assert!(book.who(&users, NOW, 8).is_empty());
    assert_eq!(book.who(&posts, NOW, 8), vec![node]);
}

#[test]
fn a_full_book_drops_expired_records_first() {
    let short = SecretKey::generate();
    let long = SecretKey::generate();
    let newcomer = SecretKey::generate();
    let mut book = Book::new(2);
    book.offer(record(&long, 1, NOW + 600, vec![]), NOW)
        .unwrap();
    book.offer(record(&short, 1, NOW + 60, vec![]), NOW)
        .unwrap();

    book.offer(record(&newcomer, 1, NOW + 600, vec![]), NOW + 61)
        .unwrap();

    assert_eq!(book.len(), 2);
    assert!(book.live(&short.public(), NOW).is_none());
    assert!(book.live(&long.public(), NOW + 61).is_some());
}

#[test]
fn a_full_book_drops_the_record_that_expires_soonest() {
    let soon = SecretKey::generate();
    let late = SecretKey::generate();
    let newcomer = SecretKey::generate();
    let mut book = Book::new(2);
    book.offer(record(&late, 1, NOW + 600, vec![]), NOW)
        .unwrap();
    book.offer(record(&soon, 1, NOW + 300, vec![]), NOW)
        .unwrap();

    book.offer(record(&newcomer, 1, NOW + 600, vec![]), NOW)
        .unwrap();

    assert_eq!(book.len(), 2);
    assert!(book.live(&soon.public(), NOW).is_none());
    assert!(book.live(&newcomer.public(), NOW).is_some());
}

#[test]
fn a_pointer_reads_back_as_printed() {
    let pointer = Pointer {
        seq: 0x1234,
        hash: u64::MAX,
    };

    assert_eq!(pointer.print(), "dsr1:0000000000001234:ffffffffffffffff");
    assert_eq!(Pointer::parse(&pointer.print()), Some(pointer));
}

#[test]
fn a_malformed_pointer_is_refused() {
    for text in [
        "",
        "dsr1:",
        "dsr2:00000192985cf87b:ffffffffffffffff",
        "dsr1:00000192985CF87B:ffffffffffffffff",
        "dsr1:00000192985cf87b-ffffffffffffffff",
        "dsr1:00000192985cf87b:fffffffffffffff",
        "dsr1:+0000192985cf87b:ffffffffffffffff",
        "dsr1:00000192985cf87b:ffffffffffffffff0",
    ] {
        assert_eq!(Pointer::parse(text), None, "{text}");
    }
}

#[test]
fn the_book_is_behind_only_a_higher_seq() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);
    assert!(book.is_behind(&node, &Pointer { seq: 0, hash: 0 }));

    book.offer(record(&key, 5, NOW + 60, vec![]), NOW).unwrap();

    assert!(!book.is_behind(&node, &Pointer { seq: 5, hash: 1 }));
    assert!(book.is_behind(&node, &Pointer { seq: 6, hash: 0 }));
}

#[test]
fn a_blinded_key_matches_only_under_the_same_secret() {
    let secret = NetworkSecret::new([7; 32]);
    let other = NetworkSecret::new([8; 32]);

    assert_eq!(blind(&secret, "users"), blind(&secret, "users"));
    assert_ne!(blind(&secret, "users"), blind(&secret, "posts"));
    assert_ne!(blind(&secret, "users"), blind(&other, "users"));
    assert_ne!(
        blind(&secret, "users").encode(),
        CollectionKey::Plain("users".to_owned()).encode()
    );
}

#[test]
fn a_collection_is_advertised_only_with_a_secret_or_when_public() {
    let secret = NetworkSecret::new([7; 32]);

    assert_eq!(advertised_key(None, false, "users"), None);
    assert_eq!(
        advertised_key(None, true, "users"),
        Some(CollectionKey::Plain("users".to_owned()))
    );
    assert_eq!(
        advertised_key(Some(&secret), true, "users"),
        Some(CollectionKey::Plain("users".to_owned()))
    );
    assert_eq!(
        advertised_key(Some(&secret), false, "users"),
        Some(blind(&secret, "users"))
    );
}

#[test]
fn the_network_secret_reads_from_hex_and_never_prints() {
    let hex = "07".repeat(32);

    assert_eq!(
        NetworkSecret::from_hex(&hex),
        Some(NetworkSecret::new([7; 32]))
    );
    assert_eq!(NetworkSecret::from_hex(&hex[..62]), None);
    assert_eq!(NetworkSecret::from_hex(&"zz".repeat(32)), None);
    assert!(!format!("{:?}", NetworkSecret::new([7; 32])).contains("07"));
}

#[test]
fn a_node_is_asked_once_per_interval_and_only_when_behind() {
    let key = SecretKey::generate();
    let node = key.public();
    let mut book = Book::new(8);
    let mut refresh = Refresh::new();
    let start = Instant::now();
    book.offer(record(&key, 5, NOW + 60, vec![]), NOW).unwrap();
    let current = Pointer { seq: 5, hash: 0 };
    let newer = Pointer { seq: 6, hash: 0 };

    assert!(!refresh.check(&book, node, &current, start));
    assert!(refresh.check(&book, node, &newer, start));
    assert!(!refresh.check(&book, node, &newer, start + Duration::from_secs(29)));
    assert!(refresh.check(&book, node, &newer, start + Duration::from_secs(30)));
}
