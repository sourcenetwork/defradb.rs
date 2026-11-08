use std::ffi::{c_char, CStr};
#[cfg(feature = "iroh")]
use std::path::PathBuf;

use crate::ffi_entry;
use crate::node::build_node_state;
use crate::state::NODES;
use crate::types::{c_str_to_string, NewNodeResult, NodeInitOptions};

/// Create a new DefraDB node with P2P enabled.
///
/// # Safety
///
/// Any non-null C string pointers inside `options` and `listen_addr` must be valid
/// null-terminated UTF-8 strings for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn new_node_with_p2p(
    options: NodeInitOptions,
    listen_addr: *const c_char,
) -> NewNodeResult {
    ffi_entry! {
        let rt = match crate::runtime::edge() {
            Some(rt) => rt,
            None => return NewNodeResult::error(crate::helpers::ERR_RUNTIME_NOT_INITIALIZED),
        };

        let listen_addr_str = c_str_to_string(listen_addr).unwrap_or_default();
        let transport = match resolve_transport(&options, &listen_addr_str) {
            Ok(transport) => transport,
            Err(error) => return NewNodeResult::error(error),
        };

        let result = crate::helpers::block_on(rt, build_node_state(options, transport, rt.clone()))
            .map(|state| NODES.insert(state));

        match result {
            Ok(handle) => NewNodeResult::success(handle),
            Err(error) => NewNodeResult::error(error),
        }
    }
}

/// The libp2p host `options` describe: its listen addresses (the JSON list,
/// else `listen_addr`), bootstrap peers, GossipSub and relay switches, and
/// resource limits.
#[cfg(feature = "libp2p")]
fn libp2p_config(
    options: &NodeInitOptions,
    listen_addr: &str,
) -> Result<defra_node::Libp2pConfig, String> {
    let list = |raw: *const c_char, name: &str| -> Result<Option<Vec<String>>, String> {
        if raw.is_null() {
            return Ok(None);
        }
        // SAFETY: the caller promised `options`' strings are null or valid.
        let json = unsafe { CStr::from_ptr(raw) }
            .to_str()
            .map_err(|_| format!("{name} is not valid UTF-8"))?;
        if json.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(json)
            .map(Some)
            .map_err(|error| format!("invalid {name}: {error}"))
    };
    let listen_addrs = match list(options.p2p_listen_addrs_json, "p2p_listen_addrs_json")? {
        Some(addrs) => addrs,
        None if !listen_addr.is_empty() => vec![listen_addr.to_string()],
        None => return Err("listen_addr is required for libp2p transport".to_string()),
    };
    let nonzero = |value: u64| (value > 0).then_some(value);
    Ok(defra_node::Libp2pConfig {
        listen_addrs,
        bootstrap_peers: list(options.p2p_bootstrap_peers_json, "p2p_bootstrap_peers_json")?
            .unwrap_or_default(),
        enable_pubsub: options.p2p_disable_pubsub == 0,
        enable_relay: options.p2p_disable_relay == 0,
        resource_limits: p2p::ResourceLimits {
            memory_mib: nonzero(options.p2p_resource_memory_mib),
            file_descriptors: nonzero(options.p2p_resource_file_descriptors),
            max_streams_per_peer: (options.p2p_max_streams_per_peer > 0)
                .then_some(options.p2p_max_streams_per_peer),
        },
        ..Default::default()
    })
}

