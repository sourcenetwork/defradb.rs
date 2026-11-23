//! The documents an import has gathered and not yet saved: consecutive
//! documents of one collection, saved in one request.

use rapidhash::RapidHashSet;
use serde_json::{Map, Value as JsonValue};

/// The most documents one save holds.
const MAX_DOCS: usize = 128;

/// The most JSON bytes one save holds.
const MAX_BYTES: usize = 1 << 20;

/// A document waiting in the batch.
pub(super) struct Gathered {
    /// Its fields, relations remapped to imported identities.
    pub(super) fields: Map<String, JsonValue>,
    /// The file's IDs for it, registered as aliases once it is saved.
    pub(super) aliases: Vec<String>,
    /// Its relations whose targets were not imported yet: (field, file ID).
    pub(super) deferred: Vec<(String, String)>,
}

/// The unsaved documents of one collection.
#[derive(Default)]
pub(super) struct Batch {
    docs: Vec<Gathered>,
    /// Every alias of the documents in the batch.
    aliases: RapidHashSet<String>,
    bytes: usize,
}

impl Batch {
    /// Whether a document must wait for this batch to be saved: one of its
    /// aliases is already waiting here, so its alias check must see that
    /// one registered (`proofs/lean/BackupImportSigner.lean`
    /// `gather_refuses_iff`), or one of its relations points at a document
    /// waiting here, whose new ID only the save gives.
    pub(super) fn must_save_before<'a, A, T>(&self, aliases: A, targets: T) -> bool
    where
        A: IntoIterator<Item = &'a String>,
        T: IntoIterator<Item = &'a str>,
    {
        aliases
            .into_iter()
            .any(|alias| self.aliases.contains(alias))
            || targets
                .into_iter()
                .any(|target| self.aliases.contains(target))
    }

    /// Gathers `doc`.
    pub(super) fn push(&mut self, doc: Gathered) {
        self.bytes += JsonValue::Object(doc.fields.clone()).to_string().len();
        self.aliases.extend(doc.aliases.iter().cloned());
        self.docs.push(doc);
    }

    /// Whether the batch holds as much as one save may.
    pub(super) fn is_full(&self) -> bool {
        self.docs.len() >= MAX_DOCS || self.bytes >= MAX_BYTES
    }

    pub(super) fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// The gathered documents, in file order, leaving the batch empty.
    pub(super) fn take(&mut self) -> Vec<Gathered> {
        self.aliases.clear();
        self.bytes = 0;
        std::mem::take(&mut self.docs)
    }
}
