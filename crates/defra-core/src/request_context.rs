//! The state one request carries down its call chain.
//!
//! A boundary (HTTP, the embedded API, FFI, the CLI, a P2P handler) builds one
//! [`RequestContext`] per request and hands it down as a value: inside the
//! request message, bound into the handle that serves the request, or as an
//! argument. Nothing about a request lives in a thread-local, a task-local or a
//! process global, so a task built on one thread and run on another, or two
//! requests interleaved on one thread, each see exactly their own request.
//!
//! A context is immutable once built and cheap to clone: the fields sit behind
//! one `Arc`, and each `with_*` call copies them only when the context is
//! shared.

use std::borrow::Cow;
use std::fmt;
use std::future::Future;
use std::sync::Arc;

use crate::encryption::EncryptionConfig;
use crate::signing::SigningConfig;
use crate::txn_id::TxnId;

/// What one request carries: who acts, with which privileges and bearer
/// token, and how its writes are encrypted and signed.
///
/// The default context is an anonymous request with nothing granted.
#[derive(Clone, Default)]
pub struct RequestContext(Arc<Fields>);

#[derive(Clone, Default)]
struct Fields {
    identity: Option<String>,
    dac_bypass: bool,
    encryption: Option<EncryptionConfig>,
    signing: Option<SigningConfig>,
    batch_session: Option<String>,
    creator: Option<String>,
    /// The DID that presented a bearer token and the token, when the request
    /// carries a bearer at all.
    bearer: Option<(Option<String>, Option<String>)>,
    txn: Option<TxnId>,
}

impl RequestContext {
    /// The context a call that arrives without one runs in: the default
    /// anonymous context. Built for tests with the `ambient` feature, it
    /// carries what the test set through the thread-local API instead (see
    /// the `ambient` module).
    pub fn ambient() -> Self {
        #[cfg(any(test, feature = "ambient"))]
        return crate::ambient::context();
        #[cfg(not(any(test, feature = "ambient")))]
        Self::default()
    }

    /// Runs `future` with this context installed in the thread-local state
    /// tests set the old way, so a test double that reads that state sees the
    /// request it serves; outside tests this is `future` itself.
    pub fn mirror<F: Future>(&self, future: F) -> impl Future<Output = F::Output> {
        #[cfg(any(test, feature = "ambient"))]
        return crate::ambient::mirror(self.clone(), future);
        #[cfg(not(any(test, feature = "ambient")))]
        future
    }

