use serde::Serialize;

/// Go upstream commit we last synced with.
///
/// Past `c34052cb` (sourcenetwork/defradb#5253), which rewrote the SIMILARITY
/// argument shape and descriptions that the introspection tests expect, and
/// past `f73a903f`, the first baseline whose Go binary understands a vector
/// index at all. `v1.2.0` is the compatibility release.
pub const GO_COMPAT_COMMIT: &str = "e152b5cab";
/// Go upstream branch.
pub const GO_COMPAT_BRANCH: &str = "develop";
/// Go release tag; empty when CI should build the pinned commit from source.
pub const GO_COMPAT_TAG: &str = "v1.2.0";

/// Go commit carrying the rustffi test client, which the FFI oracle checks out.
///
/// Deliberately independent of `GO_COMPAT_COMMIT`: that pin selects the Go
/// binary the parity job measures against and tracks upstream drift, while this
/// one fixes the Go *test corpus* the FFI oracle runs, so its pass rate stays
/// comparable across baseline bumps. The commit, not a branch, names the
/// client: it moves only when we retarget the parity claim or the client
/// changes with the FFI. CI fails when the pinned commit does not resolve on
/// sourcenetwork/defradb.
pub const GO_FFI_CLIENT_COMMIT: &str = "4c25eaa073c6cfe4e6cae05d443195da57ee0e75";

/// Go compatibility metadata.
#[derive(Debug, Clone, Serialize)]
pub struct GoCompat {
    pub commit: &'static str,
    pub branch: &'static str,
    pub tag: &'static str,
}

/// Length Go DefraDB truncates the commit hash to in its version strings.
const COMMIT_HASH_LENGTH: usize = 8;

/// Complete version information for defradb.
///
/// The JSON carries every key Go DefraDB's `version --format json` prints except
/// `go`, in Go's order (`release`, `commit`, `commitDate`, `httpAPI`,
/// `docIDVersions`, `netProtocol`), and then defradb.rs's own.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub release: &'static str,
    pub commit: &'static str,
    pub commit_date: &'static str,
    #[serde(rename = "httpAPI")]
    pub http_api: &'static str,
    #[serde(rename = "docIDVersions")]
    pub doc_id_versions: &'static str,
    pub net_protocol: &'static str,
    /// `release` under the key defradb.rs printed before it printed Go's.
    pub version: &'static str,
    /// `docIDVersions` under the key defradb.rs printed before it printed Go's.
    #[serde(rename = "docIdVersions")]
    pub doc_id_versions_rs: &'static str,
    pub rust: String,
    pub go_compat: GoCompat,
}

