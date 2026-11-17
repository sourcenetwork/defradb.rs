//! Core types used across the KMS service.

use cid::Cid;

/// CID of an `Encryption` metadata block. One CID == one DEK.
pub type EncryptionCid = Cid;

/// Scope of a DEK, derived from the on-disk `Encryption` block.
///
/// Drives which `AccessPolicy` gate applies. Never serialized on the wire —
/// re-derived on the responder from the block contents (mirrors Go's
/// `internal/kms/pubsub.go::getEncryptionKeysLocally`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyScope {
    /// Document-scoped DEK. `field: None` means a whole-document key shared
    /// by all encrypted fields; `field: Some(name)` means a per-field key.
    Document {
        doc_id: String,
        field: Option<String>,
    },
    /// Per-collection DEK (e.g. @branchable collection-head blocks). Gated
    /// by node-level NAC on release, not by per-doc DAC.
    Collection { collection_id: String },
}

impl KeyScope {
    /// A fresh DEK for this scope: random, except under the hidden
    /// Go-compatibility test switch, where a document key is derived from its
    /// DocRef and field as Go's `generateTestEncryptionKey` does.
    pub(crate) fn new_key(&self) -> [u8; 32] {
        match self {
            KeyScope::Document { doc_id, field } => match hex::decode(doc_id) {
                Ok(doc_ref) => {
                    defra_core::encryption::generate_encryption_key_for(&doc_ref, field.as_deref())
                }
                Err(_) => defra_core::encryption::generate_encryption_key(),
            },
            KeyScope::Collection { .. } => defra_core::encryption::generate_encryption_key(),
        }
    }
}

/// Outcome of an `AccessPolicy` decision. M1 ships only `Allow` / `Deny`.
/// `AllowAttested` lands with M4 Vera.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// AccessPolicy granted release.
    Allow,
    /// AccessPolicy refused release.
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_scope_equality() {
        let a = KeyScope::Document {
            doc_id: "d".into(),
            field: None,
        };
        let b = KeyScope::Document {
            doc_id: "d".into(),
            field: None,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn new_key_is_random_unless_the_test_switch_derives_gos_key() {
        let scope = KeyScope::Document {
            doc_id: hex::encode([0x89, 0x8a]),
            field: Some("age".into()),
        };
        assert_ne!(scope.new_key(), scope.new_key());

        defra_core::encryption::set_deterministic_encryption_key(true);
        // A release build without the test switch env var keeps keys random.
        if defra_core::encryption::deterministic_encryption_key_enabled() {
            assert_eq!(&scope.new_key(), b"age\x89\x8aexamplekey1234567890example");
        } else {
            assert_ne!(scope.new_key(), scope.new_key());
        }
        let collection = KeyScope::Collection {
            collection_id: "c".into(),
        };
        assert_ne!(collection.new_key(), collection.new_key());
        defra_core::encryption::set_deterministic_encryption_key(false);
    }

    #[test]
    fn policy_decision_equality() {
        assert_eq!(PolicyDecision::Allow, PolicyDecision::Allow);
        assert_ne!(PolicyDecision::Allow, PolicyDecision::Deny);
    }
}
