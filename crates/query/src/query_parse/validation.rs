//! Post-parse schema validation for GraphQL operations.
//!
//! Validates that all collection names referenced in a parsed operation
//! actually exist in the database, catching typos at parse time rather
//! than during execution.

use crate::collection_provider::CollectionProvider;
use crate::error::{QueryError, Result};
use crate::mapper::{Mutation, Select};
use crate::query_parse::ParsedOperation;
use schema::CollectionVersion;

/// Validate that all collections referenced in a parsed operation exist.
///
/// This runs after parsing but before execution, giving clear errors
/// like "Cannot query collection 'Articel': collection not found"
/// instead of opaque execution failures.
pub async fn validate_parsed_operation(
    op: &ParsedOperation,
    provider: &dyn CollectionProvider,
) -> Result<()> {
    match op {
        ParsedOperation::Query { selects, .. } => {
            for select in selects {
                validate_select_collection(select, provider, "Query").await?;
            }
        }
        ParsedOperation::Mutation { mutations, .. } => {
            for mutation in mutations {
                validate_mutation_collection(mutation, provider).await?;
            }
        }
        ParsedOperation::Subscription { select, .. } => {
            validate_select_collection(select, provider, "Subscription").await?;
        }
        ParsedOperation::Introspection { .. } => {}
    }
    Ok(())
}

/// Validate that the collection referenced by a Select exists.
///
/// Skips internal collections (`_commits`) and nested relation selects
/// (those are field names resolved later by the planner).
async fn validate_select_collection(
    select: &Select,
    provider: &dyn CollectionProvider,
    root_type: &str,
) -> Result<()> {
    let name = &select.collection_name;

    if is_internal_collection(name) {
        return Ok(());
    }

    if provider.get_collection(name).await?.is_none() {
        let suggestion = suggest_collection(name, provider).await;
        let mut msg = format!("Cannot query field \"{}\" on type \"{}\".", name, root_type);
        if let Some(suggested) = suggestion {
            msg.push_str(&format!(" Did you mean \"{}\"?", suggested));
        }
        return Err(QueryError::parse(msg));
    }

    Ok(())
}

/// Validate that the collection referenced by a Mutation exists.
async fn validate_mutation_collection(
    mutation: &Mutation,
    provider: &dyn CollectionProvider,
) -> Result<()> {
    let name = &mutation.collection_name;

    let Some(collection) = provider.get_collection(name).await? else {
        let suggestion = suggest_collection(name, provider).await;
        let field_name = match mutation.mutation_type {
            crate::mapper::MutationType::Add => format!("add_{}", name),
            crate::mapper::MutationType::Update => format!("update_{}", name),
            crate::mapper::MutationType::Delete => format!("delete_{}", name),
            crate::mapper::MutationType::Upsert => format!("upsert_{}", name),
            crate::mapper::MutationType::Truncate => format!("truncate_{}", name),
        };
        let mut msg = format!(
            "Cannot query field \"{}\" on type \"Mutation\".",
            field_name
        );
        if let Some(suggested) = suggestion {
            msg.push_str(&format!(" Did you mean \"{}\"?", suggested));
        }
        return Err(QueryError::parse(msg));
    };

    validate_encrypt_fields(mutation, &collection)
}

/// Go types `encryptFields` as a list of the collection's `<Name>ExplicitField`
/// enum, which holds every field not starting with `_`, so a built-in or unknown
/// field fails GraphQL validation before execution. The text is graphql-go's:
/// one line per invalid item, each numbered within that item's own messages.
fn validate_encrypt_fields(mutation: &Mutation, collection: &CollectionVersion) -> Result<()> {
    let is_explicit =
        |field: &str| !field.starts_with('_') && collection.fields.iter().any(|f| f.name == field);
    let mut message = String::new();
    for invalid in mutation.encrypt_fields.iter().filter(|f| !is_explicit(f)) {
        message.push_str(&format!(
            "\nIn element #1: Expected type \"{}ExplicitField\", found {invalid}.",
            mutation.collection_name
        ));
    }
    if message.is_empty() {
        return Ok(());
    }
    Err(QueryError::execution(format!(
        "Argument \"encryptFields\" has invalid value [{}].{message}",
        mutation.encrypt_fields.join(", ")
    )))
}

fn is_internal_collection(name: &str) -> bool {
    name.starts_with('_')
}

/// Suggest a similar collection name using edit distance.
async fn suggest_collection(name: &str, provider: &dyn CollectionProvider) -> Option<String> {
    let collections = provider.list_collections().await.ok()?;
    let name_lower = name.to_lowercase();

    let mut best: Option<(String, usize)> = None;
    for candidate in &collections {
        let dist = edit_distance(&name_lower, &candidate.to_lowercase());
        let threshold = (name.len() / 3).max(2);
        if dist <= threshold && best.as_ref().is_none_or(|(_, d)| dist < *d) {
            best = Some((candidate.clone(), dist));
        }
    }

    best.map(|(name, _)| name)
}

