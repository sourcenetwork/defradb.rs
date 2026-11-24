//! `lens`'s migration configuration, as `POST /lens` and the collection patch
//! accept it.

use serde::{Deserialize, Serialize};

/// [`lens::LensModule`]: one WASM transform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LensModule {
    #[serde(
        rename = "Path",
        alias = "path",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub path: Option<String>,
    #[serde(rename = "Inverse", alias = "inverse", default)]
    pub inverse: bool,
    /// Raw WASM bytes, as bytes rather than the source's `serde_bytes` view; a
    /// caller sends a path or these, and the schema admits both.
    #[serde(rename = "Module", skip_serializing_if = "Option::is_none", default)]
    pub module: Option<Vec<u8>>,
    #[serde(
        rename = "Arguments",
        alias = "arguments",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub arguments: Option<serde_json::Value>,
}

/// [`lens::LensConfig`]: which version migrates to which, through what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct LensConfig {
    /// The source spelling carries an alias (`SourceSchemaVersionID`) for the
    /// pre-rename field; the document publishes the current name.
    #[serde(
        rename = "SourceCollectionVersionID",
        alias = "SourceSchemaVersionID",
        default
    )]
    pub source_collection_version_id: String,
    #[serde(rename = "DestinationCollectionVersionID", default)]
    pub destination_collection_version_id: String,
    #[serde(rename = "Lenses", alias = "Lens", alias = "lenses", default)]
    pub lenses: Vec<LensModule>,
}

impl From<LensConfig> for lens::LensConfig {
    fn from(config: LensConfig) -> Self {
        Self {
            source_collection_version_id: config.source_collection_version_id,
            destination_collection_version_id: config.destination_collection_version_id,
            lenses: config
                .lenses
                .into_iter()
                .map(|module| lens::LensModule {
                    path: module.path,
                    inverse: module.inverse,
                    module: module.module,
                    arguments: module.arguments,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror accepts the same JSON the source does and converts to it
    /// without changing the wire form.
    #[test]
    fn lens_config_matches_its_source() {
        let json = serde_json::json!({
            "SourceCollectionVersionID": "bafkreia",
            "DestinationCollectionVersionID": "bafkreib",
            "Lenses": [{ "Path": "wasm/lens.wasm", "Inverse": false }],
        });
        let mirror: LensConfig = serde_json::from_value(json.clone()).unwrap();
        let source: lens::LensConfig = serde_json::from_value(json).unwrap();
        let converted: lens::LensConfig = mirror.into();
        assert_eq!(
            serde_json::to_value(converted).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
