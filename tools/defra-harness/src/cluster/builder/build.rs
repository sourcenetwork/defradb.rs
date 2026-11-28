//! Starting the cluster a builder describes.

use super::*;

impl TestClusterBuilder {
    pub async fn build(mut self) -> Result<TestCluster> {
        let total = self.rust_nodes + self.go_nodes;
        crate::error::ensure!(total > 0, "must have at least one node");

        let is_iroh = self.p2p_transport.as_deref() == Some("iroh");

        // Resolve Rust binary source
        let rust_binary_path = if self.rust_nodes > 0 {
            let source = self.rust_binary.clone().unwrap_or_else(|| {
                if is_iroh {
                    std::env::var("DEFRA_IROH_BINARY")
                        .ok()
                        .map(|p| BinarySource::Path(PathBuf::from(p)))
                        .unwrap_or_else(|| {
                            BinarySource::WorkspaceWithFeatures(vec!["iroh".to_string()])
                        })
                } else {
                    std::env::var("DEFRA_RUST_BINARY")
                        .ok()
                        .map(|p| BinarySource::Path(PathBuf::from(p)))
                        .unwrap_or(BinarySource::Workspace)
                }
            });
            let path = match &source {
                BinarySource::Workspace => {
                    RustNode::build().wrap_err("failed to build Rust binary")?;
                    RustNode::workspace_binary_path()
                }
                BinarySource::WorkspaceWithFeatures(features) => {
                    let refs: Vec<&str> = features.iter().map(|s| s.as_str()).collect();
                    RustNode::build_with_features(&refs)
                        .wrap_err("failed to build Rust binary with features")?;
                    RustNode::workspace_binary_path_with_features(&refs)
                }
                _ => source.resolve(NodeKind::Rust)?,
            };
            Some(path)
        } else {
            None
        };

        // Resolve Go binary source
        let go_binary_path = if self.go_nodes > 0 {
            let source = self.go_binary.clone().unwrap_or(BinarySource::Workspace);
            let path = match &source {
                BinarySource::Workspace => {
                    // Default Go behavior: PATH lookup + version check
                    GoNode::check_available().wrap_err("Go defradb binary not available")?;
                    GoNode::path_binary()
                }
                _ => source.resolve(NodeKind::Go)?,
            };
            Some(path)
        } else {
            None
        };

        // Vera or NAC requires an identity at startup.
        if (self.nac_enabled || self.vera.is_some()) && self.node_identity.is_none() {
            let binary = if let Some(ref p) = go_binary_path {
                p.clone()
            } else if let Some(ref p) = rust_binary_path {
                p.clone()
            } else {
                crate::error::bail!("no binary available for identity generation");
            };
            let id = crate::identity::generate_identity(&binary)
                .wrap_err("auto-generating identity for NAC/Vera")?;
            self.node_identity = Some(id.private_key_hex);
        }

        // Apply DEFRA_MULTIPLIERS=signed-docs unless this builder opted out.
        // Idempotent: no-op if signing_enabled is already true.
        if signed_docs_multiplier_active() && !self.signing_multiplier_opt_out {
            self.signing_enabled = true;
        }

        // Allocate ports for all nodes
        let mut all_ports = allocate_node_ports(total)?;

        // Create run directory
        let run_dir = crate::infra::TestRunDir::new(
            &crate::workspace_root().join("target/e2e"),
            "DEFRA_E2E_KEEP",
        )?;

        let vera_config = self.vera.clone();

        let mut nodes = Vec::with_capacity(total);

        // Spawn Rust nodes
        for (i, ports) in all_ports.iter_mut().enumerate().take(self.rust_nodes) {
            let name = format!("rust-{}", i);
            let node = RustNode::from_binary(rust_binary_path.as_ref().unwrap());
            let identity = self
                .node_identities
                .get(i)
                .cloned()
                .flatten()
                .or_else(|| self.node_identity.clone());

            let node_dir = run_dir.node_dir(&name)?;
            let log_dir = node_dir.join("logs");
            let rootdir = node_dir.join("data");

            // A cluster-shared SE key needs a File keyring both runtimes can
            // share; override `--no-keyring`/Env with a per-node File backend.
            let keyring = if self.shared_se_key.is_some() || self.file_keyring {
                KeyringBackend::File {
                    path: rootdir.join("keys"),
                    secret: "integration-test-secret".to_string(),
                }
            } else {
                self.keyring.clone()
            };

            // Addr fields are patched per attempt: a port-conflict retry
            // re-allocates this node's ports.
            let config_template = NodeConfig {
                name: name.clone(),
                rootdir,
                log_dir,
                http_addr: String::new(),
                p2p_enabled: self.p2p_enabled,
                p2p_addr: None,
                peers: vec![],
                identity,
                acp_document_type: self.acp_document_type.clone(),
                encryption_enabled: self.encryption_enabled,
                signing_enabled: self.signing_enabled,
                nac_enabled: self.nac_enabled,
                vera: vera_config.clone(),
                orbis_signer: None,
                keyring,
                development: self.development,
                store: self
                    .node_stores
                    .get(i)
                    .cloned()
                    .flatten()
                    .or_else(|| self.store.clone()),
                query_timeout: self.query_timeout,
                p2p_transport: self.p2p_transport.clone(),
                shared_se_key: self.shared_se_key,
                acp_cache_ttl: self.acp_cache_ttl,
                acp_circuit_breaker_threshold: self.acp_circuit_breaker_threshold,
                acp_circuit_breaker_reset_timeout: self.acp_circuit_breaker_reset_timeout,
                acp_request_timeout: self.acp_request_timeout,
                acp_receipt_timeout: self.acp_receipt_timeout,
                extra_args: self.rust_args_for(i),
            };

            let mut attempt = 1;
            let running = loop {
                let mut config = config_template.clone();
                config.http_addr = format!("127.0.0.1:{}", ports.http);
                config.p2p_addr = if self.p2p_enabled && !is_iroh {
                    Some(format!("/ip4/127.0.0.1/tcp/{}", ports.p2p))
                } else {
                    None
                };
                // Release port guards right before spawn so the child can bind
                ports.release();
                match start_node(&node, config, self.health_timeout).await {
                    Ok(r) => break r,
                    Err(e) if attempt < PORT_CONFLICT_ATTEMPTS && e.is_port_conflict() => {
                        attempt += 1;
                        tracing::warn!(
                            "{}: port stolen in guard-release window; retrying with fresh ports \
                             (attempt {}/{})",
                            name,
                            attempt,
                            PORT_CONFLICT_ATTEMPTS
                        );
                        let _ = std::fs::remove_dir_all(&config_template.rootdir);
                        *ports = allocate_node_ports(1)?
                            .pop()
                            .expect("allocate_node_ports(1) returns one entry");
                    }
                    Err(e) => return Err(e).wrap_err_with(|| format!("failed to start {}", name)),
                }
            };
            nodes.push(running);
        }

        // Spawn Go nodes
        for (i, ports) in all_ports.iter_mut().skip(self.rust_nodes).enumerate() {
            let name = format!("go-{}", i);
            let node = GoNode::from_binary(go_binary_path.as_ref().unwrap());
            let go_index = self.rust_nodes + i;
            let identity = self
                .node_identities
                .get(go_index)
                .cloned()
                .flatten()
                .or_else(|| self.node_identity.clone());

            let node_dir = run_dir.node_dir(&name)?;
            let log_dir = node_dir.join("logs");
            let rootdir = node_dir.join("data");

            // A cluster-shared SE key needs a File keyring; otherwise Go runs
            // with its usual `--no-keyring`.
            let keyring = if self.shared_se_key.is_some() || self.file_keyring {
                KeyringBackend::File {
                    path: rootdir.join("keys"),
                    secret: "integration-test-secret".to_string(),
                }
            } else {
                KeyringBackend::None
            };

            // Addr fields are patched per attempt: a port-conflict retry
            // re-allocates this node's ports.
            let config_template = NodeConfig {
                name: name.clone(),
                rootdir,
                log_dir,
                http_addr: String::new(),
                p2p_enabled: self.p2p_enabled,
                p2p_addr: None,
                peers: vec![],
                identity,
                acp_document_type: self.acp_document_type.clone(),
                encryption_enabled: self.encryption_enabled,
                signing_enabled: self.signing_enabled,
                nac_enabled: self.nac_enabled,
                vera: vera_config.clone(),
                orbis_signer: None,
                keyring,
                development: self.development,
                store: self
                    .node_stores
                    .get(go_index)
                    .cloned()
                    .flatten()
                    .or_else(|| self.store.clone()),
                query_timeout: self.query_timeout,
                p2p_transport: None,
                shared_se_key: self.shared_se_key,
                acp_cache_ttl: self.acp_cache_ttl,
                acp_circuit_breaker_threshold: self.acp_circuit_breaker_threshold,
                acp_circuit_breaker_reset_timeout: self.acp_circuit_breaker_reset_timeout,
                acp_request_timeout: self.acp_request_timeout,
                acp_receipt_timeout: self.acp_receipt_timeout,
                extra_args: self.extra_go_args.clone(),
            };

            let mut attempt = 1;
            let running = loop {
                let mut config = config_template.clone();
                config.http_addr = format!("127.0.0.1:{}", ports.http);
                config.p2p_addr = if self.p2p_enabled {
                    Some(format!("/ip4/127.0.0.1/tcp/{}", ports.p2p))
                } else {
                    None
                };
                // Release port guards right before spawn so the child can bind
                ports.release();
                match start_node(&node, config, self.health_timeout).await {
                    Ok(r) => break r,
                    Err(e) if attempt < PORT_CONFLICT_ATTEMPTS && e.is_port_conflict() => {
                        attempt += 1;
                        tracing::warn!(
                            "{}: port stolen in guard-release window; retrying with fresh ports \
                             (attempt {}/{})",
                            name,
                            attempt,
                            PORT_CONFLICT_ATTEMPTS
                        );
                        let _ = std::fs::remove_dir_all(&config_template.rootdir);
                        *ports = allocate_node_ports(1)?
                            .pop()
                            .expect("allocate_node_ports(1) returns one entry");
                    }
                    Err(e) => return Err(e).wrap_err_with(|| format!("failed to start {}", name)),
                }
            };
            nodes.push(running);
        }

        // Confirm all nodes are healthy via HTTP
        let client = Client::new();
        let urls: Vec<String> = nodes.iter().map(|n| n.api_url.clone()).collect();
        health_check_all(&client, &urls, self.health_timeout)
            .await
            .wrap_err("health check failed")?;

        // Collect effective per-node identities for test assertions
        let effective_identities: Vec<Option<String>> = (0..total)
            .map(|i| {
                self.node_identities
                    .get(i)
                    .cloned()
                    .flatten()
                    .or_else(|| self.node_identity.clone())
            })
            .collect();

        Ok(TestCluster::new(
            nodes,
            run_dir,
            self.node_identity,
            effective_identities,
        ))
    }
}
