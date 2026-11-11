use std::sync::Arc;

use regolith::sync::{Mutex, MutexGuard};

use super::*;
use crate::signing::{
    resolve_signing_config, resolve_signing_config_with_flag, HostContext, RemoteSigner,
    SigningAuthorization, SigningConfig, SigningKeyType,
};

fn make_config(label: &str) -> SigningConfig {
    SigningConfig {
        key_type: SigningKeyType::Secp256k1,
        private_key_bytes: vec![],
        public_key_bytes: vec![],
        public_key_hex: label.to_string(),
        remote_signer: None,
        signing_authorization: None,
    }
}

/// `IDENTITY_STORE` is process-global, so the tests that clear and refill it
/// cannot run concurrently: one test's `store_identity` lands between
/// another's `clear_identity_store` and its assertion.
static IDENTITY_STORE_GUARD: Mutex<()> = Mutex::new(());

async fn lock_identity_store() -> MutexGuard<'static, ()> {
    IDENTITY_STORE_GUARD.lock().await
}

#[test]
fn resolve_signing_config_uses_request_identity_when_registered() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        store_identity("did:key:request", make_config("request"))
            .expect("the store takes the identity");
        store_identity("did:key:node", make_config("node")).expect("the store takes the identity");

        let disabled =
            resolve_signing_config_with_flag(Some("did:key:request"), Some("did:key:node"), false);
        assert!(disabled.is_none());

        let resolved = resolve_signing_config(Some("did:key:request"), Some("did:key:node"))
            .expect("request identity should resolve");
        assert_eq!(resolved.public_key_hex, "request");

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn resolve_signing_config_falls_back_to_node_identity_when_request_identity_missing() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        store_identity("did:key:node", make_config("node")).expect("the store takes the identity");

        let resolved = resolve_signing_config(Some("did:key:request"), Some("did:key:node"))
            .expect("node fallback should resolve");
        assert_eq!(resolved.public_key_hex, "node");

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn find_remote_signer_did_returns_registered_remote_signer() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        struct NoopRemoteSigner;

        impl RemoteSigner for NoopRemoteSigner {
            fn sign_sync(
                &self,
                _data: &[u8],
                _authorization: Option<&SigningAuthorization>,
            ) -> Result<Vec<u8>, String> {
                Ok(vec![])
            }
        }

        clear_identity_store();
        store_identity("did:key:local", make_config("local"))
            .expect("the store takes the identity");
        store_identity(
            "did:key:remote",
            SigningConfig {
                key_type: SigningKeyType::BlsAugV1,
                private_key_bytes: vec![],
                public_key_bytes: vec![],
                public_key_hex: "remote".to_string(),
                remote_signer: Some(Arc::new(NoopRemoteSigner)),
                signing_authorization: None,
            },
        )
        .expect("the store takes the identity");

        let resolved = find_remote_signer_did().expect("remote signer DID should resolve");
        assert_eq!(resolved, "did:key:remote");

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn the_last_hold_on_an_identity_removes_its_key() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:held";
        hold_identity(did, make_config("held")).expect("the store takes the identity");
        hold_identity(did, make_config("held")).expect("the store takes the identity");

        release_identity(did);
        assert!(get_identity(did).is_some(), "one hold remains");
        release_identity(did);
        assert!(get_identity(did).is_none(), "no hold remains");
        assert!(
            !retain_identity(did),
            "a released identity has no key to retain"
        );

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn a_retained_identity_outlives_the_hold_it_was_taken_from() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:retained";
        hold_identity(did, make_config("held")).expect("the store takes the identity");
        assert!(retain_identity(did));

        release_identity(did);
        assert!(get_identity(did).is_some());
        release_identity(did);
        assert!(get_identity(did).is_none());

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn dropping_an_identity_hold_gives_it_back() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:guarded";
        let hold = IdentityHold::new(did, make_config("guarded"));
        assert!(get_identity(did).is_some());

        drop(hold);
        assert!(get_identity(did).is_none());

        clear_identity_store();
    })
    .unwrap()
}

#[test]
fn a_pinned_identity_survives_its_holds() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:pinned";
        store_identity(did, make_config("pinned")).expect("the store takes the identity");
        assert!(retain_identity(did));
        release_identity(did);
        release_identity(did);
        assert!(get_identity(did).is_some());

        clear_identity_store();
    })
    .unwrap()
}

