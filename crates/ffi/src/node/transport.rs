//! The P2P transport a C caller's [`NodeInitOptions`] ask for.

#[cfg(any(feature = "libp2p", feature = "iroh"))]
use crate::target::non_empty;
use crate::target::{go_string, split_comma_separated};
use crate::types::NodeInitOptions;

/// Where a libp2p node listens when the caller names nowhere, as in Go.
#[cfg(feature = "libp2p")]
const DEFAULT_LISTEN_ADDRESS: &str = "/ip4/0.0.0.0/tcp/9171";

/// The transport `options` ask for: none when `disable_p2p` is set,
/// otherwise the one `p2p_transport` names, libp2p by default.
///
/// # Safety
///
/// Every non-null string in `options` must be a valid null-terminated string.
pub(crate) unsafe fn read(
    options: &NodeInitOptions,
) -> Result<defra_node::TransportConfig, String> {
    let retry_intervals = retry_intervals(options)?;
    if options.disable_p2p != 0 {
        return Ok(defra_node::TransportConfig::None);
    }
    let listen_addrs = split_comma_separated(&go_string(options.listening_addresses));
    let peers = split_comma_separated(&go_string(options.peers));

    match go_string(options.p2p_transport).to_lowercase().as_str() {
        "" | "libp2p" => libp2p(options, listen_addrs, peers, retry_intervals),
        "iroh" => iroh(options, listen_addrs, peers, retry_intervals),
        other => Err(format!(
            "unsupported p2p transport '{}'. Supported: libp2p, iroh",
            other
        )),
    }
}

/// The replicator retry ladder `transport` runs, in seconds; `None` for the
/// default one, or for no transport.
pub(crate) fn retry_intervals_of(transport: &defra_node::TransportConfig) -> Option<&[u64]> {
    match transport {
        #[cfg(feature = "libp2p")]
        defra_node::TransportConfig::Libp2p(libp2p) => {
            libp2p.sync.replicator_retry_intervals.as_deref()
        }
        #[cfg(feature = "iroh")]
        defra_node::TransportConfig::Iroh(iroh) => iroh.sync.replicator_retry_intervals.as_deref(),
        _ => None,
    }
}

