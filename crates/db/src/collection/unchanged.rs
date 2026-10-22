use super::*;
use query::update_parts::Run;
use rapidhash::RapidHashSet;

impl Collection {
    /// Returns whether no field remains to write. Call only without explicit
    /// encryption and outside downsample sources, whose reassertions are samples.
    pub(crate) async fn retain_changed_fields(
        &self,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc: &Document,
        modified_fields: &mut RapidHashSet<String>,
        run: Run,
    ) -> Result<bool> {
        let dependencies = run.written(
            self.schema(),
            self.write_indexes(),
            modified_fields,
            |field| doc.pending_op(field).is_some(),
        );
        let compared: RapidHashSet<String> = modified_fields
            .iter()
            .filter(|field| doc.pending_op(field).is_none() && dependencies.contains(*field))
            .cloned()
            .collect();
        if compared.is_empty() && !modified_fields.is_empty() {
            return Ok(false);
        }
        let version = self.load_version(datastore, doc_short_id).await?;
        if version.as_deref() != Some(self.version_id()) {
            return Ok(false);
        }
        let id = doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("update requires a document ID".into()))?;
        let Some(committed) = self
            .get_with_datastore_fields_at(datastore, doc_short_id, id, &compared, version)
            .await?
        else {
            return Ok(false);
        };
        modified_fields
            .retain(|field| !compared.contains(field) || doc.get(field) != committed.get(field));
        Ok(modified_fields.is_empty())
    }
}
