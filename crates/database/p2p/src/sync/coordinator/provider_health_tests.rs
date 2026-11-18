use super::*;

fn peer(id: &str) -> PeerId {
    PeerId::new(id.to_string())
}

fn peers(ids: &[&str]) -> Vec<PeerId> {
    ids.iter().copied().map(peer).collect()
}

fn failing(id: &str) -> PeerFetchStats {
    PeerFetchStats {
        peer: id.to_string(),
        fetches: 1_000,
        ..Default::default()
    }
}

fn order(health: &ProviderHealth, stats: &[PeerFetchStats], at: Instant) -> Vec<PeerId> {
    health.rank(peers(&["source", "a", "b"]), stats, at).peers
}

#[test]
fn measured_health_decides_whether_the_source_is_demoted() {
    let healthy = Ranking {
        peers: peers(&["source", "a", "b"]),
        demoted: 0,
        cooling: 0,
    };
    let demoted = Ranking {
        peers: peers(&["a", "b", "source"]),
        demoted: 1,
        cooling: 0,
    };
    let rank = |stats: &[PeerFetchStats]| {
        ProviderHealth::default().rank(peers(&["source", "a", "b"]), stats, Instant::now())
    };
    assert_eq!(rank(&[]), healthy, "unmeasured");
    // (fetches, success_ppm, partial_ppm, timeout_ppm, demoted)
    for (fetches, success_ppm, partial_ppm, timeout_ppm, is_demoted) in [
        (82_928, 20_897, 0, 7, true),
        (40, 600_000, 0, 700_000, true),
        (MIN_SAMPLES - 1, 0, 0, 0, false),
        (1_000, 100_000, 850_000, 0, false),
        (1_000, 200_000, 200_000, 0, true),
    ] {
        let stats = PeerFetchStats {
            peer: "source".to_string(),
            fetches,
            success_ppm,
            partial_ppm,
            timeout_ppm,
            ..Default::default()
        };
        let expected = if is_demoted { &demoted } else { &healthy };
        assert_eq!(&rank(std::slice::from_ref(&stats)), expected, "{stats:?}");
    }
}

#[test]
fn a_healthy_source_stays_first_on_every_root() {
    let health = ProviderHealth::default();
    for _ in 0..4 {
        let ranked = order(&health, &[failing("a")], Instant::now());
        assert_eq!(ranked, peers(&["source", "b", "a"]));
    }
}

#[test]
fn a_moved_back_source_spreads_first_windows_across_healthy_alternates() {
    let health = ProviderHealth::default();
    let firsts: Vec<PeerId> = (0..4)
        .map(|_| {
            let ranked = order(&health, &[failing("source")], Instant::now());
            assert_eq!(ranked.last(), Some(&peer("source")));
            ranked[0].clone()
        })
        .collect();
    assert_eq!(firsts, peers(&["a", "b", "a", "b"]));
}

#[test]
fn every_exploration_root_keeps_the_context_order() {
    let health = ProviderHealth::default();
    let orders: Vec<Vec<PeerId>> = (0..EXPLORE_EVERY)
        .map(|_| order(&health, &[failing("source")], Instant::now()))
        .collect();
    let (explored, ranked) = orders.split_last().unwrap();
    assert_eq!(explored, &peers(&["source", "a", "b"]));
    assert!(ranked
        .iter()
        .all(|order| order.last() == Some(&peer("source"))));
}

#[test]
fn a_cooling_peer_goes_behind_healthy_ones_and_ahead_of_demoted_ones() {
    let health = ProviderHealth::default();
    let now = Instant::now();
    health.record_fast_failure(&peer("a"), now);
    assert_eq!(order(&health, &[], now), peers(&["source", "b", "a"]));
    let ranked = order(&health, &[failing("source")], now);
    assert_eq!(ranked, peers(&["b", "a", "source"]));
}

#[test]
fn the_cooldown_ladder_doubles_while_fresh_caps_and_starts_over_once_stale() {
    let secs = Duration::from_secs;
    // (fast failures at, checked at, still cooling)
    let cases: [(&[u64], u64, bool); 6] = [
        (&[0], 1, true),
        (&[0], 2, false),
        (&[0, 0], 3, true),
        (&[0; 12], 30, false),
        (&[0, 3], 6, true),
        (&[0, 10], 12, false),
    ];
    let now = Instant::now();
    for (failures, checked, cooling) in cases {
        let health = ProviderHealth::default();
        for at in failures {
            health.record_fast_failure(&peer("source"), now + secs(*at));
        }
        let ranking = health.rank(peers(&["source", "a"]), &[], now + secs(checked));
        assert_eq!(ranking.cooling == 1, cooling, "{failures:?} at {checked}s");
    }
}

#[test]
fn delivering_blocks_ends_the_cooldown() {
    let health = ProviderHealth::default();
    let now = Instant::now();
    health.record_fast_failure(&peer("source"), now);
    health.record_productive(&peer("source"));
    assert_eq!(health.rank(peers(&["source", "a"]), &[], now).cooling, 0);
}

#[test]
fn a_peer_with_no_healthy_peer_ahead_of_it_is_not_counted_as_skipped() {
    let health = ProviderHealth::default();
    let now = Instant::now();
    health.record_fast_failure(&peer("source"), now);
    let ranking = health.rank(peers(&["source", "a"]), &[failing("a")], now);
    assert_eq!(ranking.peers, peers(&["source", "a"]));
    assert_eq!((ranking.cooling, ranking.demoted), (0, 0));
}

#[test]
fn a_full_table_forgets_stale_cooldowns_to_remember_a_new_peer() {
    let now = Instant::now();
    let health = ProviderHealth::default();
    for i in 0..MAX_COOLDOWN_PEERS {
        health.record_fast_failure(&peer(&format!("old-{i}")), now);
    }
    let cooling = |at| health.rank(peers(&["new", "a"]), &[], at).cooling;

    health.record_fast_failure(&peer("new"), now + Duration::from_secs(1));
    assert_eq!(
        cooling(now + Duration::from_secs(1)),
        0,
        "no stale entry yet"
    );

    let later = now + COOLDOWN_BASE * 2;
    health.record_fast_failure(&peer("new"), later);
    assert_eq!(cooling(later), 1);
    assert_eq!(health.cooldowns.len(), 1);
}
