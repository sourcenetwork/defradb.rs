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

#[test]
fn corrupted_or_incomplete_state_never_rotates_the_identity() {
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"password").unwrap();
    let directory = root.path().join("worker");
    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    worker.prepare(Address::ZERO, Bytes::new()).unwrap();
    let original = fs::read(directory.join("state.json")).unwrap();
    let mut altered = worker.journal.clone();
    altered.next_sequence = 1;
    drop(worker);
    NativeWorker::persist(&directory.join("state.json"), &altered).unwrap();
    assert!(NativeWorker::open(&directory, &keyring, 9001).is_err());
    fs::write(directory.join("state.json"), &original).unwrap();
    assert!(NativeWorker::open(&directory, &keyring, 9002).is_err());
    keyring.delete(&altered.key_name).unwrap();
    assert!(NativeWorker::open(&directory, &keyring, 9001).is_err());
    assert!(keyring.list().unwrap().is_empty());
    assert_eq!(fs::read(directory.join("state.json")).unwrap(), original);
    fs::write(directory.join("state.json"), b"{").unwrap();
    assert!(NativeWorker::open(&directory, &keyring, 9001).is_err());
    assert!(keyring.list().unwrap().is_empty());
}

#[test]
fn failed_publication_does_not_allocate_a_sequence_or_return_signed_bytes() {
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"password").unwrap();
    let directory = root.path().join("worker");
    let mut worker = NativeWorker::open(&directory, &keyring, 9001).unwrap();
    let path = directory.join("state.json");
    fs::rename(&path, directory.join("saved.json")).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(worker.prepare(Address::ZERO, Bytes::new()).is_err());
    assert!(worker.pending().is_none());
    assert_eq!(worker.next_sequence(), 0);
    fs::remove_dir(&path).unwrap();
    fs::rename(directory.join("saved.json"), &path).unwrap();
    let wire = worker.prepare(Address::ZERO, Bytes::new()).unwrap();
    assert_eq!(NativeTx::decode_wire(wire).unwrap().nonce, 0);
}
