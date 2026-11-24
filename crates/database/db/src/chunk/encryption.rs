//! A chunk list holds hashes of its field's text, so it is encrypted with the field.

use std::borrow::Cow;

use defra_core::encryption::EncryptionConfig;
use schema::CollectionVersion;

/// `config` with each chunk list added where its field is encrypted by itself.
pub(crate) fn with_lists<'c>(
    config: Option<&'c EncryptionConfig>,
    schema: &CollectionVersion,
) -> Option<Cow<'c, EncryptionConfig>> {
    let config = config?;
    let lists: Vec<String> = schema
        .chunked_fields
        .iter()
        .filter(|chunked| config.should_encrypt_individual_field(&chunked.field_name))
        .map(|chunked| chunked.list_field())
        .filter(|list| !config.encrypt_fields.contains(list))
        .collect();
    if lists.is_empty() {
        return Some(Cow::Borrowed(config));
    }
    let mut config = config.clone();
    config.encrypt_fields.extend(lists);
    Some(Cow::Owned(config))
}
