use super::*;
use schema::{
    CType, CollectionSource, FieldDescription, FieldKind, IndexDescription, QuerySource,
    VectorEmbeddingDescription,
};

fn tally() -> CollectionVersion {
    CollectionVersion::new(
        "Tally",
        "tally-v1",
        "col-tally",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "count", FieldKind::int(), schema::CType::LwwRegister)
                .with_crdt_type(CType::PnCounter),
            FieldDescription::new(
                "3",
                "label",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("4", "note", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

fn set(names: &[&str]) -> RapidHashSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn filter(conditions: serde_json::Value) -> Filter {
    Filter::from_conditions(conditions.as_object().cloned().unwrap_or_default())
}

/// An increment decides nothing, so it adds no validated part.
#[test]
fn a_counter_increment_is_not_a_dependency() {
    let collection = tally();
    assert_eq!(
        planned(&crdt::Crdts::default(), &collection, &set(&["count"]), None),
        Some(set(&[]))
    );
    assert_eq!(
        planned(
            &crdt::Crdts::default(),
            &collection,
            &set(&["count", "label"]),
            None
        ),
        Some(set(&["label"]))
    );
}

/// An index over the counter reads its old value, so the counter is a
/// dependency again.
#[test]
fn an_indexed_counter_stays_a_dependency() {
    let collection =
        tally().with_index(IndexDescription::new("by_count").with_field("count", false));
    assert_eq!(
        planned(&crdt::Crdts::default(), &collection, &set(&["count"]), None),
        Some(set(&["count"]))
    );
}

/// Every component of an index over a written field is read for the old
/// entry; an index over other fields adds nothing.
#[test]
fn a_written_index_brings_its_components() {
    let collection = tally()
        .with_index(
            IndexDescription::new("by_label_note")
                .with_field("label", false)
                .with_field("note", false),
        )
        .with_index(IndexDescription::new("by_count").with_field("count", false));
    assert_eq!(
        planned(&crdt::Crdts::default(), &collection, &set(&["label"]), None),
        Some(set(&["label", "note"]))
    );
}

/// A written embedding source brings every source and the embedding itself.
#[test]
fn a_written_embedding_source_brings_the_embedding() {
    let mut collection = tally();
    collection.fields.push(FieldDescription::new(
        "5",
        "vector",
        FieldKind::float64_array(),
        schema::CType::LwwRegister,
    ));
    collection
        .vector_embeddings
        .push(VectorEmbeddingDescription {
            field_name: "vector".to_string(),
            fields: vec!["label".to_string(), "note".to_string()],
            model: "model".to_string(),
            provider: "openai".to_string(),
            template: String::new(),
            url: String::new(),
        });
    assert_eq!(
        planned(&crdt::Crdts::default(), &collection, &set(&["label"]), None),
        Some(set(&["label", "note", "vector"]))
    );
}

/// A filter decides which documents change, so the fields it compares are
/// dependencies; the document ID is identity, not a part.
#[test]
fn a_filter_brings_the_fields_it_compares() {
    let collection = tally();
    let selected = filter(serde_json::json!({
        "_docID": {"_eq": "bae-1"},
        "_or": [{"note": {"_eq": "a"}}, {"label": {"_eq": "b"}}]
    }));
    assert_eq!(
        planned(
            &crdt::Crdts::default(),
            &collection,
            &set(&["count"]),
            Some(&selected)
        ),
        Some(set(&["note", "label"]))
    );
}

/// A filter through a relation or an alias decides on other documents too.
#[test]
fn a_filter_past_the_document_needs_the_whole_read() {
    let collection = tally();
    let through_relation = filter(serde_json::json!({"author": {"name": {"_eq": "a"}}}));
    let through_alias = filter(serde_json::json!({"_alias": {"total": {"_gt": 1}}}));
    let unknown = filter(serde_json::json!({"missing": {"_eq": 1}}));
    for selected in [through_relation, through_alias, unknown] {
        assert_eq!(
            planned(
                &crdt::Crdts::default(),
                &collection,
                &set(&["label"]),
                Some(&selected)
            ),
            None
        );
    }
}

/// A migrated collection projects like any other: a document whose stored
/// version is not current is read whole by the projected read itself.
#[test]
fn a_migrated_collection_projects() {
    let mut collection = tally();
    collection.previous_version =
        Some(CollectionSource::new("tally-v0").with_transform("migrate-tally"));
    assert_eq!(
        planned(&crdt::Crdts::default(), &collection, &set(&["label"]), None),
        Some(set(&["label"]))
    );
}

/// A view's documents come from its query, and a non-scalar patch has nested
/// dependencies this module does not enumerate.
#[test]
fn views_and_structural_patches_need_the_whole_read() {
    let mut view = tally();
    view.query = Some(QuerySource::new(serde_json::json!({})));
    assert_eq!(
        planned(&crdt::Crdts::default(), &view, &set(&["label"]), None),
        None
    );
    assert_eq!(
        planned(&crdt::Crdts::default(), &tally(), &set(&["missing"]), None),
        None
    );
    assert_eq!(
        planned(&crdt::Crdts::default(), &tally(), &set(&[]), None),
        None
    );
}

/// The write path names its increments and its maintained indexes itself.
#[test]
fn written_follows_the_write_paths_increments_and_indexes() {
    let collection = tally();
    let index = IndexDescription::new("by_note").with_field("note", false);
    assert_eq!(
        written(
            &crdt::Crdts::default(),
            &collection,
            [&index],
            &set(&["count", "note"]),
            |name| name == "count"
        ),
        set(&["note"])
    );
    assert_eq!(
        written(
            &crdt::Crdts::default(),
            &collection,
            &[] as &[IndexDescription],
            &set(&["count", "note"]),
            |_| false
        ),
        set(&["count", "note"])
    );
}

/// A run after a lost race on a part merges the fields it only writes: they
/// decide nothing, so the run cannot lose that race again.
#[test]
fn a_run_after_a_lost_part_merges_the_fields_it_only_writes() {
    let collection = tally();
    let run = Run::AfterLostPart;
    assert_eq!(
        run.planned(
            &crdt::Crdts::default(),
            &collection,
            &set(&["label", "note", "count"]),
            None
        ),
        Some(set(&[]))
    );
    assert_eq!(
        run.written(
            &crdt::Crdts::default(),
            &collection,
            &[] as &[IndexDescription],
            &set(&["label", "note"]),
            |_| false
        ),
        set(&[])
    );
    assert_eq!(
        Run::First.planned(
            &crdt::Crdts::default(),
            &collection,
            &set(&["label", "note"]),
            None
        ),
        Some(set(&["label", "note"]))
    );
}

/// A field whose old value a rule reads is decided on every run: an index
/// component, an embedding source, an immutable field, a compared field, and
/// every field of a downsampled collection.
#[test]
fn a_run_after_a_lost_part_still_decides_what_a_rule_reads() {
    let run = Run::AfterLostPart;
    let indexed = tally().with_index(
        IndexDescription::new("by_label_note")
            .with_field("label", false)
            .with_field("note", false),
    );
    assert_eq!(
        run.planned(&crdt::Crdts::default(), &indexed, &set(&["label"]), None),
        Some(set(&["label", "note"]))
    );

    let mut embedded = tally();
    embedded.fields.push(FieldDescription::new(
        "5",
        "vector",
        FieldKind::float64_array(),
        schema::CType::LwwRegister,
    ));
    embedded.vector_embeddings.push(VectorEmbeddingDescription {
        field_name: "vector".to_string(),
        fields: vec!["label".to_string()],
        model: "model".to_string(),
        provider: "openai".to_string(),
        template: String::new(),
        url: String::new(),
    });
    assert_eq!(
        run.planned(&crdt::Crdts::default(), &embedded, &set(&["label"]), None),
        Some(set(&["label", "vector"]))
    );

    let mut immutable = tally();
    for field in &mut immutable.fields {
        field.immutable = field.name == "label";
    }
    assert_eq!(
        run.planned(
            &crdt::Crdts::default(),
            &immutable,
            &set(&["label", "note"]),
            None
        ),
        Some(set(&["label"]))
    );

    let selected = filter(serde_json::json!({"note": {"_eq": "a"}}));
    assert_eq!(
        run.planned(
            &crdt::Crdts::default(),
            &tally(),
            &set(&["note"]),
            Some(&selected)
        ),
        Some(set(&["note"]))
    );

    let mut downsampled = tally();
    downsampled.downsample_source = Some(QuerySource::new(serde_json::json!({})));
    assert_eq!(
        run.planned(
            &crdt::Crdts::default(),
            &downsampled,
            &set(&["label", "count"]),
            None
        ),
        Some(set(&["label"]))
    );
}

/// Only a race lost on a part makes the next run one after a lost part.
#[test]
fn the_run_follows_the_reason_the_previous_attempt_lost() {
    let lost = |access| storage::corekv::TransactionConflict {
        access,
        winning_write: regolith::WriteKind::Merge,
        observed_seq: 1,
        latest_seq: 2,
    };
    assert_eq!(Run::after(None), Run::First);
    assert_eq!(
        Run::after(Some(&lost(regolith::Access::ReadParts))),
        Run::AfterLostPart
    );
    for access in [
        regolith::Access::Read,
        regolith::Access::ReadPresence,
        regolith::Access::Put,
        regolith::Access::Merge,
    ] {
        assert_eq!(Run::after(Some(&lost(access))), Run::First);
    }
}

/// A Text field's edits count in its old text, so a run after a lost race
/// still decides on it, and diffs against the text that won.
#[test]
fn a_run_after_a_lost_part_still_decides_a_text_field() {
    let mut collection = tally();
    collection.fields.push(FieldDescription::new(
        "5",
        "body",
        FieldKind::string(),
        CType::Text,
    ));
    assert_eq!(
        Run::AfterLostPart.planned(
            &crdt::Crdts::default(),
            &collection,
            &set(&["body", "note"]),
            None
        ),
        Some(set(&["body"]))
    );
}
