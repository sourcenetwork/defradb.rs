//! The P2P transport a C caller's [`NodeInitOptions`] ask for.

use crate::target::{go_string, split_comma_separated};
use crate::types::NodeInitOptions;

/// The transport `options` ask for: none when `disable_p2p` is set,
/// otherwise pinc.
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
    pinc(options, retry_intervals)
}

/// The replicator retry ladder `transport` runs, in seconds; `None` for the
/// default one, or for no transport.
pub(crate) fn retry_intervals_of(transport: &defra_node::TransportConfig) -> Option<&[u64]> {
    match transport {
        #[cfg(feature = "p2p")]
        defra_node::TransportConfig::Pinc(pinc) => pinc.sync.replicator_retry_intervals.as_deref(),
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
#[cfg(feature = "p2p")]
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

/// The pinc node `options` describe: it binds `listening_addresses` (one
/// `ip:port`, every interface on a port the OS picks when empty), dials
/// `peers` once started, runs topics through `pinc_hubs`, and admits the
/// peers `pinc_allowed_peers` names, or every peer when it names none.
///
/// # Safety
///
/// As for [`read`].
#[cfg(feature = "p2p")]
unsafe fn pinc(
    options: &NodeInitOptions,
    retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    let bind = match split_comma_separated(&go_string(options.listening_addresses)).as_slice() {
        [] => std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
        [addr] => addr
            .trim()
            .parse()
            .map_err(|error| format!("invalid listening address '{}': {}", addr, error))?,
        _ => return Err("a pinc node listens on one address".to_string()),
    };
    let checked = |field: &str, addrs: Vec<String>| {
        addrs
            .into_iter()
            .map(|addr| match defra_pinc::parse_public_peer_addr(&addr) {
                Ok(_) => Ok(addr),
                Err(error) => Err(format!("invalid {field} address '{addr}': {error}")),
            })
            .collect::<Result<Vec<_>, _>>()
    };
    let bootstrap_peers = checked("peers", split_comma_separated(&go_string(options.peers)))?;
    let hubs = checked(
        "pinc_hubs",
        split_comma_separated(&go_string(options.pinc_hubs)),
    )?;
    let allowed = split_comma_separated(&go_string(options.pinc_allowed_peers));
    let allowlist = if allowed.is_empty() {
        defra_pinc::PincAllowlistConfig::AcceptAll
    } else {
        defra_pinc::PincAllowlistConfig::Explicit(allowed.into_iter().collect())
    };
    let block_sync_timeout = match options.p2p_block_sync_timeout_ms {
        0 => defra_pinc::PincEndpointConfig::DEFAULT_BLOCK_SYNC_TIMEOUT,
        ms if ms > 0 => std::time::Duration::from_millis(ms as u64),
        _ => return Err("p2p_block_sync_timeout_ms must not be negative".to_string()),
    };
    Ok(defra_node::TransportConfig::Pinc(defra_node::PincConfig {
        bind,
        allowlist,
        hubs,
        bootstrap_peers,
        block_sync_timeout,
        peer_key: defra_node::PeerKeySource::Store,
        sync: sync_tuning(options, retry_intervals),
        ..Default::default()
    }))
}

/// # Safety
///
/// As for [`read`].
#[cfg(not(feature = "p2p"))]
unsafe fn pinc(
    _options: &NodeInitOptions,
    _retry_intervals: Option<Vec<u64>>,
) -> Result<defra_node::TransportConfig, String> {
    Err(
        "this build does not include P2P; rebuild with the p2p feature, or set disable_p2p"
            .to_string(),
    )
}

#[cfg(all(test, feature = "p2p"))]
mod tests {
    use std::ffi::CString;

    use super::*;

    const PEER_ID: &str = "ae58ff8833241ac82d6ff7611046ed67b5072d142c588d0063e942d9a75502b6";

    fn pinc_config(options: &NodeInitOptions) -> Result<defra_node::PincConfig, String> {
        match unsafe { read(options) }? {
            defra_node::TransportConfig::Pinc(config) => Ok(config),
            _ => panic!("not a pinc transport"),
        }
    }

    #[test]
    fn the_default_binds_every_interface_on_a_port_the_os_picks() {
        let config = pinc_config(&NodeInitOptions::default()).unwrap();
        assert_eq!(config.bind, std::net::SocketAddr::from(([0, 0, 0, 0], 0)));
        assert!(config.hubs.is_empty());
        assert!(config.bootstrap_peers.is_empty());
        assert!(matches!(
            config.allowlist,
            defra_pinc::PincAllowlistConfig::AcceptAll
        ));
    }

    #[test]
    fn the_pinc_options_reach_the_config() {
        let listen = CString::new("127.0.0.1:4000").unwrap();
        let peer = format!("{PEER_ID}@127.0.0.1:4001");
        let hub = format!("{PEER_ID}@127.0.0.1:4002");
        let peers = CString::new(peer.clone()).unwrap();
        let hubs = CString::new(hub.clone()).unwrap();
        let allowed = CString::new(PEER_ID).unwrap();
        let config = pinc_config(&NodeInitOptions {
            listening_addresses: listen.as_ptr(),
            peers: peers.as_ptr(),
            pinc_hubs: hubs.as_ptr(),
            pinc_allowed_peers: allowed.as_ptr(),
            max_concurrent_dag_fetches: 2,
            ..NodeInitOptions::default()
        })
        .unwrap();
        assert_eq!(
            config.bind,
            std::net::SocketAddr::from(([127, 0, 0, 1], 4000))
        );
        assert_eq!(config.bootstrap_peers, [peer]);
        assert_eq!(config.hubs, [hub]);
        match config.allowlist {
            defra_pinc::PincAllowlistConfig::Explicit(peers) => {
                assert_eq!(peers.into_iter().collect::<Vec<_>>(), [PEER_ID]);
            }
            _ => panic!("the allowlist was dropped"),
        }
        assert_eq!(config.sync.max_concurrent_dag_fetches, 2);
    }

    #[test]
    fn malformed_addresses_are_refused() {
        let multiaddr = CString::new("/ip4/127.0.0.1/tcp/9171").unwrap();
        let two = CString::new("127.0.0.1:1,127.0.0.1:2").unwrap();
        for (options, field) in [
            (
                NodeInitOptions {
                    listening_addresses: multiaddr.as_ptr(),
                    ..NodeInitOptions::default()
                },
                "listening address",
            ),
            (
                NodeInitOptions {
                    listening_addresses: two.as_ptr(),
                    ..NodeInitOptions::default()
                },
                "one address",
            ),
            (
                NodeInitOptions {
                    peers: multiaddr.as_ptr(),
                    ..NodeInitOptions::default()
                },
                "peers",
            ),
            (
                NodeInitOptions {
                    pinc_hubs: multiaddr.as_ptr(),
                    ..NodeInitOptions::default()
                },
                "pinc_hubs",
            ),
        ] {
            let error = pinc_config(&options).unwrap_err();
            assert!(error.contains(field), "{error}");
        }
    }
}