/// Simple Levenshtein edit distance.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection_provider::StaticCollectionProvider;
    use crate::document::DocumentMapping;

    fn make_provider(names: &[&str]) -> StaticCollectionProvider {
        let collections: Vec<CollectionVersion> = names
            .iter()
            .map(|name| serde_json::from_value(serde_json::json!({ "Name": name })).unwrap())
            .collect();
        StaticCollectionProvider::new(collections)
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("Article", "Articel"), 2);
        assert_eq!(edit_distance("User", "User"), 0);
        assert_eq!(edit_distance("User", "Users"), 1);
        assert_eq!(edit_distance("abc", "xyz"), 3);
    }

    #[test]
    fn test_valid_query_passes() {
        workers::block_on(async {
            let provider = make_provider(&["User", "Article"]);
            let op = ParsedOperation::Query {
                selects: vec![Select::new("User")],
                explain: None,
                exhaustive: false,
            };
            assert!(validate_parsed_operation(&op, &provider).await.is_ok());
        })
        .unwrap()
    }

    #[test]
    fn test_invalid_query_collection() {
        workers::block_on(async {
            let provider = make_provider(&["User", "Article"]);
            let op = ParsedOperation::Query {
                selects: vec![Select::new("Uzr")],
                explain: None,
                exhaustive: false,
            };
            let err = validate_parsed_operation(&op, &provider).await.unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("Uzr"), "error should contain collection name");
            assert!(msg.contains("Cannot query field \"Uzr\" on type \"Query\"."));
            assert!(
                msg.contains("Did you mean \"User\""),
                "error should suggest User, got: {}",
                msg
            );
        })
        .unwrap()
    }

    #[test]
    fn test_commits_skipped() {
        workers::block_on(async {
            let provider = make_provider(&["User"]);
            let op = ParsedOperation::Query {
                selects: vec![Select::new("_commits")],
                explain: None,
                exhaustive: false,
            };
            assert!(validate_parsed_operation(&op, &provider).await.is_ok());
        })
        .unwrap()
    }

    #[test]
    fn test_invalid_mutation_collection() {
        workers::block_on(async {
            let provider = make_provider(&["User"]);
            let mutation = Mutation {
                mutation_type: crate::mapper::MutationType::Add,
                collection_name: "Usr".to_string(),
                alias: None,
                create_input: vec![],
                update_input: Default::default(),
                doc_ids: None,
                filter: None,
                fields: vec![],
                document_mapping: DocumentMapping::new(),
                encrypt_doc: false,
                encrypt_fields: vec![],
            };
            let op = ParsedOperation::Mutation {
                mutations: vec![mutation],
                explain: None,
            };
            let err = validate_parsed_operation(&op, &provider).await.unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("Cannot query field \"add_Usr\" on type \"Mutation\"."));
            assert!(msg.contains("Did you mean \"User\""));
        })
        .unwrap()
    }

    fn encrypt_fields_result(encrypt_fields: &[&str]) -> Result<()> {
        let collection: CollectionVersion = serde_json::from_value(serde_json::json!({
            "Name": "Users",
            "Fields": [{ "Name": "_docID" }, { "Name": "name" }, { "Name": "age" }],
        }))
        .unwrap();
        let mutation = Mutation {
            mutation_type: crate::mapper::MutationType::Add,
            collection_name: "Users".to_string(),
            alias: None,
            create_input: vec![],
            update_input: Default::default(),
            doc_ids: None,
            filter: None,
            fields: vec![],
            document_mapping: DocumentMapping::new(),
            encrypt_doc: false,
            encrypt_fields: encrypt_fields.iter().map(|f| f.to_string()).collect(),
        };
        let op = ParsedOperation::Mutation {
            mutations: vec![mutation],
            explain: None,
        };
        let provider = StaticCollectionProvider::new(vec![collection]);
        workers::block_on(async { validate_parsed_operation(&op, &provider).await }).unwrap()
    }

    #[test]
    fn test_encrypt_fields_rejects_builtin_field_like_go() {
        let msg = encrypt_fields_result(&["_docID"]).unwrap_err().to_string();
        assert_eq!(
            msg,
            "Argument \"encryptFields\" has invalid value [_docID].\nIn element #1: Expected type \"UsersExplicitField\", found _docID."
        );
    }

    #[test]
    fn test_encrypt_fields_rejects_unknown_field_like_go() {
        let msg = encrypt_fields_result(&["name", "points"])
            .unwrap_err()
            .to_string();
        assert_eq!(
            msg,
            "Argument \"encryptFields\" has invalid value [name, points].\nIn element #1: Expected type \"UsersExplicitField\", found points."
        );
    }

    #[test]
    fn test_encrypt_fields_reports_every_invalid_field_like_go() {
        let msg = encrypt_fields_result(&["_docID", "name", "points"])
            .unwrap_err()
            .to_string();
        assert_eq!(
            msg,
            "Argument \"encryptFields\" has invalid value [_docID, name, points].\nIn element #1: Expected type \"UsersExplicitField\", found _docID.\nIn element #1: Expected type \"UsersExplicitField\", found points."
        );
    }

    #[test]
    fn test_encrypt_fields_accepts_explicit_fields() {
        assert!(encrypt_fields_result(&["name", "age"]).is_ok());
        assert!(encrypt_fields_result(&[]).is_ok());
    }

    #[test]
    fn test_introspection_skipped() {
        workers::block_on(async {
            let provider = make_provider(&[]);
            let op = ParsedOperation::Introspection {
                query: "{ __schema { types { name } } }".to_string(),
            };
            assert!(validate_parsed_operation(&op, &provider).await.is_ok());
        })
        .unwrap()
    }

    #[test]
    fn test_no_suggestion_for_distant_name() {
        workers::block_on(async {
            let provider = make_provider(&["User"]);
            let op = ParsedOperation::Query {
                selects: vec![Select::new("CompletelyDifferent")],
                explain: None,
                exhaustive: false,
            };
            let err = validate_parsed_operation(&op, &provider).await.unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("CompletelyDifferent"));
            assert!(!msg.contains("Did you mean"));
        })
        .unwrap()
    }
}
