//! Nested fields an ORDER BY reads but the selection does not render.

use crate::mapper::{Requestable, Select};

/// `(relation output key, field key)` pairs that ORDER BY joined in only to
/// sort on, stripped from each rendered document afterwards.
///
/// An order path segment names a field or, through `_alias`, its alias, so a
/// segment matches a selection by either. A nested relation the selection
/// renders is never ordering-only.
pub(super) fn ordering_only_fields(select: &Select) -> Vec<(String, String)> {
    let Some(order_by) = &select.order_by else {
        return Vec::new();
    };
    order_by
        .conditions
        .iter()
        .filter(|condition| condition.fields.len() > 1)
        .filter_map(|condition| {
            let (relation, field) = (&condition.fields[0], &condition.fields[1]);
            let Some(nested) = select
                .fields
                .iter()
                .find_map(|requestable| match requestable {
                    Requestable::Select(nested)
                        if names(&nested.field.name, nested.field.output_name(), relation) =>
                    {
                        Some(nested)
                    }
                    _ => None,
                })
            else {
                return Some((relation.clone(), field.clone()));
            };
            let rendered = nested.fields.iter().any(|requestable| match requestable {
                Requestable::Field(f) => names(&f.name, f.output_name(), field),
                Requestable::Select(s) => names(&s.field.name, s.field.output_name(), field),
                _ => false,
            });
            (!rendered).then(|| (nested.field.output_name().to_string(), field.clone()))
        })
        .collect()
}

fn names(name: &str, output_name: &str, segment: &str) -> bool {
    name == segment || output_name == segment
}

#[cfg(test)]
mod tests {
    use super::ordering_only_fields;

    fn strip_list(query: &str) -> Vec<(String, String)> {
        ordering_only_fields(&crate::parse_query(query).unwrap()[0])
    }

    #[test]
    fn selected_nested_relation_is_kept() {
        let query = "query { Publisher(order: {printed: {author: {name: ASC}}}) { name printed { name author { name } } } }";
        assert!(strip_list(query).is_empty());
    }

    #[test]
    fn aliased_relation_and_field_are_kept() {
        for query in [
            "query { Book(order: {_alias: {writer: {age: ASC}}}) { name writer: author { age } } }",
            "query { Book(order: {author: {_alias: {authorAge: ASC}}}) { name author { authorAge: age } } }",
        ] {
            assert!(strip_list(query).is_empty(), "{query}");
        }
    }

    #[test]
    fn unselected_order_field_is_stripped_under_the_output_key() {
        let query = "query { Book(order: {author: {age: ASC}}) { name writer: author { name } } }";
        assert_eq!(strip_list(query), vec![("writer".into(), "age".into())]);
    }
}
