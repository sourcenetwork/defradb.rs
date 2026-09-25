//! A node judges its own writes by the merge validator before committing
//! them, so a write every peer would refuse is refused here first, with the
//! reason, instead of committed and silently rejected everywhere else.

use super::*;

/// A note is accepted when a grant names its `grant` value as writer;
/// `forged` is rejected outright; otherwise it defers on the grant. Reads
/// the note's own fields through the view, so a local write's uncommitted
/// composite has to be readable there.
struct NotesNeedGrant;

#[async_trait]
impl MergeValidator for NotesNeedGrant {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .ok_or("the candidate's own composite is not readable")?;
        let Some((_, FieldValue::Value(NormalValue::String(grant)))) =
            fields.into_iter().find(|(name, _)| name == "grant")
        else {
            return Ok(MergeVerdict::reject("note names no grant"));
        };
        if grant == "forged" {
            return Ok(MergeVerdict::reject("forged grant"));
        }
        let writer = NormalValue::String(grant);
        if view
            .find_documents("Grants", "writer", &writer)
            .await?
            .is_empty()
        {
            return Ok(MergeVerdict::defer(
                "no grant for this writer",
                [Awaited::immutable_field("Grants", "writer", writer)],
            ));
        }
        Ok(MergeVerdict::Accept)
    }
}

#[tokio::test]
async fn a_local_write_the_validator_rejects_is_refused_with_the_reason() {
    let node = Node::with_immutable_grants(Arc::new(NotesNeedGrant)).await;
    let error = node
        .try_create_locally("Notes", r#"{"grant": "forged"}"#)
        .await
        .unwrap_err();
    assert!(
        error.contains("refused by the merge validator") && error.contains("forged grant"),
        "{error}"
    );
    assert!(
        node.doc_ids("Notes").await.is_empty(),
        "the refused write was committed"
    );
}

#[tokio::test]
async fn a_local_write_the_validator_would_defer_is_refused_naming_what_it_awaits() {
    let node = Node::with_immutable_grants(Arc::new(NotesNeedGrant)).await;
    let error = node
        .try_create_locally("Notes", r#"{"grant": "alice"}"#)
        .await
        .unwrap_err();
    assert!(
        error.contains("every peer would defer") && error.contains("alice"),
        "{error}"
    );
    assert!(node.doc_ids("Notes").await.is_empty());
}

#[tokio::test]
async fn a_local_write_the_validator_accepts_commits() {
    let node = Node::with_immutable_grants(Arc::new(NotesNeedGrant)).await;
    // Grants is not claimed, so this write is not judged.
    node.create_locally("Grants", r#"{"writer": "alice", "label": "x"}"#)
        .await;
    node.create_locally("Notes", r#"{"grant": "alice"}"#).await;
    assert_eq!(node.doc_ids("Notes").await.len(), 1);
}
