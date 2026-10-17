//! Downsample execution: per-document processing, persistence, and bootstrapping.

use super::parse::*;
use super::types::*;
use crate::block::builder::decode_priority_varint;
use crate::error::{Error, Result};
use chrono::Utc;
use defra_core::RequestContext;
use document::{DocID, Document, NormalValue};
use query::fetcher::CommitsQueryOptions;
use query::mutator::{DocMutator, MutationBatchController};
use query::runner::DocFetcher;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use schema::CollectionVersion;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use storage::corekv::{IterOptions, Store};
use storage::keys::headstore::HeadstoreDocKey;

impl<S: Store + 'static> crate::database::DB<S> {
    pub(super) async fn latest_doc_priority(&self, doc_id: &str) -> Result<u64> {
        let txn = self.new_txn(true).await?;
        let systemstore = txn.systemstore()?;
        let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, doc_id).await? else {
            let _ = txn.discard();
            return Ok(0);
        };

        let headstore = txn.headstore()?;
        let mut iter = crate::block::heads::document::iterator(
            &headstore,
            IterOptions::new().with_prefix(HeadstoreDocKey::document_prefix(doc_ref.doc_short_id)),
        )
        .await
        .map_err(Error::Storage)?;

        let mut max_priority = 0;
        while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
            max_priority = max_priority.max(decode_priority_varint(&pair.value));
        }
        iter.close().await.map_err(Error::Storage)?;

        let _ = txn.discard();
        Ok(max_priority)
    }

    pub(super) async fn load_source_documents(
        &self,
        collection: &CollectionVersion,
    ) -> Result<Vec<Document>> {
        let source_collection = self
            .get_collection(&collection.name)?
            .ok_or_else(|| Error::CollectionNotFound(collection.name.clone()))?;
        let txn = self.new_txn(true).await?;
        let datastore = txn.datastore()?;
        let systemstore = txn.systemstore()?;
        let result = source_collection
            .get_all_with_datastore(&datastore, &systemstore)
            .await;
        let _ = txn.discard();
        result
    }

    /// Find the downsample target document for a series by its
    /// `source_doc_id` field. Target DocIDs are genesis-derived (not
    /// seed-derived), so the series linkage field is the stable lookup key.
    pub(super) async fn find_downsample_target(
        self: &Arc<Self>,
        target_name: &str,
        series_doc_id: &str,
    ) -> Result<Option<Document>> {
        let txn = self.new_txn(true).await?;
        self.find_downsample_target_in_txn(
            &txn.datastore()?,
            &txn.systemstore()?,
            target_name,
            series_doc_id,
        )
        .await
    }

    /// Transaction-scoped variant of `find_downsample_target` for callers
    /// already holding datastore/systemstore views (write validation).
    pub(super) async fn find_downsample_target_in_txn(
        &self,
        datastore: &datastore::NamespaceView,
        systemstore: &datastore::NamespaceView,
        target_name: &str,
        series_doc_id: &str,
    ) -> Result<Option<Document>> {
        let Some(target_collection) = self.get_collection(target_name)? else {
            return Ok(None);
        };
        let Some(doc_id) = datastore
            .get(&target_collection.series_key(series_doc_id))
            .await
            .map_err(Error::Storage)?
        else {
            return Ok(None);
        };
        let doc_id =
            std::str::from_utf8(&doc_id).map_err(|error| Error::Other(error.to_string()))?;
        let doc_id = DocID::from_string(doc_id)?;
        target_collection
            .get_by_doc_id(datastore, systemstore, &doc_id)
            .await
    }

    pub(super) fn series_doc_id(&self, source_doc: &Document) -> Result<String> {
        self.series_doc_id_opt(source_doc)
            .ok_or_else(|| Error::Other("downsample source document is missing an id".to_string()))
    }

    /// Resolve the series identity of a downsample source, or `None` when
    /// the source has neither an explicit `source_doc_id` nor a DocID yet
    /// (a first-time create before its genesis identity is assigned).
    pub(super) fn series_doc_id_opt(&self, source_doc: &Document) -> Option<String> {
        source_doc
            .get(SERIES_FIELD)
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| source_doc.id().map(ToString::to_string))
    }

    fn set_field(
        &self,
        doc: &mut Document,
        modified_fields: &mut RapidHashSet<String>,
        field_name: &str,
        value: NormalValue,
        force_write: bool,
    ) {
        if force_write || doc.get(field_name) != Some(&value) {
            doc.set(field_name, value);
            modified_fields.insert(field_name.to_string());
        }
    }

    fn apply_window_to_target_doc(
        &self,
        plan: &DownsamplePlan,
        source_doc: &Document,
        doc: &mut Document,
        aggregate: &WindowAggregate,
        series_doc_id: &str,
    ) -> Result<RapidHashSet<String>> {
        let mut modified_fields = RapidHashSet::new();

        self.set_field(
            doc,
            &mut modified_fields,
            SERIES_FIELD,
            NormalValue::String(series_doc_id.to_string()),
            false,
        );
        self.set_field(
            doc,
            &mut modified_fields,
            "source_height",
            NormalValue::Int(aggregate.source_height),
            true,
        );
        self.set_field(
            doc,
            &mut modified_fields,
            "window_start",
            NormalValue::Time(aggregate.window_start),
            true,
        );
        self.set_field(
            doc,
            &mut modified_fields,
            "window_end",
            NormalValue::Time(aggregate.window_end),
            true,
        );

        for field_name in &plan.passthrough_fields {
            let value = source_doc.get(field_name).cloned().ok_or_else(|| {
                Error::Other(format!(
                    "downsample source '{}' is missing passthrough field '{}'",
                    plan.source.name, field_name
                ))
            })?;
            self.set_field(doc, &mut modified_fields, field_name, value, false);
        }

        for field in &plan.aggregate_fields {
            match field {
                AggregateField::Count => {
                    self.set_field(
                        doc,
                        &mut modified_fields,
                        "count",
                        NormalValue::Int(aggregate.count),
                        true,
                    );
                }
                AggregateField::Sum => {
                    let value = aggregate.sum.ok_or_else(|| {
                        Error::Other(format!(
                            "downsample window for '{}' is missing a sum value",
                            plan.target.name
                        ))
                    })?;
                    self.set_field(
                        doc,
                        &mut modified_fields,
                        "sum",
                        numeric_value_to_normal_value(&plan.target, "sum", value)?,
                        true,
                    );
                }
                AggregateField::Avg => {
                    let value = aggregate.avg.ok_or_else(|| {
                        Error::Other(format!(
                            "downsample window for '{}' is missing an avg value",
                            plan.target.name
                        ))
                    })?;
                    self.set_field(
                        doc,
                        &mut modified_fields,
                        "avg",
                        average_value_to_normal_value(&plan.target, "avg", value)?,
                        true,
                    );
                }
                AggregateField::Min => {
                    let value = aggregate.min.ok_or_else(|| {
                        Error::Other(format!(
                            "downsample window for '{}' is missing a min value",
                            plan.target.name
                        ))
                    })?;
                    self.set_field(
                        doc,
                        &mut modified_fields,
                        "min",
                        numeric_value_to_normal_value(&plan.target, "min", value)?,
                        true,
                    );
                }
                AggregateField::Max => {
                    let value = aggregate.max.ok_or_else(|| {
                        Error::Other(format!(
                            "downsample window for '{}' is missing a max value",
                            plan.target.name
                        ))
                    })?;
                    self.set_field(
                        doc,
                        &mut modified_fields,
                        "max",
                        numeric_value_to_normal_value(&plan.target, "max", value)?,
                        true,
                    );
                }
            }
        }

        Ok(modified_fields)
    }

    /// [`Self::persist_window_update_as`] in [`RequestContext::ambient`].
    #[cfg(test)]
    async fn persist_window_update(
        self: &Arc<Self>,
        plan: &DownsamplePlan,
        source_doc: &Document,
        series_doc_id: &str,
        target_doc_id: Option<&DocID>,
        aggregate: &WindowAggregate,
    ) -> Result<DocID> {
        self.persist_window_update_as(
            &RequestContext::ambient(),
            plan,
            source_doc,
            series_doc_id,
            target_doc_id,
            aggregate,
        )
        .await
    }

    /// Upsert one aggregation window into the target doc. Returns the target
    /// DocID so a create in the first window threads its derived identity to
    /// the remaining windows (the DocID only exists after the create).
    async fn persist_window_update_as(
        self: &Arc<Self>,
        context: &RequestContext,
        plan: &DownsamplePlan,
        source_doc: &Document,
        series_doc_id: &str,
        target_doc_id: Option<&DocID>,
        aggregate: &WindowAggregate,
    ) -> Result<DocID> {
        let target = match target_doc_id {
            Some(id) => Some(id.clone()),
            None => self.find_series_target_id(plan, series_doc_id).await?,
        };
        let created = Arc::new(AtomicBool::new(false));
        match self
            .persist_window(
                context,
                plan,
                source_doc,
                series_doc_id,
                target,
                aggregate,
                &created,
            )
            .await
        {
            // A create that lost the series to another processor's target
            // updates that target instead. Only a persisted winner justifies
            // reading a failed create as a lost race.
            Err(error) if created.load(Ordering::Acquire) => {
                let Some(winner) = self.find_series_target_id(plan, series_doc_id).await? else {
                    return Err(error);
                };
                self.persist_window(
                    context,
                    plan,
                    source_doc,
                    series_doc_id,
                    Some(winner),
                    aggregate,
                    &created,
                )
                .await
            }
            result => result,
        }
    }

    async fn find_series_target_id(
        self: &Arc<Self>,
        plan: &DownsamplePlan,
        series_doc_id: &str,
    ) -> Result<Option<DocID>> {
        Ok(self
            .find_downsample_target(&plan.target.name, series_doc_id)
            .await?
            .and_then(|doc| doc.id().cloned()))
    }

    /// Read the target, decide, and write the window in one transaction
    /// whose conflicts the engine re-runs: each re-run reads the target a
    /// competing processor left and decides again. `created` says whether the
    /// last attempt took the create branch.
    #[allow(clippy::too_many_arguments)]
    async fn persist_window(
        self: &Arc<Self>,
        context: &RequestContext,
        plan: &DownsamplePlan,
        source_doc: &Document,
        series_doc_id: &str,
        target: Option<DocID>,
        aggregate: &WindowAggregate,
        created: &Arc<AtomicBool>,
    ) -> Result<DocID> {
        let window = Arc::new(WindowWrite {
            db: Arc::clone(self),
            context: context.clone(),
            plan: plan.clone(),
            source_doc: source_doc.clone(),
            series_doc_id: series_doc_id.to_owned(),
            target,
            aggregate: aggregate.clone(),
            created: Arc::clone(created),
        });
        let operation: query::mutator::MutationBatchOperation =
            Arc::new(move |mutator, _fetcher| {
                let window = Arc::clone(&window);
                Box::pin(async move { window.write(mutator.as_ref()).await })
            });
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = crate::write::autocommit::AutoCommitMutator::new(Arc::clone(self))
            .with_prepared_inputs_value(inputs.clone());
        let controller = mutator.new_retryable_batch_controller();
        let written = loop {
            match controller.run(Arc::clone(&operation), None, None).await {
                Err(query::error::QueryError::NeedsInput(request)) => inputs
                    .answer_wave(&request, |wave| {
                        mutator.resolve_prepared_inputs_as(context, wave)
                    })
                    .await
                    .map_err(Error::Query)?,
                result => break result.map_err(Error::Query)?,
            }
        };
        let id = written.as_str().ok_or_else(|| {
            Error::Other("downsample window write returned no document ID".into())
        })?;
        Ok(DocID::from_string(id)?)
    }

    async fn process_source_doc_for_plan(
        self: &Arc<Self>,
        context: &RequestContext,
        plan: &DownsamplePlan,
        source_doc: &Document,
        complete_through: chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<()> {
        let Some(source_doc_id) = source_doc.id() else {
            return Ok(());
        };
        let source_doc_id = source_doc_id.to_string();
        let latest_source_priority = self.latest_doc_priority(&source_doc_id).await?;
        if latest_source_priority == 0 {
            return Ok(());
        }

        let series_doc_id = self.series_doc_id(source_doc)?;

        let current_target = self
            .find_downsample_target(&plan.target.name, &series_doc_id)
            .await?;
        let mut target_doc_id = current_target.as_ref().and_then(|doc| doc.id().cloned());
        let processed_height = current_target
            .as_ref()
            .and_then(|doc| doc.get("source_height"))
            .and_then(|value| value.as_int())
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let current_window_start = current_target
            .as_ref()
            .and_then(|doc| doc.get("window_start"))
            .and_then(normal_value_to_time);

        if processed_height >= latest_source_priority {
            return Ok(());
        }

        let fetcher = crate::LensedAutoCommitFetcher::new(self.clone());
        let commits = fetcher
            .get_commits(&CommitsQueryOptions {
                doc_id: Some(source_doc_id.clone()),
                height_start: Some(processed_height + 1),
                height_end: Some(latest_source_priority + 1),
                ..Default::default()
            })
            .await
            .map_err(Error::Query)?;

        let samples = self.build_source_samples(plan, commits)?;
        if samples.is_empty() {
            return Ok(());
        }

        let windows = self.aggregate_samples_into_windows(plan, samples, complete_through)?;
        if windows.is_empty() {
            return Ok(());
        }

        if let Some(current_window_start) = current_window_start {
            let current_window_start_nanos = timestamp_nanos(&current_window_start)?;
            let first_window_start_nanos = timestamp_nanos(&windows[0].window_start)?;
            if first_window_start_nanos <= current_window_start_nanos {
                return Err(Error::Other(format!(
                    "downsample source '{}' produced out-of-order timestamps for series '{}'; late data for already-closed windows is not yet supported",
                    plan.source.name, series_doc_id
                )));
            }
        }

        for aggregate in windows {
            let persisted_id = self
                .persist_window_update_as(
                    context,
                    plan,
                    source_doc,
                    &series_doc_id,
                    target_doc_id.as_ref(),
                    &aggregate,
                )
                .await?;
            target_doc_id = Some(persisted_id);
        }

        Ok(())
    }

    async fn bootstrap_downsample_plan(
        self: &Arc<Self>,
        context: &RequestContext,
        plan: &DownsamplePlan,
    ) -> Result<()> {
        let now = Utc::now().fixed_offset();
        for source_doc in self.load_source_documents(&plan.source).await? {
            self.process_source_doc_for_plan(context, plan, &source_doc, now)
                .await?;
        }
        Ok(())
    }

    /// [`Self::bootstrap_downsamples_as`] in [`RequestContext::ambient`].
    pub async fn bootstrap_downsamples(self: &Arc<Self>, names: Option<&[String]>) -> Result<()> {
        self.bootstrap_downsamples_as(&RequestContext::ambient(), names)
            .await
    }

    /// Downsample every source document of the plans `names` selects (all of
    /// them for `None`), writing the windows in `context`.
    pub async fn bootstrap_downsamples_as(
        self: &Arc<Self>,
        context: &RequestContext,
        names: Option<&[String]>,
    ) -> Result<()> {
        let name_set = names.map(|names| names.iter().cloned().collect::<RapidHashSet<_>>());
        let mut plans = self.downsample_plans(name_set.as_ref(), None)?;

        let mut memo = RapidHashMap::new();
        let mut visiting = RapidHashSet::new();
        plans.sort_by_key(|plan| {
            self.downsample_depth(&plan.target.name, &mut memo, &mut visiting)
                .unwrap_or(usize::MAX)
        });

        for plan in plans {
            self.bootstrap_downsample_plan(context, &plan).await?;
        }

        Ok(())
    }

    // Only the downsample task drives this, and that task is native-only.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) async fn process_downsample_update(
        self: &Arc<Self>,
        collection_id: &str,
        doc_id: &str,
    ) -> Result<()> {
        let collection = self
            .find_collection_by_id(collection_id)?
            .ok_or_else(|| Error::CollectionNotFound(collection_id.to_string()))?;

        let plans = self.downsample_plans(None, Some(collection.name()))?;
        if plans.is_empty() {
            return Ok(());
        }

        let doc_id = DocID::from_string(doc_id)
            .map_err(|e| Error::Other(format!("invalid doc_id '{}': {}", doc_id, e)))?;
        let fetcher = crate::write::autocommit::AutoCommitMutator::new(self.clone());
        let source_doc = match fetcher
            .get_for_update(collection.name(), &doc_id)
            .await
            .map_err(Error::Query)?
        {
            Some(doc) => doc,
            None => {
                tracing::debug!(
                    collection = %collection.name(),
                    doc_id = %doc_id,
                    "Source document not found for downsample update"
                );
                return Ok(());
            }
        };

        // A source write downsamples in the background, for no request.
        let context = RequestContext::default();
        let now = Utc::now().fixed_offset();
        for plan in plans {
            self.process_source_doc_for_plan(&context, &plan, &source_doc, now)
                .await?;
        }

        Ok(())
    }
}