/// `proofs/tla/IdentityStore.tla`: a clear unpins and drops what nothing
/// holds, but keeps a held key and its count, so a hold taken after the
/// clear and one taken before it are both given back to the same slot.
#[test]
fn a_clear_keeps_held_keys_and_their_count() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let held = "did:key:held-through-clear";
        let pinned = "did:key:pinned-then-cleared";
        hold_identity(held, make_config("held")).expect("the store takes the identity");
        store_identity(pinned, make_config("pinned")).expect("the store takes the identity");

        clear_identity_store();
        assert!(get_identity(held).is_some(), "a held key survives a clear");
        assert!(get_identity(pinned).is_none(), "a clear unpins");

        hold_identity(held, make_config("held")).expect("the store takes the identity");
        release_identity(held);
        assert!(
            get_identity(held).is_some(),
            "the hold taken before the clear still counts"
        );
        release_identity(held);
        assert!(get_identity(held).is_none(), "no hold remains");

        clear_identity_store();
    })
    .unwrap()
}

/// A host signer calling the host context its number names: two of them
/// with one number are two registrations of one context.
struct HostSigner(u8);

impl RemoteSigner for HostSigner {
    fn sign_sync(
        &self,
        _data: &[u8],
        _authorization: Option<&SigningAuthorization>,
    ) -> Result<Vec<u8>, String> {
        Ok(vec![])
    }

    fn host_context(&self) -> Option<HostContext> {
        Some(HostContext {
            handle: usize::from(self.0),
            callback: 0,
        })
    }
}

fn remote_config(signer: &Arc<HostSigner>) -> SigningConfig {
    let mut config = make_config("remote");
    config.remote_signer = Some(signer.clone());
    config
}

fn signs_through(did: &str, signer: &Arc<HostSigner>) -> bool {
    get_identity(did).is_some_and(|config| {
        config
            .remote_signer
            .as_ref()
            .is_some_and(|stored| std::ptr::addr_eq(Arc::as_ptr(stored), Arc::as_ptr(signer)))
    })
}

/// `proofs/tla/IdentityStore.tla` INV_OwnSigner: while a DID is held, a
/// hold that brings another signer is refused and takes nothing, so the
/// first holder keeps signing through its own signer, also after the
/// refused registration's owner gives its context up. Once the DID is
/// free, another signer may take it.
#[test]
fn a_held_did_refuses_another_signer() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:registered-twice";
        let first = Arc::new(HostSigner(1));
        let second = Arc::new(HostSigner(2));

        hold_identity(did, remote_config(&first)).expect("the first signer takes the DID");
        let refused = hold_identity(did, remote_config(&second))
            .expect_err("a second signer for a held DID is refused");
        assert_eq!(refused.did, did);
        assert!(signs_through(did, &first), "the holder keeps its signer");

        release_identity(did);
        assert!(get_identity(did).is_none(), "the refused hold took nothing");

        hold_identity(did, remote_config(&second)).expect("a free DID takes any signer");
        assert!(signs_through(did, &second));
        release_identity(did);

        clear_identity_store();
    })
    .unwrap()
}

/// `proofs/tla/IdentityStore.tla` INV_AcceptSame: a second registration of
/// one host context is a separate signer object but the same signer, so a
/// held DID takes it; the DID keeps its key until both holds are given back.
#[test]
fn a_held_did_takes_a_signer_with_the_same_host_context() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:one-context-twice";
        let first = Arc::new(HostSigner(1));
        let again = Arc::new(HostSigner(1));

        hold_identity(did, remote_config(&first)).expect("the first signer takes the DID");
        hold_identity(did, remote_config(&again)).expect("the same context joins the hold");
        assert!(signs_through(did, &first), "the stored signer stays");

        release_identity(did);
        assert!(get_identity(did).is_some(), "one hold remains");
        release_identity(did);
        assert!(get_identity(did).is_none(), "no hold remains");

        clear_identity_store();
    })
    .unwrap()
}

/// INV_OwnSigner for pins: a pin may replace a signer only pinned, never
/// one something holds.
#[test]
fn a_pin_replaces_only_an_unheld_signer() {
    workers::block_on(async {
        let _guard = lock_identity_store().await;
        clear_identity_store();
        let did = "did:key:pinned-and-held";
        let first = Arc::new(HostSigner(1));
        let second = Arc::new(HostSigner(2));

        store_identity(did, remote_config(&first)).expect("a free DID takes a pin");
        store_identity(did, remote_config(&second)).expect("a pin replaces a pin");
        assert!(signs_through(did, &second));

        assert!(retain_identity(did));
        store_identity(did, remote_config(&first)).expect_err("a pin may not re-route a holder");
        assert!(signs_through(did, &second), "the holder keeps its signer");
        store_identity(did, remote_config(&second)).expect("the same signer may pin");
        release_identity(did);

        clear_identity_store();
    })
    .unwrap()
}