#[cfg_attr(
    not(any(feature = "libp2p", feature = "iroh")),
    allow(unused_variables)
)]
fn resolve_transport(
    options: &NodeInitOptions,
    listen_addr: &str,
) -> Result<defra_node::TransportConfig, String> {
    let transport = unsafe { c_str_to_string(options.p2p_transport) }
        .unwrap_or_else(|| "libp2p".to_string())
        .to_lowercase();

    match transport.as_str() {
        "" | "libp2p" => {
            #[cfg(feature = "libp2p")]
            {
                Ok(defra_node::TransportConfig::Libp2p(libp2p_config(
                    options,
                    listen_addr,
                )?))
            }
            #[cfg(not(feature = "libp2p"))]
            {
                Err(
                    "this build does not include the libp2p transport; rebuild with the libp2p feature"
                        .to_string(),
                )
            }
        }
        "iroh" => {
            #[cfg(feature = "iroh")]
            {
                let mut config = defra_node::P2PConfig {
                    peer_key: defra_node::PeerKeySource::Store,
                    secret_key_path: unsafe { c_str_to_string(options.iroh_key_path) }
                        .map(PathBuf::from),
                    ..Default::default()
                };

                let relay_mode = unsafe { c_str_to_string(options.iroh_relay_mode) };
                let relay_url = unsafe { c_str_to_string(options.iroh_relay_url) };
                let relay_urls_json = unsafe { c_str_to_string(options.iroh_relay_urls_json) };
                let relay_urls = match relay_urls_json {
                    Some(raw) if !raw.trim().is_empty() => {
                        serde_json::from_str::<Vec<String>>(&raw)
                            .map_err(|error| format!("invalid iroh_relay_urls_json: {}", error))?
                    }
                    _ => Vec::new(),
                };

                config.relay_mode = match relay_mode.as_deref() {
                    Some("disabled") => p2p::iroh::IrohRelayModeConfig::Disabled,
                    Some("default") => p2p::iroh::IrohRelayModeConfig::Default,
                    Some("custom") => {
                        let mut urls = relay_urls;
                        if let Some(url) = relay_url {
                            urls.push(url);
                        }
                        if urls.is_empty() {
                            return Err("iroh_relay_mode=custom requires at least one relay URL"
                                .to_string());
                        }
                        p2p::iroh::IrohRelayModeConfig::Custom(urls)
                    }
                    Some(other) => {
                        return Err(format!(
                            "unsupported iroh_relay_mode '{}'. Supported: default, disabled, custom",
                            other
                        ));
                    }
                    None => {
                        let mut urls = relay_urls;
                        if let Some(url) = relay_url {
                            urls.push(url);
                        }
                        if urls.is_empty() {
                            p2p::iroh::IrohRelayModeConfig::Default
                        } else {
                            p2p::iroh::IrohRelayModeConfig::Custom(urls)
                        }
                    }
                };

                let discovery_origin =
                    unsafe { c_str_to_string(options.iroh_discovery_origin_domain) };
                let pkarr_relay_url = unsafe { c_str_to_string(options.iroh_pkarr_relay_url) };
                config.discovery = match (
                    options.iroh_discovery != 0,
                    discovery_origin,
                    pkarr_relay_url,
                ) {
                    (_, Some(origin_domain), Some(pkarr_relay_url)) => {
                        p2p::iroh::IrohDiscoveryConfig::CustomDns {
                            origin_domain,
                            pkarr_relay_url,
                        }
                    }
                    (_, Some(_), None) | (_, None, Some(_)) => {
                        return Err(
                            "custom iroh discovery requires both iroh_discovery_origin_domain and iroh_pkarr_relay_url"
                                .to_string(),
                        );
                    }
                    (false, None, None) => p2p::iroh::IrohDiscoveryConfig::Disabled,
                    (true, None, None) => p2p::iroh::IrohDiscoveryConfig::N0,
                };

                if !options.iroh_bind_addr.is_null() {
                    let bind_addr = unsafe { c_str_to_string(options.iroh_bind_addr) }
                        .ok_or_else(|| "iroh_bind_addr is not valid UTF-8".to_string())?;
                    config.bind_addr = Some(bind_addr.parse().map_err(|error| {
                        format!("invalid iroh_bind_addr '{}': {}", bind_addr, error)
                    })?);
                }
                if options.iroh_bind_port != 0 {
                    config.port = Some(options.iroh_bind_port);
                }

                if !listen_addr.trim().is_empty() {
                    let socket_addr: std::net::SocketAddr =
                        listen_addr.parse().map_err(|error| {
                            format!("invalid iroh listen address '{}': {}", listen_addr, error)
                        })?;
                    config.bind_addr = Some(socket_addr.ip());
                    config.port = Some(socket_addr.port());
                }

                Ok(defra_node::TransportConfig::Iroh(config))
            }
            #[cfg(not(feature = "iroh"))]
            {
                Err("iroh transport not enabled. Rebuild with --features iroh".to_string())
            }
        }
        other => Err(format!(
            "unsupported p2p transport '{}'. Supported: libp2p, iroh",
            other
        )),
    }
}

#[cfg(all(test, feature = "libp2p"))]
mod tests {
    use std::ffi::{CStr, CString};
    use std::ptr::null;

    use rapidhash::RapidHashSet;

    use super::*;
    use crate::node::node_close;
    use crate::types::defra_free_string;

    fn cstr(text: &str) -> CString {
        CString::new(text).unwrap()
    }

