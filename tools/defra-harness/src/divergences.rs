/// Which DefraDB implementation a node is running.
///
/// Threading this through the test harness makes every Go/Rust behavioral
/// difference explicit at the call site rather than hidden behind `or_else()`
/// fallback chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Rust,
    Go,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::Rust => write!(f, "Rust"),
            NodeKind::Go => write!(f, "Go"),
        }
    }
}

// ---- Divergence catalog ----
//
// Every known behavioral difference between Go and Rust DefraDB is listed
// here, grouped by category. When a divergence is resolved (Go and Rust
// converge), the function is collapsed to return a single shared value.
//
// Remaining divergences are an at-a-glance measure of how far apart the
// implementations are.

// -- Start command flags (CONVERGED) --

/// ACP document type flag name. Both use `--document-acp-type`.
pub fn acp_document_type_flag(_kind: NodeKind) -> &'static str {
    "--document-acp-type"
}

/// NAC enable flag name. Both use `--node-acp-enable`.
pub fn nac_enable_flag(_kind: NodeKind) -> &'static str {
    "--node-acp-enable"
}

// -- CLI subcommand syntax (CONVERGED) --

/// Collection doc-ids subcommand name. Both use `docIDs`.
pub fn doc_ids_subcommand(_kind: NodeKind) -> &'static str {
    "docIDs"
}

/// Both Go and Rust now use `--collection`/`--name` flags for index commands.
pub fn index_uses_positional_args(_kind: NodeKind) -> bool {
    false
}

/// Both Go and Rust now use `--collection`/`--field` flags for encrypted-index commands.
pub fn encrypted_index_uses_positional_args(_kind: NodeKind) -> bool {
    false
}

// -- Output format (CONVERGED) --

/// Both Go and Rust wrap query output in `{"data": ...}`.
/// Go also prefixes with a "Request Results" header line.
pub fn query_wraps_in_data(_kind: NodeKind) -> bool {
    true
}

/// Both Go and Rust output line-separated `{"docID": "..."}` objects.
pub fn doc_id_key(_kind: NodeKind) -> &'static str {
    "docID"
}

// -- Log patterns (CONVERGED) --

/// The log line substring that indicates the HTTP server is ready.
pub fn ready_log_pattern(_kind: NodeKind) -> &'static str {
    "Providing HTTP API at"
}

/// Peer-connected log pattern regex. Both use "Peer connected".
pub fn peer_connected_pattern(_kind: NodeKind) -> &'static str {
    r"Peer connected|PeerConnect"
}

/// P2P listening log pattern regex. Both use "Created LibP2P host".
pub fn p2p_listening_pattern(_kind: NodeKind) -> &'static str {
    r"Created LibP2P host"
}

// -- Vera support --

/// Whether the node supports `--vera-*` start flags.
/// Rust: yes, Go: not yet
pub fn supports_vera_flags(kind: NodeKind) -> bool {
    match kind {
        NodeKind::Rust => true,
        NodeKind::Go => false,
    }
}

// -- Transport support --

/// Whether the node supports `--p2p-transport` flag.
/// Rust: yes (libp2p + iroh), Go: no (libp2p only)
pub fn supports_p2p_transport_flag(kind: NodeKind) -> bool {
    match kind {
        NodeKind::Rust => true,
        NodeKind::Go => false,
    }
}
