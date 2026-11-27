#[cfg(test)]
mod unit_tests {
    use acp::nac::{NacStatus, NodePermission};
    use identity::Did;

    use db::nac::factory::create_memory_nac_manager;
    use db::nac::{NacCaller, NacConfig};

    fn test_did() -> Did {
        Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap()
    }

    fn test_did2() -> Did {
        Did::new("did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH").unwrap()
    }

    #[test]
    fn test_nac_manager_disabled_by_default() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::default());

            assert!(!manager.is_enabled().await);
            assert_eq!(manager.status().await, NacStatus::NotConfigured);

            let identity = test_did();
            let allowed = manager
                .check_permission(&identity, NodePermission::DacBypass)
                .await
                .unwrap();
            assert!(allowed);
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_enable() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());

            let owner = test_did();
            manager.initialize(Some(&owner)).await.unwrap();

            assert!(manager.is_enabled().await);
            assert_eq!(manager.status().await, NacStatus::Enabled);
            assert!(manager.is_owner(&owner).await);
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_permission_check() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());

            let owner = test_did();
            let other = test_did2();
            manager.initialize(Some(&owner)).await.unwrap();

            assert!(manager
                .check_permission(&owner, NodePermission::DacBypass)
                .await
                .unwrap());

            assert!(!manager
                .check_permission(&other, NodePermission::DacBypass)
                .await
                .unwrap());
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_admin() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());

            let owner = test_did();
            let admin = test_did2();
            manager.initialize(Some(&owner)).await.unwrap();

            let added = manager.add_admin(&owner, &admin).await.unwrap();
            assert!(added);

            assert!(manager
                .check_permission(&admin, NodePermission::DacBypass)
                .await
                .unwrap());
        })
        .unwrap()
    }

    /// The NAC store a node keeps in its own regolith store.
    type Stored = acp::PersistentZanzibarStore<storage::RegolithStore>;

    /// A relationship change for `target`, as `DB::change_nac_relationship`
    /// builds one.
    fn admin_change(change: db::nac::NacChange, target: &str) -> db::nac::NacRelationshipRequest {
        db::nac::NacRelationshipRequest {
            change,
            relation: Some(db::nac::NacRelation::Admin),
            target: db::nac::NacTarget::parse(target),
        }
    }

    /// Go's `ErrRelationshipTargetActorEmpty`: an empty target is refused on
    /// add and on delete, so it never reaches the wildcard relationship; a
    /// target that is no DID still finds nothing to take.
    #[test]
    fn an_empty_target_is_refused_and_never_reaches_the_wildcard() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());
            let owner = test_did();
            manager.initialize(Some(&owner)).await.unwrap();
            let caller = NacCaller {
                identity: Some(owner.clone()),
                node: None,
            };
            let add = db::nac::NacChange::Add;
            let remove = db::nac::NacChange::Remove;
            manager
                .change_relationship_as(&caller, &admin_change(add, "*"))
                .await
                .unwrap();

            for change in [add, remove] {
                let refused = manager
                    .change_relationship_as(&caller, &admin_change(change, ""))
                    .await
                    .unwrap_err()
                    .to_string();
                assert_eq!(refused, "acp relationship target actor must not be empty");
            }
            assert!(
                manager.is_admin(&test_did2()).await.unwrap(),
                "the wildcard admin is still there"
            );

            let found = manager
                .change_relationship_as(&caller, &admin_change(remove, "not-a-did"))
                .await
                .unwrap();
            assert!(!found, "a target that is no DID holds nothing");
        })
        .unwrap()
    }

    /// A manager over the same stored state, as a restarted node builds one.
    fn restarted(store: &std::sync::Arc<Stored>, config: NacConfig) -> db::NacManager<Stored> {
        db::NacManager::new(std::sync::Arc::clone(store), config)
    }

    /// Go's TestNAC_DisableSuccessfullyThenRestartWithStartArgs_RemainsDisabled:
    /// starting again with the enable option sets NAC up only the first time,
    /// so a disabled NAC stays disabled until an authorized re-enable.
    #[test]
    fn a_disabled_nac_stays_disabled_when_restarted_with_the_enable_option() {
        workers::block_on(async {
            let store = std::sync::Arc::new(acp::PersistentZanzibarStore::from_store(
                std::sync::Arc::new(storage::RegolithStore::in_memory().unwrap()),
            ));
            let owner = test_did();
            let caller = NacCaller {
                identity: Some(owner.clone()),
                node: None,
            };
            let first = restarted(&store, NacConfig::new().with_enabled());
            first.initialize(Some(&owner)).await.unwrap();
            first.disable_as(&caller).await.unwrap();
            drop(first);

            let second = restarted(&store, NacConfig::new().with_enabled());
            second.initialize(Some(&owner)).await.unwrap();
            assert_eq!(second.status().await, NacStatus::DisabledTemporarily);

            second.re_enable_as(&caller).await.unwrap();
            assert_eq!(second.status().await, NacStatus::Enabled);
        })
        .unwrap()
    }

    /// Enabling never turns a disabled NAC back on, nor adds an owner: that
    /// would skip the stored re-enable permission.
    #[test]
    fn enabling_a_disabled_nac_is_refused() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());
            let owner = test_did();
            let stranger = test_did2();
            manager.initialize(Some(&owner)).await.unwrap();
            let caller = NacCaller {
                identity: Some(owner.clone()),
                node: None,
            };
            manager.disable_as(&caller).await.unwrap();

            let refused = manager.enable(&stranger).await.unwrap_err().to_string();
            assert!(refused.contains("re-enable it instead"), "{refused}");
            assert_eq!(manager.status().await, NacStatus::DisabledTemporarily);
            assert!(!manager.is_owner(&stranger).await, "no owner was added");
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_disable_reenable() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());

            let owner = test_did();
            let other = test_did2();
            manager.initialize(Some(&owner)).await.unwrap();

            let caller = NacCaller {
                identity: Some(owner.clone()),
                node: None,
            };
            manager.disable_as(&caller).await.unwrap();
            assert_eq!(manager.status().await, NacStatus::DisabledTemporarily);

            assert!(manager
                .check_permission(&other, NodePermission::DacBypass)
                .await
                .unwrap());

            manager.re_enable_as(&caller).await.unwrap();
            assert_eq!(manager.status().await, NacStatus::Enabled);

            assert!(!manager
                .check_permission(&other, NodePermission::DacBypass)
                .await
                .unwrap());
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_purge_requires_dev_mode() {
        workers::block_on(async {
            let manager = create_memory_nac_manager(NacConfig::new().with_enabled());

            let owner = test_did();
            manager.initialize(Some(&owner)).await.unwrap();

            let result = manager.purge(&owner).await;
            assert!(result.is_err());

            let manager =
                create_memory_nac_manager(NacConfig::new().with_enabled().with_dev_mode());
            manager.initialize(Some(&owner)).await.unwrap();

            let result = manager.purge(&owner).await;
            assert!(result.is_ok());
            assert_eq!(manager.status().await, NacStatus::NotConfigured);
        })
        .unwrap()
    }

    #[test]
    fn test_nac_manager_info() {
        workers::block_on(async {
            let manager =
                create_memory_nac_manager(NacConfig::new().with_enabled().with_dev_mode());

            let owner = test_did();
            manager.initialize(Some(&owner)).await.unwrap();

            let info = manager.info().await;
            assert_eq!(info.status, "enabled");
            assert!(info.configured_enabled);
            assert!(info.dev_mode);
            assert!(info.owner.is_some());
        })
        .unwrap()
    }
}
