//! A new index that repeats the definition of one the collection already
//! lists is refused, so a second copy never doubles every write's index
//! maintenance.

use schema::{CollectionVersion, IndexDescription, IndexKind, IndexedFieldDescription};

use crate::error::{Error, Result};
use crate::index::{fulltext_index_description, listing::is_hidden_relation_index};

/// Refuse an index named `name` (generated when empty) over `fields` of
/// `kind` when `version` lists an index with the same fields, directions and
/// kind. A given name that is already taken is left for the name check, which
/// reports it in Go's words.
///
/// `version` is the definition the creating transaction read and validates at
/// commit, so a duplicate committed concurrently re-runs this check.
///
/// Only an index creation request runs it (`DB::create_index`,
/// `DB::start_index` and their in-transaction form). `ensure_ordinary_index`
/// reconciles a stable name and keeps defining it beside an index of the same
/// definition, as an embedder's startup expects.
pub(crate) fn refuse_duplicate_index(
    version: &CollectionVersion,
    name: &str,
    fields: &[IndexedFieldDescription],
    kind: IndexKind,
) -> Result<()> {
    if !name.is_empty()
        && (version.indexes.iter().any(|index| index.name == name)
            || version
                .fulltext_indexes
                .iter()
                .any(|fulltext| crate::index::fulltext_index_name(&fulltext.field_name) == name))
    {
        return Ok(());
    }
    let kind = normalized_kind(kind);
    let existing_name = version
        .indexes
        .iter()
        .find(|index| {
            index.fields == fields
                && IndexDescription::clone(index)
                    .normalized()
                    .kind
                    .map(normalized_kind)
                    == Some(kind.clone())
                && !is_hidden_relation_index(version, index)
        })
        .map(|index| index.name.clone())
        .or_else(|| {
            let IndexKind::FullText(fulltext) = kind else {
                return None;
            };
            version
                .fulltext_indexes
                .iter()
                .find(|existing| existing.field_name == fulltext.field_name)
                .map(|existing| fulltext_index_description(existing).name)
        });
    match existing_name {
        Some(existing) => Err(Error::IndexDefinitionExists(existing)),
        None => Ok(()),
    }
}

fn normalized_kind(kind: IndexKind) -> IndexKind {
    match kind {
        IndexKind::Vector(vector) => IndexKind::Vector(vector.normalized()),
        ordered @ IndexKind::Ordered(_) => ordered,
        fulltext @ IndexKind::FullText(_) => fulltext,
    }
}
