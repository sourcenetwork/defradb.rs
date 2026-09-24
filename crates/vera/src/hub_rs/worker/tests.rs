use keyring::{FileKeyring, Keyring};

use super::*;

#[test]
fn pending_request_and_identity_survive_reopen() {
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"password").unwrap();
    let directory = root.path().join("worker");
    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    let did = worker.did().to_owned();
    assert!(NativeWorker::open(&directory, &keyring, 9001).is_err());
    let payload = Bytes::from_static(b"request");
    let wire = worker
        .prepare(Address::ZERO, payload.clone())
        .unwrap()
        .to_vec();
    assert_eq!(
        worker.prepare(Address::ZERO, payload.clone()).unwrap(),
        wire
    );
    assert!(worker
        .prepare(Address::ZERO, Bytes::from_static(b"another"))
        .is_err());
    assert_eq!(worker.next_sequence(), 0);
    drop(worker);

    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    assert_eq!(worker.did(), did);
    assert_eq!(worker.pending().unwrap(), wire);
    assert_eq!(worker.prepare(Address::ZERO, payload).unwrap(), wire);
    assert_eq!(keyring.list().unwrap().len(), 1);
    let other = NativeWorker::open(&root.path().join("other"), &keyring, 9001).unwrap();
    assert_ne!(other.did(), worker.did());
    assert_eq!(keyring.list().unwrap().len(), 2);
}
