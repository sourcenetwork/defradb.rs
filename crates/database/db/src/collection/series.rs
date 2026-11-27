//! A point dependency elects one live document for each downsample series.
use super::*;

impl Collection {
    pub(crate) fn series_key(&self, series: &str) -> Vec<u8> {
        format!("/downsample/series/{}/{}", self.resolved_root_id(), series).into_bytes()
    }

    pub(crate) async fn claim_series(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
    ) -> Result<()> {
        if self.def.downsample_source.is_none() {
            return Ok(());
        }
        let series = doc
            .get(crate::downsample::SERIES_FIELD)
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                Error::InvalidDocument("downsample target is missing source_doc_id".into())
            })?;
        let doc_id = doc
            .id()
            .ok_or_else(|| Error::InvalidDocument("Document must have an ID".into()))?
            .to_string();
        let key = self.series_key(series);
        match datastore.get(&key).await.map_err(Error::Storage)? {
            Some(existing) if existing.as_ref() != doc_id.as_bytes() => {
                Err(Error::Storage(storage::Error::UniqueConstraintViolation))
            }
            Some(_) => Ok(()),
            None => datastore
                .set(&key, doc_id.as_bytes())
                .await
                .map_err(Error::Storage),
        }
    }

    pub(crate) async fn release_series(
        &self,
        datastore: &NamespaceView,
        doc: &Document,
    ) -> Result<()> {
        if self.def.downsample_source.is_none() {
            return Ok(());
        }
        if let (Some(series), Some(doc_id)) = (
            doc.get(crate::downsample::SERIES_FIELD)
                .and_then(|value| value.as_str()),
            doc.id(),
        ) {
            let key = self.series_key(series);
            if datastore
                .get(&key)
                .await
                .map_err(Error::Storage)?
                .as_deref()
                == Some(doc_id.to_string().as_bytes())
            {
                datastore.delete(&key).await.map_err(Error::Storage)?;
            }
        }
        Ok(())
    }
}