    /// `bound`, the context a handle was bound to, or
    /// [`RequestContext::ambient`] for a handle bound to no request.
    pub fn bound_or_ambient(bound: Option<&Self>) -> Cow<'_, Self> {
        match bound {
            Some(context) => Cow::Borrowed(context),
            None => Cow::Owned(Self::ambient()),
        }
    }

    /// This context with every field it leaves unset taken from
    /// [`RequestContext::ambient`]; unchanged outside tests.
    #[must_use]
    pub fn or_ambient(self) -> Self {
        #[cfg(any(test, feature = "ambient"))]
        {
            let ambient = Self::ambient();
            let mut context = self;
            let fields = Arc::make_mut(&mut context.0);
            fields.identity = fields
                .identity
                .take()
                .or_else(|| ambient.0.identity.clone());
            fields.dac_bypass |= ambient.0.dac_bypass;
            fields.encryption = fields
                .encryption
                .take()
                .or_else(|| ambient.0.encryption.clone());
            fields.signing = fields.signing.take().or_else(|| ambient.0.signing.clone());
            fields.batch_session = fields
                .batch_session
                .take()
                .or_else(|| ambient.0.batch_session.clone());
            fields.creator = fields.creator.take().or_else(|| ambient.0.creator.clone());
            fields.bearer = fields.bearer.take().or_else(|| ambient.0.bearer.clone());
            context
        }
        #[cfg(not(any(test, feature = "ambient")))]
        self
    }

    /// The DID acting in this request: the identity node access control
    /// checks and an explicit transaction belongs to. `None` for an anonymous
    /// request.
    pub fn identity(&self) -> Option<&str> {
        self.0.identity.as_deref()
    }

    /// This context with the acting identity set to `identity`.
    #[must_use]
    pub fn with_identity(mut self, identity: Option<String>) -> Self {
        Arc::make_mut(&mut self.0).identity = identity;
        self
    }

    /// Whether node access control grants this request's identity the
    /// `bypass-dac` permission, so document access control lets an
    /// authenticated caller read and write every document. Resolved once, at
    /// the boundary, for the request's identity.
    pub fn dac_bypass(&self) -> bool {
        self.0.dac_bypass
    }

    /// This context with the DAC bypass set to `bypass`.
    #[must_use]
    pub fn with_dac_bypass(mut self, bypass: bool) -> Self {
        Arc::make_mut(&mut self.0).dac_bypass = bypass;
        self
    }

    /// Which fields of the documents a mutation writes it encrypts, when the
    /// mutation asks for encryption. Without one a write inherits each
    /// field's encryption from its previous block.
    pub fn encryption(&self) -> Option<&EncryptionConfig> {
        self.0.encryption.as_ref()
    }

    /// This context with the encryption set to `encryption`.
    #[must_use]
    pub fn with_encryption(mut self, encryption: Option<EncryptionConfig>) -> Self {
        Arc::make_mut(&mut self.0).encryption = encryption;
        self
    }

    /// This context with the encryption set to `encryption` when there is
    /// one, and its own kept otherwise.
    #[must_use]
    pub fn or_encryption(self, encryption: Option<EncryptionConfig>) -> Self {
        match encryption {
            Some(encryption) => self.with_encryption(Some(encryption)),
            None => self,
        }
    }

    /// The key this request signs the blocks it writes with; unsigned blocks
    /// without one.
    pub fn signing(&self) -> Option<&SigningConfig> {
        self.0.signing.as_ref()
    }

    /// This context with the signing set to `signing`.
    #[must_use]
    pub fn with_signing(mut self, signing: Option<SigningConfig>) -> Self {
        Arc::make_mut(&mut self.0).signing = signing;
        self
    }

    /// The batch signing session the CIDs of the blocks this request commits
    /// are collected into (see [`crate::batch_signing`]), if it runs in one.
    pub fn batch_session(&self) -> Option<&str> {
        self.0.batch_session.as_deref()
    }

    /// This context with the batch signing session set to `session`.
    #[must_use]
    pub fn with_batch_session(mut self, session: Option<String>) -> Self {
        Arc::make_mut(&mut self.0).batch_session = session;
        self
    }

    /// The DID that creates what this request writes: document ACP registers
    /// it as the owner of the documents it creates, and P2P broadcasts name it
    /// as their creator, so a receiving node registers the same owner. P2P
    /// names the node's peer ID instead when there is none.
    pub fn creator(&self) -> Option<&str> {
        self.0.creator.as_deref()
    }

    /// This context with the creator set to `creator`.
    #[must_use]
    pub fn with_creator(mut self, creator: Option<String>) -> Self {
        Arc::make_mut(&mut self.0).creator = creator;
        self
    }

    /// The bearer token `did` presents in this request, the credential a
    /// remote ACP provider acts on its behalf with: the request's own token
    /// when `did` presented it, and none for any other DID. A request that
    /// carries no bearer at all presents the token the process binds to `did`
    /// ([`set_request_bearer_token`](crate::signing::set_request_bearer_token)).
    pub fn bearer_token_for(&self, did: &str) -> Option<String> {
        match &self.0.bearer {
            Some((presenter, token)) => (presenter.as_deref() == Some(did))
                .then(|| token.clone())
                .flatten(),
            None => crate::signing::stored_bearer_token(did),
        }
    }

    /// The explicit transaction this request runs in; `None` for a request
    /// that commits on its own.
    pub fn txn(&self) -> Option<TxnId> {
        self.0.txn
    }

    /// This context running in the explicit transaction `txn`.
    #[must_use]
    pub fn with_txn(mut self, txn: Option<TxnId>) -> Self {
        Arc::make_mut(&mut self.0).txn = txn;
        self
    }

    /// The DID that presented this request's bearer token and the token,
    /// when it carries a bearer.
    #[cfg(any(test, feature = "ambient"))]
    pub(crate) fn bearer(&self) -> Option<&(Option<String>, Option<String>)> {
        self.0.bearer.as_ref()
    }

    /// This context carrying the bearer `token` `did` authenticated the
    /// request with, `None` when it presented none.
    #[must_use]
    pub fn with_bearer(mut self, did: Option<String>, token: Option<String>) -> Self {
        Arc::make_mut(&mut self.0).bearer = Some((did, token));
        self
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestContext")
            .field("identity", &self.0.identity)
            .field("dac_bypass", &self.0.dac_bypass)
            .field("encryption", &self.0.encryption)
            .field(
                "signing",
                &self.0.signing.as_ref().map(|signing| signing.key_type),
            )
            .field("batch_session", &self.0.batch_session)
            .field("creator", &self.0.creator)
            .field("txn", &self.0.txn)
            .field(
                "bearer_presenter",
                &self.0.bearer.as_ref().map(|(presenter, _)| presenter),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_context_grants_nothing() {
        let context = RequestContext::default();
        assert!(context.identity().is_none());
        assert!(!context.dac_bypass());
        assert!(context.encryption().is_none());
        assert!(context.signing().is_none());
        assert!(context.batch_session().is_none());
        assert!(context.creator().is_none());
    }

    #[test]
    fn debug_never_shows_the_signing_key() {
        let context = RequestContext::default().with_signing(Some(SigningConfig {
            key_type: crate::signing::SigningKeyType::Ed25519,
            private_key_bytes: vec![0xAB; 32],
            public_key_bytes: Vec::new(),
            public_key_hex: "public".to_string(),
            remote_signer: None,
            signing_authorization: None,
        }));
        let shown = format!("{context:?}");
        assert!(shown.contains("Ed25519"));
        assert!(!shown.contains("171"), "{shown}");
    }

    #[test]
    fn with_encryption_sets_and_clears_the_encryption() {
        let encrypted = RequestContext::default().with_encryption(Some(EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        }));
        assert!(encrypted
            .encryption()
            .is_some_and(|config| config.should_encrypt_field("secret")));
        assert!(encrypted.with_encryption(None).encryption().is_none());
    }

    #[test]
    fn or_encryption_keeps_the_context_encryption_when_given_none() {
        let sealed = EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: Vec::new(),
        };
        let context = RequestContext::default().with_encryption(Some(sealed));
        assert!(context
            .clone()
            .or_encryption(None)
            .encryption()
            .is_some_and(|config| config.encrypt_doc));
        let fields = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".to_string()],
        };
        assert!(context
            .or_encryption(Some(fields))
            .encryption()
            .is_some_and(|config| !config.encrypt_doc));
    }

    #[test]
    fn a_bound_handle_serves_its_own_request_and_an_unbound_one_the_ambient_one() {
        let alice = RequestContext::default().with_identity(Some("did:key:alice".to_string()));
        crate::current_identity::set_current_identity(Some("did:key:thread".to_string()));
        assert_eq!(
            RequestContext::bound_or_ambient(Some(&alice)).identity(),
            Some("did:key:alice")
        );
        assert_eq!(
            RequestContext::bound_or_ambient(None).identity(),
            Some("did:key:thread")
        );
        crate::current_identity::set_current_identity(None);
    }

    #[test]
    fn or_ambient_fills_only_what_the_context_leaves_unset() {
        crate::ambient::set_dac_bypass(true);
        crate::ambient::set_batch_session_key(Some("ambient-session".to_string()));
        crate::current_identity::set_current_identity(Some("did:key:ambient".to_string()));
        let merged = RequestContext::default()
            .with_identity(Some("did:key:explicit".to_string()))
            .or_ambient();
        crate::ambient::set_dac_bypass(false);
        crate::ambient::set_batch_session_key(None);
        crate::current_identity::set_current_identity(None);

        assert_eq!(merged.identity(), Some("did:key:explicit"));
        assert!(merged.dac_bypass());
        assert_eq!(merged.batch_session(), Some("ambient-session"));
    }

    #[test]
    fn mirror_installs_the_context_while_its_future_runs_and_restores_after() {
        workers::block_on(async {
            let context = RequestContext::default()
                .with_identity(Some("did:key:mirrored".to_string()))
                .with_dac_bypass(true)
                .with_creator(Some("did:key:creator".to_string()));
            let seen = context
                .mirror(async {
                    workers::yield_now().await;
                    (
                        crate::current_identity::get_effective_identity(),
                        crate::current_identity::get_current_identity(),
                        crate::ambient::get_dac_bypass(),
                        crate::ambient::get_broadcast_creator_did(),
                    )
                })
                .await;
            assert_eq!(
                seen,
                (
                    Some("did:key:mirrored".to_string()),
                    Some("did:key:mirrored".to_string()),
                    true,
                    Some("did:key:creator".to_string()),
                )
            );
            assert_eq!(crate::current_identity::get_current_identity(), None);
            assert!(!crate::ambient::get_dac_bypass());
            assert_eq!(crate::ambient::get_broadcast_creator_did(), None);
        })
        .unwrap()
    }

    #[test]
    fn a_request_presents_its_own_bearer_only_for_the_did_that_presented_it() {
        let did = "did:key:request-context-bearer";
        let other = "did:key:request-context-other";
        crate::signing::set_request_bearer_token(did, "stored");
        crate::signing::set_request_bearer_token(other, "stored-other");

        let request =
            RequestContext::default().with_bearer(Some(did.to_string()), Some("own".to_string()));
        assert_eq!(request.bearer_token_for(did).as_deref(), Some("own"));
        assert_eq!(request.bearer_token_for(other), None);

        let tokenless = RequestContext::default().with_bearer(Some(did.to_string()), None);
        assert_eq!(
            tokenless.bearer_token_for(did),
            None,
            "a request that presented no token fell back to the process store"
        );

        let unscoped = RequestContext::default();
        assert_eq!(unscoped.bearer_token_for(did).as_deref(), Some("stored"));

        crate::signing::clear_request_bearer_token(did);
        crate::signing::clear_request_bearer_token(other);
    }

    #[test]
    fn mirror_installs_the_bearer_for_the_old_api() {
        workers::block_on(async {
            let did = "did:key:mirrored-bearer";
            let context = RequestContext::default()
                .with_bearer(Some(did.to_string()), Some("mirrored".to_string()));
            let seen = context
                .mirror(async { crate::ambient::get_request_bearer_token(did) })
                .await;
            assert_eq!(seen.as_deref(), Some("mirrored"));
            assert_eq!(crate::ambient::get_request_bearer_token(did), None);
        })
        .unwrap()
    }

    #[test]
    fn with_dac_bypass_leaves_the_context_it_was_cloned_from_unchanged() {
        let anonymous = RequestContext::default();
        let admin = anonymous.clone().with_dac_bypass(true);
        assert!(admin.dac_bypass());
        assert!(!anonymous.dac_bypass());
    }
}
