//! The GraphQL queries the REST reads run.

use super::trait_def::CollectionDocIdsPagination;

pub(super) fn build_list_ids_query(
    collection: &str,
    pagination: Option<CollectionDocIdsPagination>,
) -> String {
    match pagination {
        Some(pagination) => format!(
            r#"{{ {collection}(limit: {limit}, offset: {offset}) {{ _docID }} }}"#,
            collection = collection,
            limit = pagination.limit,
            offset = pagination.offset
        ),
        None => format!(
            r#"{{ {collection} {{ _docID }} }}"#,
            collection = collection
        ),
    }
}

pub(super) fn build_count_query(collection: &str) -> String {
    format!(
        r#"{{ total: COUNT({collection}: {{}}) }}"#,
        collection = collection
    )
}
