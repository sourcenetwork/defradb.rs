//! Mirrors of `defra-version`'s shapes.

use serde::Serialize;

/// [`defra_version::GoCompat`]: the Go baseline this build tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct GoCompat {
    pub commit: String,
    pub branch: String,
    pub tag: String,
}

/// [`defra_version::VersionInfo`]: what `GET /version` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub commit_date: String,
    #[serde(rename = "httpAPI")]
    pub http_api: String,
    pub doc_id_versions: String,
    pub net_protocol: String,
    pub rust: String,
    pub go_compat: GoCompat,
}

impl From<defra_version::VersionInfo> for VersionInfo {
    fn from(info: defra_version::VersionInfo) -> Self {
        Self {
            version: info.version.to_string(),
            commit: info.commit.to_string(),
            commit_date: info.commit_date.to_string(),
            http_api: info.http_api.to_string(),
            doc_id_versions: info.doc_id_versions.to_string(),
            net_protocol: info.net_protocol.to_string(),
            rust: info.rust,
            go_compat: GoCompat {
                commit: info.go_compat.commit.to_string(),
                branch: info.go_compat.branch.to_string(),
                tag: info.go_compat.tag.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror serializes exactly as the type it stands for, so `GET
    /// /version` is unchanged by the document naming the mirror.
    #[test]
    fn version_info_matches_its_source() {
        let source = defra_version::VersionInfo::new();
        assert_eq!(
            serde_json::to_value(VersionInfo::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
