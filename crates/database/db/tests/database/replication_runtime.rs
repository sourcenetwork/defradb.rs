use super::*;

#[test]
fn cloned_plans_keep_their_replay_alive_until_the_last_owner_drops() {
    let runtime = ReplicationRuntime::default();
    let owner = runtime.own_install("peer", 1).unwrap();
    let clone = Arc::clone(&owner);
    assert!(runtime.install_is_running("peer", 1));
    drop(owner);
    assert!(runtime.install_is_running("peer", 1));
    drop(clone);
    assert!(!runtime.install_is_running("peer", 1));
    assert!(runtime.owners.is_empty());
}

#[test]
fn an_expired_observation_cannot_remove_a_replacement_owner() {
    let runtime = ReplicationRuntime::default();
    let key = ("peer".to_owned(), 1);
    let owner = runtime.own_install("peer", 1).unwrap();
    let observed = runtime.owners.get(&key).unwrap();
    drop(owner);
    let replacement = runtime.own_install("peer", 1).unwrap();
    assert!(runtime
        .owners
        .remove_if(&key, |current| Weak::ptr_eq(current, &observed))
        .is_none());
    assert!(runtime.install_is_running("peer", 1));
    drop(replacement);
    assert!(runtime.owners.is_empty());
}

#[test]
fn rejecting_a_duplicate_identity_does_not_release_its_owner() {
    let runtime = ReplicationRuntime::default();
    let owner = runtime.own_install("peer", 1).unwrap();
    assert!(runtime.own_install("peer", 1).is_none());
    assert!(runtime.install_is_running("peer", 1));
    drop(owner);
    assert!(runtime.owners.is_empty());
}
