//! What a verified write brings so the node may act for its signer.

/// What lets the node act for the signer of a verified write on a document
/// ACP provider that cannot join the write's transaction. Such a provider
/// takes what the write owes it only under the signer's authority; a local
/// document ACP authorizes inside the transaction and reads none of this.
///
/// The credential is the signer's own and is used for the signer only.
#[derive(Clone, PartialEq, Eq)]
pub enum SignerCredential {
    /// Nothing. A create is admitted when the node holds a key for the
    /// signer. An update or a delete owes the provider nothing and is
    /// admitted on the signer's permission alone, where the provider proves
    /// its decisions (vera.rs); elsewhere it too needs the key.
    None,
    /// The signer's delegation to this node for its policy commands, as a
    /// caller presents one over HTTP. It covers whatever the write owes the
    /// provider until it expires. Beside an update or a delete it proves who
    /// is asking, and the signer's permission still decides.
    Delegation(String),
    /// The signer's delegation bound to the one command the write sends the
    /// provider: the registration of the document a create makes. It covers
    /// that command and nothing else, so an update or a delete, which send
    /// none, are refused with it, even where the node holds the key.
    RequestBound(String),
}

impl SignerCredential {
    /// `context` carrying this credential for `signer` and for no one else.
    pub(crate) fn carried_by(
        self,
        context: defra_core::RequestContext,
        signer: &str,
    ) -> defra_core::RequestContext {
        match self {
            Self::None => context.with_bearer(Some(signer.to_owned()), None),
            Self::Delegation(token) => context.with_bearer(Some(signer.to_owned()), Some(token)),
            Self::RequestBound(token) => context.with_request_delegation(signer.to_owned(), token),
        }
    }
}

impl std::fmt::Debug for SignerCredential {
    /// A delegation is a credential: its token is never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Delegation(_) => "Delegation(..)",
            Self::RequestBound(_) => "RequestBound(..)",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const OTHER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

    fn carried(credential: SignerCredential) -> defra_core::RequestContext {
        credential.carried_by(defra_core::RequestContext::default(), SIGNER)
    }

    #[test]
    fn a_credential_is_carried_for_its_signer_only() {
        let plain = carried(SignerCredential::Delegation("plain".into()));
        assert_eq!(plain.bearer_token_for(SIGNER).as_deref(), Some("plain"));
        assert_eq!(plain.bearer_token_for(OTHER), None);
        assert!(!plain.bearer_names_request());

        let bound = carried(SignerCredential::RequestBound("bound".into()));
        assert_eq!(bound.bearer_token_for(SIGNER).as_deref(), Some("bound"));
        assert_eq!(bound.bearer_token_for(OTHER), None);
        assert!(bound.bearer_names_request());
    }

    #[test]
    fn no_credential_reads_no_token_the_process_holds() {
        defra_core::signing::set_request_bearer_token(SIGNER, "stored");
        let none = carried(SignerCredential::None);
        assert_eq!(none.bearer_token_for(SIGNER), None);
        assert!(!none.bearer_names_request());
        defra_core::signing::clear_request_bearer_token(SIGNER);
    }

    #[test]
    fn a_credential_is_not_printed() {
        for credential in [
            SignerCredential::Delegation("secret".into()),
            SignerCredential::RequestBound("secret".into()),
        ] {
            assert!(!format!("{credential:?}").contains("secret"));
        }
    }
}
