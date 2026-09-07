//! Tests for file-based keyring

use keyring::{Error, FileKeyring, Keyring};

#[test]
fn test_file_keyring_set_get() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    let key_data = b"my-secret-key-data";
    keyring.set("test-key", key_data).unwrap();

    let retrieved = keyring.get("test-key").unwrap();
    assert_eq!(&retrieved[..], key_data);
}

#[test]
fn test_file_keyring_get_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    let result = keyring.get("nonexistent");
    assert!(matches!(result, Err(Error::NotFound(_))));
}

#[test]
fn test_file_keyring_delete() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    keyring.set("to-delete", b"some-data").unwrap();
    assert!(keyring.get("to-delete").is_ok());

    keyring.delete("to-delete").unwrap();
    assert!(matches!(keyring.get("to-delete"), Err(Error::NotFound(_))));
}

#[test]
fn test_file_keyring_delete_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    let result = keyring.delete("nonexistent");
    assert!(matches!(result, Err(Error::NotFound(_))));
}

#[test]
fn test_file_keyring_list() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    keyring.set("key1", b"data1").unwrap();
    keyring.set("key2", b"data2").unwrap();
    keyring.set("key3", b"data3").unwrap();

    let mut keys = keyring.list().unwrap();
    keys.sort();
    assert_eq!(keys, vec!["key1", "key2", "key3"]);
}

#[test]
fn test_file_keyring_list_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    let keys = keyring.list().unwrap();
    assert!(keys.is_empty());
}

#[test]
fn test_file_keyring_overwrite() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"test-password").unwrap();

    keyring.set("key", b"original").unwrap();
    keyring.set("key", b"updated").unwrap();

    let retrieved = keyring.get("key").unwrap();
    assert_eq!(&retrieved[..], b"updated");
}

#[cfg(unix)]
#[test]
fn replacement_preserves_open_readers_and_restricts_permissions() {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(dir.path(), b"password").unwrap();
    keyring.set("worker", b"original").unwrap();
    let path = dir.path().join("worker");
    let original = std::fs::read(&path).unwrap();
    let mut reader = std::fs::File::open(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    keyring.set("worker", b"replacement").unwrap();

    let mut observed = Vec::new();
    reader.read_to_end(&mut observed).unwrap();
    assert_eq!(observed, original);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let reopened = FileKeyring::open(dir.path(), b"password").unwrap();
    assert_eq!(&*reopened.get("worker").unwrap(), b"replacement");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn failed_replacement_preserves_destination_and_cleans_staging() {
    let dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(dir.path(), b"password").unwrap();
    let destination = dir.path().join("worker");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("sentinel"), b"keep").unwrap();

    assert!(matches!(
        keyring.set("worker", b"replacement"),
        Err(Error::Io(_))
    ));
    assert_eq!(
        std::fs::read(destination.join("sentinel")).unwrap(),
        b"keep"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(keyring.list().unwrap().is_empty());
}

#[test]
fn test_file_keyring_wrong_password() {
    let temp_dir = tempfile::tempdir().unwrap();

    let keyring1 = FileKeyring::open(temp_dir.path(), b"password1").unwrap();
    keyring1.set("key", b"secret").unwrap();

    let keyring2 = FileKeyring::open(temp_dir.path(), b"password2").unwrap();
    let result = keyring2.get("key");
    assert!(matches!(result, Err(Error::Decryption(_))));
}

#[test]
fn test_jwe_format_go_compatible() {
    // Verify JWE compact serialization format (5 base64url parts separated by dots)
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(temp_dir.path(), b"secret").unwrap();

    keyring.set("peer_key", b"abc").unwrap();

    // Read raw file content
    let cipher = std::fs::read(temp_dir.path().join("peer_key")).unwrap();
    let token = std::str::from_utf8(&cipher).unwrap();

    // JWE compact serialization has 5 parts: header.encrypted_key.iv.ciphertext.tag
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 5, "JWE compact should have 5 parts");

    // Verify header contains expected algorithm
    use base64::Engine;
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .unwrap();
    let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();

    assert_eq!(header["alg"], "PBES2-HS512+A256KW");
    assert_eq!(header["enc"], "A256GCM");
    assert_eq!(header["p2c"], 10000);

    // Verify salt is 32 bytes (encoded as base64url)
    let p2s = header["p2s"].as_str().unwrap();
    let salt_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(p2s)
        .unwrap();
    assert_eq!(salt_bytes.len(), 32, "salt should be 32 bytes");
}
