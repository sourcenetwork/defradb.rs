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

/// The single-mutation path, which a GraphQL request with one mutation, a
/// REST document write and a backup import take, is judged like the batch
/// and `/tx` paths: create, create-many, update and delete.
#[tokio::test]
async fn a_single_mutation_is_judged_on_every_write_it_makes() {
    let node = Node::with_immutable_grants(Arc::new(NotesNeedGrant)).await;
    let mutator = AutoCommitMutator::new(node.db.clone());
    mutator
        .create(
            "Grants",
            Document::from_json_str(r#"{"writer": "alice", "label": "x"}"#).unwrap(),
        )
        .await
        .unwrap();

    let error = mutator
        .create(
            "Notes",
            Document::from_json_str(r#"{"grant": "forged"}"#).unwrap(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("forged grant"), "create: {error}");
    let error = mutator
        .create_many(
            "Notes",
            vec![
                Document::from_json_str(r#"{"grant": "alice"}"#).unwrap(),
                Document::from_json_str(r#"{"grant": "forged"}"#).unwrap(),
            ],
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("forged grant"), "create_many: {error}");
    assert!(
        node.doc_ids("Notes").await.is_empty(),
        "a refused write, or its batch, was committed"
    );

    let created = mutator
        .create(
            "Notes",
            Document::from_json_str(r#"{"grant": "alice"}"#).unwrap(),
        )
        .await
        .unwrap();
    let mut forged = Document::from_json_str(r#"{"grant": "forged"}"#).unwrap();
    forged.set_id(created.doc_id.clone());
    let error = mutator
        .update("Notes", forged, ["grant".to_string()].into_iter().collect())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("forged grant"), "update: {error}");

    // A delete composite links no fields, so this validator finds no grant.
    let error = mutator
        .delete("Notes", &created.doc_id)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("names no grant"), "delete: {error}");
    let error = mutator
        .delete_many_impl("Notes", std::slice::from_ref(&created.doc_id))
        .await
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(error.contains("names no grant"), "delete_many: {error}");
    assert_eq!(
        node.doc_ids("Notes").await,
        vec![created.doc_id.to_string()]
    );
}