impl VersionInfo {
    pub fn new() -> Self {
        let release = env!("CARGO_PKG_VERSION");
        let doc_id_versions = "1";
        Self {
            release,
            commit: env!("GIT_COMMIT"),
            commit_date: env!("BUILD_DATE"),
            http_api: "v1",
            doc_id_versions,
            net_protocol: "/defra/0.0.1",
            version: release,
            doc_id_versions_rs: doc_id_versions,
            rust: format!(
                "{} {}/{}",
                env!("RUSTC_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            ),
            go_compat: GoCompat {
                commit: GO_COMPAT_COMMIT,
                branch: GO_COMPAT_BRANCH,
                tag: GO_COMPAT_TAG,
            },
        }
    }

    fn commit_hash(&self) -> &'static str {
        let end = self
            .commit
            .char_indices()
            .nth(COMMIT_HASH_LENGTH)
            .map_or(self.commit.len(), |(index, _)| index);
        &self.commit[..end]
    }

    /// `defradb version`: `defradb <release> (<commit8> <date>) built with Rust <rustc> <os>/<arch>`,
    /// Go DefraDB's line with Rust in place of Go.
    pub fn short(&self) -> String {
        format!(
            "defradb {} ({} {}) built with Rust {}",
            self.release,
            self.commit_hash(),
            self.commit_date,
            self.rust
        )
    }

    /// `defradb version --full`: Go DefraDB's lines, `* Rust:` in place of
    /// `* Go:`, then the Go baseline this build is compatible with.
    pub fn full(&self) -> String {
        let tag_suffix = if self.go_compat.tag.is_empty() {
            String::new()
        } else {
            format!(", {}", self.go_compat.tag)
        };
        format!(
            "defradb {} ({} {})\n\
             * HTTP API: {}\n\
             * P2P multicodec: {}\n\
             * DocID versions: {}\n\
             * Rust: {}\n\
             * Commit: {}\n\
             * Go compat: {} ({}{})",
            self.release,
            self.commit_hash(),
            self.commit_date,
            self.http_api,
            self.net_protocol,
            self.doc_id_versions,
            self.rust,
            self.commit,
            self.go_compat.commit,
            self.go_compat.branch,
            tag_suffix
        )
    }
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_info_fields_are_populated() {
        let info = VersionInfo::new();
        assert!(!info.release.is_empty());
        assert!(!info.commit.is_empty());
        assert!(!info.rust.is_empty());
        assert_eq!(info.http_api, "v1");
        assert_eq!(info.doc_id_versions, "1");
        assert_eq!(info.net_protocol, "/defra/0.0.1");
        assert_eq!(info.version, info.release);
        assert_eq!(info.doc_id_versions_rs, info.doc_id_versions);
    }

    #[test]
    fn short_matches_go_shape() {
        let info = VersionInfo::new();
        let commit8: String = info.commit.chars().take(8).collect();
        assert_eq!(
            info.short(),
            format!(
                "defradb {} ({commit8} {}) built with Rust {}",
                info.release, info.commit_date, info.rust
            )
        );
    }

    #[test]
    fn full_opens_with_go_lines() {
        let info = VersionInfo::new();
        let full = info.full();
        let lines: Vec<&str> = full.lines().collect();
        let commit8: String = info.commit.chars().take(8).collect();
        assert_eq!(
            lines[0],
            format!("defradb {} ({commit8} {})", info.release, info.commit_date)
        );
        assert_eq!(lines[1], "* HTTP API: v1");
        assert_eq!(lines[2], "* P2P multicodec: /defra/0.0.1");
        assert_eq!(lines[3], "* DocID versions: 1");
        assert_eq!(lines[4], format!("* Rust: {}", info.rust));
        assert_eq!(lines[5], format!("* Commit: {}", info.commit));
        assert!(lines[6].starts_with(&format!("* Go compat: {GO_COMPAT_COMMIT} (")));
    }

    #[test]
    fn json_carries_go_keys_in_go_order_then_rust_keys() {
        let info = VersionInfo::new();
        let json = serde_json::to_string(&info).unwrap();
        let keys = [
            "\"release\"",
            "\"commit\"",
            "\"commitDate\"",
            "\"httpAPI\"",
            "\"docIDVersions\"",
            "\"netProtocol\"",
            "\"version\"",
            "\"docIdVersions\"",
            "\"rust\"",
            "\"goCompat\"",
        ];
        let positions: Vec<usize> = keys
            .iter()
            .map(|key| json.find(key).unwrap_or_else(|| panic!("{key} in {json}")))
            .collect();
        assert!(positions.is_sorted(), "{json}");
    }

    #[test]
    fn commit_hash_is_go_length() {
        let info = VersionInfo {
            commit: "e152b5cab0123456789",
            ..VersionInfo::new()
        };
        assert_eq!(info.commit_hash(), "e152b5ca");
        let short = VersionInfo {
            commit: "unknown",
            ..VersionInfo::new()
        };
        assert_eq!(short.commit_hash(), "unknown");
    }

    #[test]
    fn go_compat_constants_are_set() {
        assert!(!GO_COMPAT_COMMIT.is_empty());
        assert!(!GO_COMPAT_BRANCH.is_empty());
        assert!(!GO_FFI_CLIENT_COMMIT.is_empty());
    }
}
