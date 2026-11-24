//! Per-frame write authorization for updates to protected documents.

use super::hook::CompositeFrame;
use super::*;

/// What the write check says about a frame it does not let through.
pub(crate) enum Refusal {
    /// The frame's outcome, as the hook gave it.
    Outcome(MergeOutcome),
    /// A signed frame the provider refused: it waits, parked by its
    /// document, for that document's relationships to change.
    Parked(MergeOutcome),
}

impl Refusal {
    pub(crate) fn into_outcome(self) -> MergeOutcome {
        match self {
            Self::Outcome(outcome) | Self::Parked(outcome) => outcome,
        }
    }
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Judge one composite by its own signature. Ancestors loaded from the
    /// blockstore and composites linked from a collection block arrive with
    /// the carrier's metadata, whose signer says nothing about this block.
    pub(crate) async fn check_protected_update(
        &self,
        cid: &Cid,
        block: &Block,
        payload: &defra_core::block::CompositeDeltaPayload,
        doc_id: &str,
        collection: &CollectionVersion,
        view: Option<datastore::NamespaceView>,
    ) -> std::result::Result<Option<Refusal>, MergeError> {
        let Some(hook) = self.composite_merge_hook() else {
            return Ok(None);
        };
        if view.is_none() && hook.supports_transactions() {
            // Local authorization is evaluated below in the mutation snapshot.
            return Ok(None);
        }
        let scoped = view.and_then(|view| hook.bind_transaction(view));
        let hook = scoped.as_ref().unwrap_or(hook);
        let is_genesis = block.heads.as_deref().is_none_or(<[Cid]>::is_empty);
        if collection.policy.is_none() || is_genesis || !hook.guards_protected_updates() {
            return Ok(None);
        }

        let signer = frame_signer(cid, self.verify_block_signature(cid, block, &[]).await);
        let outcome = hook
            .on_protected_update(
                doc_id,
                collection,
                CompositeFrame {
                    is_genesis,
                    status: payload.status,
                    signer: signer.as_ref(),
                },
            )
            .await?;
        // A frame that names nobody names nobody a grant could ever cover:
        // its refusal is final. The hook judged the same `signer`.
        Ok(outcome.map(|outcome| match outcome {
            MergeOutcome::Rejected { reason }
                if signer.is_some() && hook.parks_refusals(collection) =>
            {
                Refusal::Parked(MergeOutcome::retryable_skip(format!("parked: {reason}")))
            }
            outcome => Refusal::Outcome(outcome),
        }))
    }
}

/// Whom a frame is judged as signed by: the DID its own signature names.
/// A signature that fails to verify, or that names something other than a
/// DID, names nobody, so the frame is judged as unsigned and its refusal is
/// final; the root's own verification failure is already refused at
/// dispatch.
fn frame_signer(
    cid: &Cid,
    verified: std::result::Result<Option<String>, MergeError>,
) -> Option<identity::Did> {
    match verified {
        Ok(Some(signer)) => identity::Did::new(signer)
            .inspect_err(|error| tracing::debug!(%cid, %error, "Composite signer is not a DID"))
            .ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::debug!(%cid, %error, "Composite signature did not verify");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid() -> Cid {
        Cid::default()
    }

    /// A verified signer that is not a DID names nobody, as an unsigned
    /// frame does, so its refusal is final rather than parked.
    #[test]
    fn a_signer_that_is_not_a_did_is_judged_unsigned() {
        assert_eq!(frame_signer(&cid(), Ok(Some("not-a-did".into()))), None);
        assert_eq!(frame_signer(&cid(), Ok(None)), None);
        assert_eq!(
            frame_signer(
                &cid(),
                Err(MergeError::SignatureVerificationFailed {
                    cid: cid(),
                    reason: "bad".into(),
                })
            ),
            None
        );
        assert_eq!(
            frame_signer(&cid(), Ok(Some("did:key:z6Mkowner".into()))),
            Some(identity::Did::new("did:key:z6Mkowner").unwrap())
        );
    }
}
