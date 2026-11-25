//! A new index that repeats the definition of one the collection already
//! lists is refused, so a second copy never doubles every write's index
//! maintenance.

use schema::{CollectionVersion, IndexDescription, IndexKind, IndexedFieldDescription};

use crate::error::{Error, Result};
use crate::index::listing::is_hidden_relation_index;

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
    if !name.is_empty() && version.indexes.iter().any(|index| index.name == name) {
        return Ok(());
    }
    let kind = normalized_kind(kind);
    match version.indexes.iter().find(|index| {
        index.fields == fields
            && IndexDescription::clone(index)
                .normalized()
                .kind
                .map(normalized_kind)
                == Some(kind)
            && !is_hidden_relation_index(version, index)
    }) {
        Some(existing) => Err(Error::IndexDefinitionExists(existing.name.clone())),
        None => Ok(()),
    }
}

fn normalized_kind(kind: IndexKind) -> IndexKind {
    match kind {
        IndexKind::Vector(vector) => IndexKind::Vector(vector.normalized()),
        ordered @ IndexKind::Ordered(_) => ordered,
    }
}
