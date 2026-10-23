//! Thread-safety tests for identity operations.
//!
//! These tests verify that `RawIdentity` properly implements `Send + Sync`
//! and can be safely used across threads and in async contexts.

use crypto::{generate_ed25519, generate_secp256k1};
use identity::{FullIdentity, Identity, RawIdentity};
use std::sync::Arc;

// ===== Static Assertions for Send + Sync =====

/// Compile-time assertion that RawIdentity is Send
fn assert_send<T: Send>() {}

/// Compile-time assertion that RawIdentity is Sync
fn assert_sync<T: Sync>() {}

#[test]
fn test_raw_identity_is_send() {
    assert_send::<RawIdentity>();
}

#[test]
fn test_raw_identity_is_sync() {
    assert_sync::<RawIdentity>();
}

#[test]
fn test_arc_raw_identity_is_send() {
    assert_send::<Arc<RawIdentity>>();
}

#[test]
fn test_arc_raw_identity_is_sync() {
    assert_sync::<Arc<RawIdentity>>();
}

#[test]
fn test_boxed_identity_trait_is_send() {
    assert_send::<Box<dyn Identity>>();
}

#[test]
fn test_boxed_identity_trait_is_sync() {
    assert_sync::<Box<dyn Identity>>();
}

#[test]
fn test_boxed_full_identity_trait_is_send() {
    assert_send::<Box<dyn FullIdentity>>();
}

#[test]
fn test_boxed_full_identity_trait_is_sync() {
    assert_sync::<Box<dyn FullIdentity>>();
}

// ===== Runtime Thread-Safety Tests =====

#[test]
fn test_identity_usable_across_spawned_tasks() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());
        let identity_clone = identity.clone();

        // Sign in a spawned task
        let handle = workers::spawn(None, "test", move || async move {
            identity_clone.sign(b"message from spawned task").unwrap()
        })
        .unwrap();

        let signature = handle.await.unwrap();

        // Verify in the main task
        let verified = identity
            .pub_key()
            .verify(b"message from spawned task", &signature)
            .unwrap();
        assert!(
            verified,
            "Signature from spawned task should verify in main task"
        );
    })
    .unwrap()
}

#[test]
fn test_identity_concurrent_signing() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());

        let mut handles = vec![];

        // Spawn multiple tasks that sign concurrently
        for i in 0..10 {
            let identity_clone = identity.clone();
            let message = format!("message {}", i);
            handles.push(
                workers::spawn(None, "test", move || async move {
                    let sig = identity_clone.sign(message.as_bytes()).unwrap();
                    (message, sig)
                })
                .unwrap(),
            );
        }

        // Collect all signatures
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // Verify all signatures
        for (message, signature) in results {
            let verified = identity
                .pub_key()
                .verify(message.as_bytes(), &signature)
                .unwrap();
            assert!(verified, "Signature for '{}' should verify", message);
        }
    })
    .unwrap()
}

#[test]
fn test_identity_concurrent_did_generation() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());

        let mut handles = vec![];

        // Spawn multiple tasks that get DID concurrently
        for _ in 0..10 {
            let identity_clone = identity.clone();
            handles.push(
                workers::spawn(None, "test", move || async move {
                    identity_clone.did().unwrap()
                })
                .unwrap(),
            );
        }

        // Collect all DIDs
        let mut dids = vec![];
        for handle in handles {
            dids.push(handle.await.unwrap());
        }

        // All DIDs should be identical
        let first_did = &dids[0];
        for did in &dids {
            assert_eq!(
                did, first_did,
                "All concurrent DID calls should return the same value"
            );
        }
    })
    .unwrap()
}

#[test]
fn test_identity_shared_between_tasks_secp256k1() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_secp256k1().unwrap()).unwrap());
        let identity_clone = identity.clone();

        // Sign in a spawned task
        let handle = workers::spawn(None, "test", move || async move {
            identity_clone
                .sign(b"secp256k1 message from spawned task")
                .unwrap()
        })
        .unwrap();

        let signature = handle.await.unwrap();

        // Verify in the main task
        let verified = identity
            .pub_key()
            .verify(b"secp256k1 message from spawned task", &signature)
            .unwrap();
        assert!(
            verified,
            "secp256k1 signature from spawned task should verify"
        );
    })
    .unwrap()
}

#[test]
fn test_identity_move_between_tasks() {
    workers::block_on(async {
        let identity = RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap();

        // Move identity into a task
        let result = workers::spawn(None, "test", move || async move {
            let did = identity.did().unwrap();
            let signature = identity.sign(b"test").unwrap();
            (did, signature)
        })
        .unwrap()
        .await
        .unwrap();

        assert!(result.0.as_str().starts_with("did:key:"));
        assert_eq!(result.1.len(), 64); // Ed25519 signature is 64 bytes
    })
    .unwrap()
}

#[test]
fn test_identity_send_to_std_thread() {
    let identity = RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap();

    // Move identity into a std thread
    let handle = std::thread::spawn(move || {
        let did = identity.did().unwrap();
        let signature = identity.sign(b"thread message").unwrap();
        (did, signature)
    });

    let (did, signature) = handle.join().unwrap();
    assert!(did.as_str().starts_with("did:key:"));
    assert_eq!(signature.len(), 64);
}

#[test]
fn test_arc_identity_shared_between_std_threads() {
    let identity = Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());

    let mut handles = vec![];

    // Spawn multiple threads that use the identity
    for i in 0..5 {
        let identity_clone = identity.clone();
        handles.push(std::thread::spawn(move || {
            let message = format!("thread {} message", i);
            identity_clone.sign(message.as_bytes()).unwrap()
        }));
    }

    // All threads should complete successfully
    for handle in handles {
        let signature = handle.join().unwrap();
        assert!(!signature.is_empty());
    }
}

#[test]
fn test_identity_under_multi_threaded_runtime() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());

        let mut handles = vec![];

        // Spawn many tasks that will be distributed across worker threads
        for i in 0..100 {
            let identity_clone = identity.clone();
            handles.push(
                workers::spawn(None, "test", move || async move {
                    let message = format!("multi-threaded message {}", i);
                    let sig = identity_clone.sign(message.as_bytes()).unwrap();
                    (message, sig)
                })
                .unwrap(),
            );
        }

        // Verify all signatures
        for handle in handles {
            let (message, signature) = handle.await.unwrap();
            let verified = identity
                .pub_key()
                .verify(message.as_bytes(), &signature)
                .unwrap();
            assert!(
                verified,
                "Signature should verify under multi-threaded runtime"
            );
        }
    })
    .unwrap()
}

#[test]
fn test_identity_key_type_access_is_thread_safe() {
    workers::block_on(async {
        let identity =
            Arc::new(RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap());

        let mut handles = vec![];

        // Spawn tasks that access key_type concurrently
        for _ in 0..10 {
            let identity_clone = identity.clone();
            handles.push(
                workers::spawn(None, "test", move || async move {
                    (
                        identity_clone.key_type(),
                        identity_clone.identity_key_type(),
                    )
                })
                .unwrap(),
            );
        }

        for handle in handles {
            let (key_type, identity_key_type) = handle.await.unwrap();
            assert_eq!(key_type, crypto::KeyType::Ed25519);
            assert_eq!(identity_key_type, identity::IdentityKeyType::Ed25519);
        }
    })
    .unwrap()
}