/// `replicator_retry_intervals`, comma-separated whole seconds.
///
/// # Safety
///
/// As for [`read`].
unsafe fn retry_intervals(options: &NodeInitOptions) -> Result<Option<Vec<u64>>, String> {
    let intervals = split_comma_separated(&go_string(options.replicator_retry_intervals))
        .iter()
        .map(|interval| match interval.trim().parse::<i64>() {
            Ok(seconds) if seconds > 0 => Ok(seconds as u64),
            Ok(_) => {
                Err("negative time intervals are not allowed for replicator retries".to_string())
            }
            Err(error) => Err(format!(
                "invalid replicator retry interval '{}': {}",
                interval, error
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((!intervals.is_empty()).then_some(intervals))
}

/// The sync limits and retry ladder the caller set; a zero leaves the node's
/// default.
#[cfg(any(feature = "libp2p", feature = "iroh"))]
fn sync_tuning(
    options: &NodeInitOptions,
    replicator_retry_intervals: Option<Vec<u64>>,
) -> defra_node::SyncTuning {
    let mut tuning = defra_node::SyncTuning {
        replicator_retry_intervals,
        ..Default::default()
    };
    if options.max_concurrent_dag_fetches > 0 {
        tuning.max_concurrent_dag_fetches = options.max_concurrent_dag_fetches;
    }
    if options.max_concurrent_push_tasks > 0 {
        tuning.max_concurrent_push_tasks = options.max_concurrent_push_tasks;
    }
    if options.max_doc_sync_request_doc_ids > 0 {
        tuning.max_doc_sync_request_doc_ids = options.max_doc_sync_request_doc_ids;
    }
    if options.rate_limit_burst > 0 {
        tuning.rate_limit_burst = options.rate_limit_burst;
    }
    if options.rate_limit_rate > 0.0 {
        tuning.rate_limit_rate = options.rate_limit_rate;
    }
    tuning
}

/// # Safety
///
/// As for [`read`].
#[cfg(feature = "libp2p")]
unsafe fn libp2p(
    options: &NodeInitOptions,
    listen_addrs: Vec<String>,
    peers: Vec<String>,
    retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    reject_iroh_only(options)?;
    let listen_addrs = match (options.disable_p2p_listen != 0, listen_addrs.is_empty()) {
        (true, true) => Vec::new(),
        (true, false) => {
            return Err("listening_addresses and disable_p2p_listen both set; set one".to_string())
        }
        (false, true) => vec![DEFAULT_LISTEN_ADDRESS.to_string()],
        (false, false) => listen_addrs,
    };
    Ok(defra_node::TransportConfig::Libp2p(
        defra_node::Libp2pConfig {
            listen_addrs,
            bootstrap_peers: peers,
            enable_pubsub: options.enable_pub_sub != 0,
            enable_relay: options.enable_relay != 0,
            stream_timeout_secs: (options.p2p_block_sync_timeout_ms > 0)
                .then(|| (options.p2p_block_sync_timeout_ms as u64).div_ceil(1000)),
            sync: sync_tuning(options, retry_intervals),
            resource_limits: p2p::ResourceLimits {
                memory_mib: (options.p2p_resource_memory_mib > 0)
                    .then_some(options.p2p_resource_memory_mib),
                file_descriptors: (options.p2p_resource_file_descriptors > 0)
                    .then_some(options.p2p_resource_file_descriptors),
                max_streams_per_peer: (options.p2p_max_streams_per_peer > 0)
                    .then_some(options.p2p_max_streams_per_peer),
            },
            ..Default::default()
        },
    ))
}

/// iroh options a libp2p node has no counterpart for: set, they fail the
/// call rather than leave the caller believing they took effect.
///
/// # Safety
///
/// As for [`read`].
#[cfg(feature = "libp2p")]
unsafe fn reject_iroh_only(options: &NodeInitOptions) -> Result<(), String> {
    let iroh_only = [
        (
            non_empty(options.iroh_relay_url).is_some(),
            "iroh_relay_url",
        ),
        (
            non_empty(options.iroh_relay_mode).is_some(),
            "iroh_relay_mode",
        ),
        (
            non_empty(options.iroh_relay_urls_json).is_some(),
            "iroh_relay_urls_json",
        ),
        (
            non_empty(options.iroh_bind_addr).is_some(),
            "iroh_bind_addr",
        ),
        (options.iroh_bind_port != 0, "iroh_bind_port"),
        (
            options.iroh_disable_discovery != 0,
            "iroh_disable_discovery",
        ),
        (
            non_empty(options.iroh_discovery_origin_domain).is_some(),
            "iroh_discovery_origin_domain",
        ),
        (
            non_empty(options.iroh_pkarr_relay_url).is_some(),
            "iroh_pkarr_relay_url",
        ),
        (non_empty(options.iroh_key_path).is_some(), "iroh_key_path"),
    ];
    match iroh_only.iter().find(|(set, _)| *set) {
        Some((_, what)) => Err(format!(
            "{} is an iroh option; the libp2p transport does not use it",
            what
        )),
        None => Ok(()),
    }
}

#[cfg(not(feature = "libp2p"))]
unsafe fn libp2p(
    _options: &NodeInitOptions,
    _listen_addrs: Vec<String>,
    _peers: Vec<String>,
    _retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    Err(
        "this build does not include the libp2p transport; rebuild with the libp2p feature, \
         or set disable_p2p"
            .to_string(),
    )
}

#[cfg(not(feature = "iroh"))]
unsafe fn iroh(
    _options: &NodeInitOptions,
    _listen_addrs: Vec<String>,
    _peers: Vec<String>,
    _retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    Err("iroh transport not enabled. Rebuild with --features iroh".to_string())
}

/// # Safety
///
/// As for [`read`].
#[cfg(feature = "iroh")]
unsafe fn iroh(
    options: &NodeInitOptions,
    listen_addrs: Vec<String>,
    peers: Vec<String>,
    retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    if !peers.is_empty() {
        return Err("not supported yet: peers with the iroh transport".to_string());
    }
    reject_libp2p_only(options)?;
    if !listen_addrs.is_empty()
        && (non_empty(options.iroh_bind_addr).is_some() || options.iroh_bind_port != 0)
    {
        return Err(
            "listening_addresses and iroh_bind_addr/iroh_bind_port both set where the iroh \
             transport listens; set one"
                .to_string(),
        );
    }
    // The node keeps one peer key in its store, for either transport; an iroh
    // key file only seeds it.
    let mut config = defra_node::P2PConfig {
        peer_key: defra_node::PeerKeySource::Store,
        secret_key_path: non_empty(options.iroh_key_path).map(std::path::PathBuf::from),
        sync: sync_tuning(options, retry_intervals),
        ..Default::default()
    };

    let relay_url = non_empty(options.iroh_relay_url);
    let mut relay_urls = match non_empty(options.iroh_relay_urls_json) {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str::<Vec<String>>(&raw)
            .map_err(|error| format!("invalid iroh_relay_urls_json: {}", error))?,
        _ => Vec::new(),
    };
    config.relay_mode = match non_empty(options.iroh_relay_mode).as_deref() {
        Some("disabled") => p2p::iroh::IrohRelayModeConfig::Disabled,
        Some("default") => p2p::iroh::IrohRelayModeConfig::Default,
        Some("custom") => {
            relay_urls.extend(relay_url);
            if relay_urls.is_empty() {
                return Err("iroh_relay_mode=custom requires at least one relay URL".to_string());
            }
            p2p::iroh::IrohRelayModeConfig::Custom(relay_urls)
        }
        Some(other) => {
            return Err(format!(
                "unsupported iroh_relay_mode '{}'. Supported: default, disabled, custom",
                other
            ));
        }
        None => {
            relay_urls.extend(relay_url);
            if relay_urls.is_empty() {
                p2p::iroh::IrohRelayModeConfig::Default
            } else {
                p2p::iroh::IrohRelayModeConfig::Custom(relay_urls)
            }
        }
    };

    config.discovery = match (
        options.iroh_disable_discovery == 0,
        non_empty(options.iroh_discovery_origin_domain),
        non_empty(options.iroh_pkarr_relay_url),
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

    if let Some(bind_addr) = non_empty(options.iroh_bind_addr) {
        config.bind_addr = Some(
            bind_addr
                .parse()
                .map_err(|error| format!("invalid iroh_bind_addr '{}': {}", bind_addr, error))?,
        );
    }
    if options.iroh_bind_port != 0 {
        config.port = Some(options.iroh_bind_port);
    }

    match listen_addrs.as_slice() {
        [] => {}
        [listen_addr] => {
            let socket_addr: std::net::SocketAddr = listen_addr.parse().map_err(|error| {
                format!("invalid iroh listen address '{}': {}", listen_addr, error)
            })?;
            config.bind_addr = Some(socket_addr.ip());
            config.port = Some(socket_addr.port());
        }
        _ => return Err("the iroh transport listens on one address".to_string()),
    }

    Ok(defra_node::TransportConfig::Iroh(config))
}

/// libp2p options an iroh node has no counterpart for: set, they fail the
/// call rather than leave the caller believing they took effect.
#[cfg(feature = "iroh")]
fn reject_libp2p_only(options: &NodeInitOptions) -> Result<(), String> {
    let libp2p_only = [
        (options.disable_p2p_listen != 0, "disable_p2p_listen"),
        (options.enable_pub_sub != 0, "enable_pub_sub"),
        (options.enable_relay != 0, "enable_relay"),
        (
            options.p2p_block_sync_timeout_ms != 0,
            "p2p_block_sync_timeout_ms",
        ),
        (
            options.p2p_resource_memory_mib != 0,
            "p2p_resource_memory_mib",
        ),
        (
            options.p2p_resource_file_descriptors != 0,
            "p2p_resource_file_descriptors",
        ),
        (
            options.p2p_max_streams_per_peer != 0,
            "p2p_max_streams_per_peer",
        ),
    ];
    match libp2p_only.iter().find(|(set, _)| *set) {
        Some((_, what)) => Err(format!(
            "{} is a libp2p option; the iroh transport does not use it",
            what
        )),
        None => Ok(()),
    }
}

#[cfg(all(test, feature = "iroh"))]
mod iroh_tests {
    use std::ffi::CString;

    use super::*;

    /// `read` of iroh options with `set` applied.
    fn read_iroh<S>(set: S) -> Result<defra_node::TransportConfig, String>
    where
        S: FnOnce(&mut NodeInitOptions),
    {
        let iroh = CString::new("iroh").unwrap();
        let mut options = NodeInitOptions {
            p2p_transport: iroh.as_ptr(),
            ..NodeInitOptions::default()
        };
        set(&mut options);
        unsafe { read(&options) }
    }

    fn refused<S>(set: S) -> String
    where
        S: FnOnce(&mut NodeInitOptions),
    {
        match read_iroh(set) {
            Err(error) => error,
            Ok(_) => panic!("the options were accepted"),
        }
    }

    /// Sets one option on `options`.
    type SetOption = fn(&mut NodeInitOptions);

    #[test]
    fn libp2p_only_options_are_refused_under_iroh() {
        let cases: [(SetOption, &str); 7] = [
            (|o| o.disable_p2p_listen = 1, "disable_p2p_listen"),
            (|o| o.enable_pub_sub = 1, "enable_pub_sub"),
            (|o| o.enable_relay = 1, "enable_relay"),
            (
                |o| o.p2p_block_sync_timeout_ms = 5000,
                "p2p_block_sync_timeout_ms",
            ),
            (
                |o| o.p2p_resource_memory_mib = 256,
                "p2p_resource_memory_mib",
            ),
            (
                |o| o.p2p_resource_file_descriptors = 512,
                "p2p_resource_file_descriptors",
            ),
            (
                |o| o.p2p_max_streams_per_peer = 64,
                "p2p_max_streams_per_peer",
            ),
        ];
        for (set, name) in cases {
            let error = refused(set);
            assert!(error.contains(name), "{error}");
        }
    }

    #[test]
    fn listening_addresses_and_iroh_bind_together_are_refused() {
        let listen = CString::new("127.0.0.1:0").unwrap();
        let bind_addr = CString::new("127.0.0.1").unwrap();
        let error = refused(|o| {
            o.listening_addresses = listen.as_ptr();
            o.iroh_bind_port = 4000;
        });
        assert!(error.contains("listening_addresses"), "{error}");
        let error = refused(|o| {
            o.listening_addresses = listen.as_ptr();
            o.iroh_bind_addr = bind_addr.as_ptr();
        });
        assert!(error.contains("listening_addresses"), "{error}");
    }

    #[test]
    fn listening_addresses_alone_sets_the_iroh_bind() {
        let listen = CString::new("127.0.0.1:4000").unwrap();
        match read_iroh(|o| o.listening_addresses = listen.as_ptr()).unwrap() {
            defra_node::TransportConfig::Iroh(config) => {
                assert_eq!(config.bind_addr, Some([127, 0, 0, 1].into()));
                assert_eq!(config.port, Some(4000));
            }
            _ => panic!("not an iroh transport"),
        }
    }
}

#[cfg(all(test, feature = "libp2p"))]
mod tests {
    use super::*;

    fn libp2p_config(options: &NodeInitOptions) -> defra_node::Libp2pConfig {
        match unsafe { read(options) }.unwrap() {
            defra_node::TransportConfig::Libp2p(config) => config,
            _ => panic!("not a libp2p transport"),
        }
    }

    #[test]
    fn the_libp2p_resource_limits_reach_the_host_config() {
        let config = libp2p_config(&NodeInitOptions {
            p2p_resource_memory_mib: 256,
            p2p_resource_file_descriptors: 512,
            p2p_max_streams_per_peer: 64,
            ..NodeInitOptions::default()
        });
        assert_eq!(config.resource_limits.memory_mib, Some(256));
        assert_eq!(config.resource_limits.file_descriptors, Some(512));
        assert_eq!(config.resource_limits.max_streams_per_peer, Some(64));
    }

    #[test]
    fn an_empty_listen_list_listens_on_the_go_default() {
        let config = libp2p_config(&NodeInitOptions::default());
        assert_eq!(
            config.listen_addrs,
            vec![DEFAULT_LISTEN_ADDRESS.to_string()]
        );
    }

    #[test]
    fn disable_p2p_listen_runs_a_dial_only_host() {
        let config = libp2p_config(&NodeInitOptions {
            disable_p2p_listen: 1,
            ..NodeInitOptions::default()
        });
        assert!(config.listen_addrs.is_empty(), "{:?}", config.listen_addrs);
    }

    #[test]
    fn disable_p2p_listen_with_listening_addresses_is_refused() {
        let listen = std::ffi::CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
        let options = NodeInitOptions {
            disable_p2p_listen: 1,
            listening_addresses: listen.as_ptr(),
            ..NodeInitOptions::default()
        };
        let error = match unsafe { read(&options) } {
            Err(error) => error,
            Ok(_) => panic!("both were accepted"),
        };
        assert!(error.contains("disable_p2p_listen"), "{error}");
    }

    #[test]
    fn zero_resource_limits_keep_the_host_defaults() {
        let config = libp2p_config(&NodeInitOptions::default());
        assert_eq!(config.resource_limits, p2p::ResourceLimits::default());
    }

    /// Sets one option on `options`, to `value` when it is a string.
    type SetOption = fn(&mut NodeInitOptions, *const std::ffi::c_char);

    #[test]
    fn iroh_only_options_are_refused_under_libp2p() {
        let value = std::ffi::CString::new("127.0.0.1").unwrap();
        let cases: [(SetOption, &str); 9] = [
            (|o, v| o.iroh_relay_url = v, "iroh_relay_url"),
            (|o, v| o.iroh_relay_mode = v, "iroh_relay_mode"),
            (|o, v| o.iroh_relay_urls_json = v, "iroh_relay_urls_json"),
            (|o, v| o.iroh_bind_addr = v, "iroh_bind_addr"),
            (|o, _| o.iroh_bind_port = 4000, "iroh_bind_port"),
            (
                |o, _| o.iroh_disable_discovery = 1,
                "iroh_disable_discovery",
            ),
            (
                |o, v| o.iroh_discovery_origin_domain = v,
                "iroh_discovery_origin_domain",
            ),
            (|o, v| o.iroh_pkarr_relay_url = v, "iroh_pkarr_relay_url"),
            (|o, v| o.iroh_key_path = v, "iroh_key_path"),
        ];
        for (set, name) in cases {
            let mut options = NodeInitOptions::default();
            set(&mut options, value.as_ptr());
            let error = match unsafe { read(&options) } {
                Err(error) => error,
                Ok(_) => panic!("{name} was accepted"),
            };
            assert!(error.contains(name), "{error}");
        }
    }
}
