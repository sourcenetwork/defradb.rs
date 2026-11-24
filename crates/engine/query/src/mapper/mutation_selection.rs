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
/// Identify a mutation limited to one physical document. Delete execution drops
/// extra filter predicates, so deletion requires the DocID predicate alone.
pub fn single_document_mutation_target(mutation: &super::Mutation, delete: bool) -> Option<&str> {
    if delete
        && mutation
            .filter
            .as_ref()
            .is_some_and(|filter| filter.conditions().len() != 1)
    {
        return None;
    }
    match (&mutation.doc_ids, &mutation.filter) {
        (Some(ids), None) if ids.len() == 1 => Some(ids[0].as_str()),
        (None, Some(filter)) => {
            let condition = filter.conditions().get("_docID")?.as_object()?;
            if condition.len() != 1 {
                return None;
            }
            condition.get("_eq")?.as_str()
        }
        _ => None,
    }
}
