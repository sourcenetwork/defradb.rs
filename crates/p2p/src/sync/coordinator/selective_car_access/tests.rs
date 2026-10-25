use multihash_codetable::{Code, MultihashDigest};

use super::*;

fn cid(label: &[u8]) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(label))
}

#[test]
fn completed_push_grants_post_ack_recovery_for_its_peer_and_root() {
    let access = Arc::new(SelectiveCarAccess::default());
    let peer = PeerId::new("peer-a".to_string());
    let other_peer = PeerId::new("peer-b".to_string());
    let root = cid(b"root");
    let child = cid(b"child");
    let unrelated = cid(b"unrelated");

    let guard = access.register(peer.clone(), root).unwrap();
    drop(guard);

    assert!(access.allows_root(&peer, &root));
    assert!(!access.allows_root(&peer, &unrelated));
    assert!(!access.allows_root(&peer, &child));
    assert!(!access.allows_root(&other_peer, &root));
}

#[test]
fn post_ack_grant_expires_after_recovery_window() {
    let access = Arc::new(SelectiveCarAccess::with_recovery_window(Duration::ZERO));
    let peer = PeerId::new("peer".to_string());
    let root = cid(b"root");
    let guard = access.register(peer.clone(), root).unwrap();
    assert!(access.allows_root(&peer, &root));

    drop(guard);
    assert!(!access.allows_root(&peer, &root));
}

#[test]
fn overlapping_push_grants_finish_independently() {
    let access = Arc::new(SelectiveCarAccess::with_recovery_window(Duration::ZERO));
    let peer = PeerId::new("peer".to_string());
    let root = cid(b"root");
    let first = access.register(peer.clone(), root).unwrap();
    let second = access.register(peer.clone(), root).unwrap();
    drop(first);
    assert!(access.allows_root(&peer, &root));

    drop(second);
    assert!(!access.allows_root(&peer, &root));
}

#[test]
fn active_authority_is_bounded_and_overflow_is_actionable() {
    let access = Arc::new(SelectiveCarAccess::default());
    let peer = PeerId::new("peer".to_string());
    let root = cid(b"root");
    let mut grants = Vec::new();
    for _ in 0..MAX_SELECTIVE_CAR_GRANTS_PER_PEER {
        grants.push(access.register(peer.clone(), root).unwrap());
    }
    assert!(access.register(peer, root).is_none());
    assert_eq!(grants.len(), MAX_SELECTIVE_CAR_GRANTS_PER_PEER);
}

fn seed_grants(
    access: &SelectiveCarAccess,
    peer: &PeerId,
    root: Cid,
    count: usize,
    completed: bool,
) {
    let grants = access
        .grants
        .get_or_insert(peer.clone(), Arc::new(rapid_map()));
    let expiry = completed.then(|| Instant::now() + access.recovery_window);
    for _ in 0..count {
        let id = access.next_id.fetch_add(1, Ordering::Relaxed);
        grants.insert(
            id,
            Arc::new(PushGrant {
                root_cid: root,
                expires_at: Atom::new(expiry),
            }),
        );
    }
}

fn grant_count(access: &SelectiveCarAccess) -> usize {
    access.grants.values().map(|grants| grants.len()).sum()
}

#[test]
fn completed_entries_yield_peer_capacity_without_displacing_another_peers_active_send() {
    let access = Arc::new(SelectiveCarAccess::default());
    let peer = PeerId::new("warm-peer".to_string());
    let other = PeerId::new("active-peer".to_string());
    let old_root = cid(b"old-root");
    let active_root = cid(b"active-root");
    let new_root = cid(b"new-root");
    let _active = access.register(other.clone(), active_root).unwrap();
    seed_grants(
        &access,
        &peer,
        old_root,
        MAX_SELECTIVE_CAR_GRANTS_PER_PEER,
        true,
    );

    let _new = access.register(peer.clone(), new_root).unwrap();

    assert_eq!(
        access.grants.get(&peer).unwrap().len(),
        MAX_SELECTIVE_CAR_GRANTS_PER_PEER
    );
    assert_eq!(access.grants.get(&other).unwrap().len(), 1);
    assert!(access.allows_root(&other, &active_root));
    assert!(access.allows_root(&peer, &new_root));
    assert!(!access.allows_root(&other, &new_root));
    assert!(!access.allows_root(&peer, &active_root));
}

fn fill_global_capacity(access: &SelectiveCarAccess, active_peer: &PeerId, completed: bool) {
    let root = cid(b"cached-root");
    let mut remaining = MAX_SELECTIVE_CAR_GRANTS - 1;
    let mut index = 0;
    while remaining > 0 {
        let peer = if index == 0 {
            active_peer.clone()
        } else {
            PeerId::new(format!("cached-peer-{index}"))
        };
        let occupied = usize::from(index == 0);
        let count = remaining.min(MAX_SELECTIVE_CAR_GRANTS_PER_PEER - occupied);
        seed_grants(access, &peer, root, count, completed);
        remaining -= count;
        index += 1;
    }
    assert_eq!(grant_count(access), MAX_SELECTIVE_CAR_GRANTS);
}

#[test]
fn completed_entries_yield_global_capacity_while_active_authority_survives() {
    let access = Arc::new(SelectiveCarAccess::default());
    let active_peer = PeerId::new("active-peer".to_string());
    let active_root = cid(b"active-root");
    let _active = access.register(active_peer.clone(), active_root).unwrap();
    fill_global_capacity(&access, &active_peer, true);
    let new_peer = PeerId::new("new-peer".to_string());
    let new_root = cid(b"new-root");

    let _new = access.register(new_peer.clone(), new_root).unwrap();

    assert_eq!(grant_count(&access), MAX_SELECTIVE_CAR_GRANTS);
    assert!(access.allows_root(&active_peer, &active_root));
    assert!(access.allows_root(&new_peer, &new_root));
    assert!(!access.allows_root(&active_peer, &new_root));
    assert!(!access.allows_root(&new_peer, &active_root));
}

#[test]
fn active_entries_at_global_capacity_are_not_reclaimed() {
    let access = Arc::new(SelectiveCarAccess::default());
    let peer = PeerId::new("active-peer".to_string());
    let root = cid(b"active-root");
    let _active = access.register(peer.clone(), root).unwrap();
    fill_global_capacity(&access, &peer, false);
    let new_peer = PeerId::new("new-peer".to_string());
    let new_root = cid(b"new-root");

    assert!(access.register(new_peer.clone(), new_root).is_none());

    assert_eq!(grant_count(&access), MAX_SELECTIVE_CAR_GRANTS);
    assert!(access.allows_root(&peer, &root));
    assert!(!access.allows_root(&new_peer, &new_root));
}

#[test]
fn completed_entries_keep_their_recovery_window_without_capacity_pressure() {
    let access = Arc::new(SelectiveCarAccess::default());
    let peer = PeerId::new("peer".to_string());
    let old_root = cid(b"old-root");
    let new_root = cid(b"new-root");
    drop(access.register(peer.clone(), old_root).unwrap());

    let _new = access.register(peer.clone(), new_root).unwrap();

    assert_eq!(access.grants.get(&peer).unwrap().len(), 2);
    assert!(access.allows_root(&peer, &old_root));
    assert!(access.allows_root(&peer, &new_root));
}