    #[test]
    fn libp2p_options_reach_the_host_config() {
        let addrs = cstr(r#"["/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.2/tcp/0"]"#);
        let peers = cstr(r#"["/ip4/127.0.0.1/tcp/9171/p2p/12D3KooWExample"]"#);
        let config = libp2p_config(
            &NodeInitOptions {
                p2p_listen_addrs_json: addrs.as_ptr(),
                p2p_bootstrap_peers_json: peers.as_ptr(),
                p2p_disable_pubsub: 1,
                p2p_disable_relay: 1,
                p2p_resource_memory_mib: 256,
                p2p_resource_file_descriptors: 512,
                p2p_max_streams_per_peer: 64,
                ..NodeInitOptions::default()
            },
            "/ip4/0.0.0.0/tcp/1",
        )
        .unwrap();
        assert_eq!(
            config.listen_addrs,
            ["/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.2/tcp/0"],
            "the list replaces listen_addr"
        );
        assert_eq!(
            config.bootstrap_peers,
            ["/ip4/127.0.0.1/tcp/9171/p2p/12D3KooWExample"]
        );
        assert!(!config.enable_pubsub);
        assert!(!config.enable_relay);
        assert_eq!(config.resource_limits.memory_mib, Some(256));
        assert_eq!(config.resource_limits.file_descriptors, Some(512));
        assert_eq!(config.resource_limits.max_streams_per_peer, Some(64));
    }

    #[test]
    fn libp2p_defaults_keep_the_host_defaults() {
        let config = libp2p_config(&NodeInitOptions::default(), "/ip4/127.0.0.1/tcp/0").unwrap();
        assert_eq!(
            config,
            defra_node::Libp2pConfig {
                listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_owned()],
                ..Default::default()
            }
        );
        let error = libp2p_config(&NodeInitOptions::default(), "").unwrap_err();
        assert!(error.contains("listen_addr is required"), "{error}");
        let bad = cstr("not json");
        let error = libp2p_config(
            &NodeInitOptions {
                p2p_bootstrap_peers_json: bad.as_ptr(),
                ..NodeInitOptions::default()
            },
            "/ip4/127.0.0.1/tcp/0",
        )
        .unwrap_err();
        assert!(error.contains("p2p_bootstrap_peers_json"), "{error}");
    }

    #[test]
    fn an_empty_listen_list_listens_on_nothing() {
        let empty = cstr("[]");
        let options = NodeInitOptions {
            p2p_listen_addrs_json: empty.as_ptr(),
            ..NodeInitOptions::default()
        };
        for listen_addr in ["/ip4/127.0.0.1/tcp/0", ""] {
            let config = libp2p_config(&options, listen_addr).unwrap();
            assert!(config.listen_addrs.is_empty(), "{listen_addr:?}");
        }
        let invalid = CString::from_vec_with_nul(b"[\xff]\0".to_vec()).unwrap();
        let error = libp2p_config(
            &NodeInitOptions {
                p2p_listen_addrs_json: invalid.as_ptr(),
                ..NodeInitOptions::default()
            },
            "/ip4/127.0.0.1/tcp/0",
        )
        .unwrap_err();
        assert!(error.contains("not valid UTF-8"), "{error}");
    }

    #[test]
    fn a_node_listens_on_every_listed_address() {
        assert!(crate::runtime::init_runtime());
        let addrs = cstr(r#"["/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.1/tcp/0"]"#);
        let options = NodeInitOptions {
            p2p_listen_addrs_json: addrs.as_ptr(),
            p2p_resource_memory_mib: 256,
            p2p_resource_file_descriptors: 512,
            p2p_max_streams_per_peer: 64,
            ..NodeInitOptions::default()
        };
        let result = unsafe { new_node_with_p2p(options, null()) };
        assert_eq!(result.status, 0);
        let node = result.node_ptr;
        let info = unsafe { crate::p2p::p2p_peer_info(node, null()) };
        assert_eq!(info.status, 0);
        let text = unsafe { CStr::from_ptr(info.value) }
            .to_string_lossy()
            .into_owned();
        unsafe { defra_free_string(info.value) };
        let listening: Vec<String> = serde_json::from_str(&text).unwrap();
        let bound: RapidHashSet<&String> = listening
            .iter()
            .filter(|addr| addr.starts_with("/ip4/127.0.0.1/tcp/"))
            .collect();
        assert_eq!(bound.len(), 2, "{listening:?}");
        node_close(node);
    }
}