/// One window persisted by one engine attempt: the target's current state
/// is read in the attempt and the window decided against it.
struct WindowWrite<S>
where
    S: Store,
{
    db: Arc<crate::database::DB<S>>,
    context: RequestContext,
    plan: DownsamplePlan,
    source_doc: Document,
    series_doc_id: String,
    target: Option<DocID>,
    aggregate: WindowAggregate,
    created: Arc<AtomicBool>,
}

impl<S> WindowWrite<S>
where
    S: Store + 'static,
{
    async fn write(&self, mutator: &dyn DocMutator) -> query::error::Result<serde_json::Value> {
        let decide = |error: Error| query::error::QueryError::execution(error.to_string());
        self.created.store(false, Ordering::Release);
        let target_name = &self.plan.target.name;
        let existing = match &self.target {
            Some(id) => mutator.get_for_update(target_name, id).await?,
            None => None,
        };
        let Some(mut doc) = existing else {
            self.created.store(true, Ordering::Release);
            let mut doc = Document::new();
            doc.set_collection(self.plan.target.clone());
            doc.set_schema_version_id(self.plan.target.version_id.clone());
            self.db
                .apply_window_to_target_doc(
                    &self.plan,
                    &self.source_doc,
                    &mut doc,
                    &self.aggregate,
                    &self.series_doc_id,
                )
                .map_err(decide)?;
            let created = mutator.create_as(&self.context, target_name, doc).await?;
            return Ok(serde_json::Value::String(created.doc_id.to_string()));
        };
        let id = doc.id().cloned().ok_or_else(|| {
            query::error::QueryError::execution("downsample target has no document ID")
        })?;
        let done = Ok(serde_json::Value::String(id.to_string()));
        // A processor that already wrote this window, or a later one, wins.
        if doc
            .get("source_height")
            .and_then(NormalValue::as_int)
            .is_some_and(|height| height >= self.aggregate.source_height)
        {
            return done;
        }
        let modified_fields = self
            .db
            .apply_window_to_target_doc(
                &self.plan,
                &self.source_doc,
                &mut doc,
                &self.aggregate,
                &self.series_doc_id,
            )
            .map_err(decide)?;
        if modified_fields.is_empty() {
            return done;
        }
        doc.set_collection(self.plan.target.clone());
        doc.set_schema_version_id(self.plan.target.version_id.clone());
        doc.set_id(id);
        mutator
            .update_as(&self.context, target_name, doc, modified_fields)
            .await?;
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{FieldDescription, FieldKind};
    use storage::RegolithStore;

    #[test]
    fn concurrent_window_updates_keep_the_highest_source_height() {
        workers::block_on(async {
            let db = Arc::new(crate::DB::new(RegolithStore::in_memory().unwrap()).unwrap());
            // Exercise persistence directly so background scheduling cannot hide a
            // stale processor behind a process-local series lock.
            let target = CollectionVersion::new(
                "Window",
                "window-v1",
                "window",
                vec![
                    FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                    FieldDescription::new("2", "source_doc_id", FieldKind::string()),
                    FieldDescription::new("3", "source_height", FieldKind::int()),
                    FieldDescription::new("4", "window_start", FieldKind::datetime()),
                    FieldDescription::new("5", "window_end", FieldKind::datetime()),
                    FieldDescription::new("6", "count", FieldKind::int()),
                ],
            );
            db.create_collection(target).await.unwrap();
            let target = db
                .get_collection("Window")
                .unwrap()
                .unwrap()
                .schema()
                .clone();
            let plan = DownsamplePlan {
                source: target.clone(),
                target,
                interval_raw: "1s".into(),
                interval_nanos: 1_000_000_000,
                time_field: "ts".into(),
                retention_nanos: None,
                passthrough_fields: vec![],
                aggregate_fields: vec![AggregateField::Count],
                source_kind: SourceKind::Raw {
                    measure_field: "value".into(),
                },
            };
            let source = Document::new();
            let window = |height| WindowAggregate {
                count: height,
                sum: None,
                avg: None,
                min: None,
                max: None,
                window_start: chrono::DateTime::from_timestamp(height, 0)
                    .unwrap()
                    .fixed_offset(),
                window_end: chrono::DateTime::from_timestamp(height + 1, 0)
                    .unwrap()
                    .fixed_offset(),
                source_height: height,
            };
            let id = db
                .persist_window_update(&plan, &source, "series", None, &window(1))
                .await
                .unwrap();
            let high = window(10);
            let middle = window(9);
            let low = window(8);
            let (a, b, c) = futures::join!(
                db.persist_window_update(&plan, &source, "series", Some(&id), &high),
                db.persist_window_update(&plan, &source, "series", Some(&id), &middle),
                db.persist_window_update(&plan, &source, "series", Some(&id), &low),
            );
            for result in [a, b, c] {
                assert_eq!(result.unwrap(), id);
            }
            // A delayed processor must also be harmless after all races finish.
            db.persist_window_update(&plan, &source, "series", Some(&id), &window(2))
                .await
                .unwrap();
            let actual = crate::AutoCommitMutator::new(db)
                .get_for_update("Window", &id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(actual.get("source_height"), Some(&NormalValue::Int(10)));
            assert_eq!(actual.get("count"), Some(&NormalValue::Int(10)));
            assert_eq!(
                actual.get("window_start").and_then(normal_value_to_time),
                Some(high.window_start)
            );
        })
        .unwrap()
    }
}
