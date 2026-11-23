//! Downsample write validation.
//!
//! A write into a downsample source is checked against every plan that reads
//! the source: the time and measure must be valid, and a time must not fall in
//! a bucket the rollup already closed. An update decides on the stored time
//! and series of its document wherever it does not change them; those are
//! read here, validated at commit over exactly those parts, so a concurrent
//! write of any other field of the document does not conflict with it.

use super::parse::*;
use super::types::*;
use crate::collection::Collection;
use crate::error::{Error, Result};
use datastore::NamespaceView;
use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use storage::corekv::Store;

/// Whether a write changing `modified` (`None`: a create, every field) leaves
/// `plan` unchecked: a raw source is checked only when its time or measure
/// changes.
fn skips(plan: &DownsamplePlan, modified: Option<&RapidHashSet<String>>) -> bool {
    match (&plan.source_kind, modified) {
        (SourceKind::Raw { measure_field }, Some(fields)) => {
            !fields.contains(&plan.time_field) && !fields.contains(measure_field)
        }
        _ => false,
    }
}

/// The source fields a checked `plan` reads.
fn read_fields(plan: &DownsamplePlan) -> impl Iterator<Item = &str> {
    let measure = match &plan.source_kind {
        SourceKind::Raw { measure_field } => Some(measure_field.as_str()),
        SourceKind::Downsample => None,
    };
    [Some(plan.time_field.as_str()), measure, Some(SERIES_FIELD)]
        .into_iter()
        .flatten()
}

impl<S: Store + 'static> crate::database::DB<S> {
    pub async fn validate_downsample_write(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        source: &Collection,
        source_doc: &Document,
        modified_fields: Option<&RapidHashSet<String>>,
    ) -> Result<()> {
        let source_collection = source.schema();
        let plans = self.downsample_plans(None, Some(&source_collection.name))?;
        if plans.is_empty() {
            return Ok(());
        }
        let stored = match modified_fields {
            Some(modified) => {
                unchanged_decision_fields(
                    datastore,
                    systemstore,
                    source,
                    source_doc,
                    modified,
                    &plans,
                )
                .await?
            }
            None => None,
        };
        let source_doc = stored.as_ref().unwrap_or(source_doc);

        // The series identity is the source's own DocID, which is only
        // assigned once the genesis block is written. A source without one
        // yet is a first-time create: there is no prior rollup target, so
        // the closed-bucket (late-data) lookup below is skipped — but the
        // field-shape validation still runs.
        let series_doc_id = self.series_doc_id_opt(source_doc);

        for plan in plans {
            if skips(&plan, modified_fields) {
                continue;
            }
            if let SourceKind::Raw { measure_field } = &plan.source_kind {
                let time_changed =
                    modified_fields.is_some_and(|fields| fields.contains(&plan.time_field));
                let measure_changed =
                    modified_fields.is_some_and(|fields| fields.contains(measure_field));

                if time_changed != measure_changed {
                    return Err(Error::Other(format!(
                        "downsample source '{}': '{}' and '{}' must be updated together",
                        source_collection.name, plan.time_field, measure_field
                    )));
                }

                source_doc
                    .get(&plan.time_field)
                    .and_then(normal_value_to_time)
                    .ok_or_else(|| {
                        Error::Other(format!(
                            "downsample source '{}.{}' must contain a valid RFC3339 timestamp",
                            source_collection.name, plan.time_field
                        ))
                    })?;
                source_doc
                    .get(measure_field)
                    .and_then(normal_value_to_numeric)
                    .ok_or_else(|| {
                        Error::Other(format!(
                            "downsample source '{}.{}' must contain a numeric value",
                            source_collection.name, measure_field
                        ))
                    })?;
            }

            let source_time = source_doc
                .get(&plan.time_field)
                .and_then(normal_value_to_time)
                .ok_or_else(|| {
                    Error::Other(format!(
                        "downsample source '{}.{}' must contain a valid RFC3339 timestamp",
                        source_collection.name, plan.time_field
                    ))
                })?;
            let source_window_start_nanos = bucket_start_nanos(&source_time, plan.interval_nanos)?;

            // No series identity yet ⇒ first-time create ⇒ no prior rollup
            // bucket to conflict with; the field validation above still ran.
            let Some(series_doc_id) = series_doc_id.as_deref() else {
                continue;
            };

            let Some(target_doc) = self
                .find_downsample_target_in_txn(
                    datastore,
                    systemstore,
                    &plan.target.name,
                    series_doc_id,
                )
                .await?
            else {
                continue;
            };

            let current_window_start = target_doc
                .get("window_start")
                .and_then(normal_value_to_time)
                .ok_or_else(|| {
                    Error::Other(format!(
                        "downsample target '{}.window_start' is missing or invalid",
                        plan.target.name
                    ))
                })?;
            let current_window_start_nanos = timestamp_nanos(&current_window_start)?;

            if source_window_start_nanos <= current_window_start_nanos {
                return Err(Error::Other(format!(
                    "late data is not accepted for downsample source '{}': timestamp '{}' falls in closed bucket '{}'",
                    source_collection.name, source_time, current_window_start
                )));
            }
        }

        Ok(())
    }
}

/// The document as an update of `modified` decides on it: the caller's copy,
/// with every field a checked plan reads but the update does not change taken
/// from a read of the stored document validated over exactly those parts.
/// `None` when the checks read no stored field or the document is gone.
async fn unchanged_decision_fields(
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    source: &Collection,
    source_doc: &Document,
    modified: &RapidHashSet<String>,
    plans: &[DownsamplePlan],
) -> Result<Option<Document>> {
    let declared = |name: &str| {
        source
            .schema()
            .fields
            .iter()
            .any(|field| field.name == name)
    };
    let fields: RapidHashSet<String> = plans
        .iter()
        .filter(|plan| !skips(plan, Some(modified)))
        .flat_map(read_fields)
        .filter(|name| !modified.contains(*name) && declared(name))
        .map(str::to_owned)
        .collect();
    if fields.is_empty() {
        return Ok(None);
    }
    let Some(doc_id) = source_doc.id() else {
        return Ok(None);
    };
    let Some(short_id) = source.resolve_doc_short_id(systemstore, doc_id).await? else {
        return Ok(None);
    };
    let Some(stored) = source
        .get_with_datastore_fields(datastore, short_id, doc_id, &fields)
        .await?
    else {
        return Ok(None);
    };
    let mut decided = source_doc.clone();
    for name in &fields {
        decided.set_unchecked(
            name.clone(),
            stored.get(name).cloned().unwrap_or(NormalValue::Null),
        );
    }
    Ok(Some(decided))
}
