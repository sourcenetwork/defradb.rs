use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use defra_harness::TestCluster;

/// How long a winning thief keeps the port before handing it back.
///
/// Long enough that a node spawned while it holds the port cannot possibly
/// bind (startup is tens of milliseconds at best), short enough that the
/// harness's reclaim budget outlives it.
const THIEF_HOLD: Duration = Duration::from_secs(5);

/// How long the thief keeps racing for the port before giving up, so a passing
/// run never hangs on the join.
const THIEF_DEADLINE: Duration = Duration::from_secs(20);

/// A competitor spinning on `port`, modelling any host process that is handed
/// the number while the node is down. Returns whether it ever won the race.
fn spawn_thief(port: u16, stop: Arc<AtomicBool>) -> JoinHandle<bool> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + THIEF_DEADLINE;
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
                std::thread::sleep(THIEF_HOLD);
                drop(listener);
                return true;
            }
        }
        false
    })
}

fn api_port(api_url: &str) -> u16 {
    api_url
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("no port in api url {}", api_url))
}

/// A restart must come back on the address its callers and peers already hold,
/// even when something else is racing for the port the whole time it is down.
///
/// Two mechanisms are exercised. The kill → respawn window is covered by
/// holding the ports, so a competitor can never be *handed* them by the OS
/// while the node is down. The respawn → child-bind window is covered by the
/// port-conflict retry, which waits for the transient holder to let go and
/// tries the same ports again — a restart may not move to fresh ones.
#[tokio::test]
async fn restart_keeps_its_ports_against_a_racing_binder() {
    let mut cluster = TestCluster::builder()
        .rust_nodes(1)
        .build()
        .await
        .expect("failed to build cluster");

    let api_url = cluster.api_url(0).to_string();
    let stop = Arc::new(AtomicBool::new(false));
    let thief = spawn_thief(api_port(&api_url), Arc::clone(&stop));

    let restarted = cluster.restart_node(0, Duration::from_secs(60)).await;

    stop.store(true, Ordering::SeqCst);
    let stolen = thief.join().expect("thief thread panicked");

    restarted.expect("node must restart while another binder races for its port");

    assert_eq!(
        cluster.api_url(0),
        api_url,
        "restart must not move the node off the address its callers hold"
    );
    assert!(
        stolen,
        "the competitor never got the port, so this run proved nothing"
    );
}
