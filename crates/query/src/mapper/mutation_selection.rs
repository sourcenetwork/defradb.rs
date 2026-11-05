use super::Requestable;

/// Require the physical document ID as an unaliased, unshadowed response field.
pub fn valid_unaliased_doc_id_selection(fields: &[Requestable]) -> bool {
    fields.iter().any(|field| {
        matches!(field, Requestable::Field(field) if field.name == "_docID" && field.alias.is_none())
    }) && !fields.iter().any(|field| match field {
        Requestable::Field(field) => field.output_name() == "_docID" && field.name != "_docID",
        Requestable::Select(field) => field.field.output_name() == "_docID",
        Requestable::Aggregate(field) => field.output_name() == "_docID",
        Requestable::Similarity(field) => field.output_name() == "_docID",
        Requestable::FullTextSearch(field) => field.output_name() == "_docID",
    })
}
