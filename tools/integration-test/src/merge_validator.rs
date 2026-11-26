//! A merge validator for tests: it rejects a replicated document that sets
//! one field to one value, and accepts every other. A test node embeds it
//! through `IrohSpec::merge_validator`.

use async_trait::async_trait;
use db::merge::governance::{FieldValue, MergeCandidate, MergeValidator, MergeVerdict, MergeView};
use document::NormalValue;
use serde::{Deserialize, Serialize};

/// The field and the value a test node's merge validator rejects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectValue {
    pub field: String,
    pub value: String,
}

#[async_trait]
impl MergeValidator for RejectValue {
    /// The candidate is the composite being merged, so its fields are held
    /// here; failing to read them is a local failure, never a verdict.
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .ok_or_else(|| format!("the fields of {} could not be read", candidate.cid))?;
        let refused = FieldValue::Value(NormalValue::String(self.value.clone()));
        let sets_it = fields
            .iter()
            .any(|(name, value)| *name == self.field && *value == refused);
        Ok(match sets_it {
            true => MergeVerdict::reject(format!("{} may not be {}", self.field, self.value)),
            false => MergeVerdict::Accept,
        })
    }
}
