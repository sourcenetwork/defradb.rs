use super::*;

fn delegate(relation: NacRelation) -> DelegationFacts {
    DelegationFacts {
        requestor_admin: false,
        relation,
        holds_operation: true,
        holds_permission: true,
        wildcard_subject: false,
        privileged_subject: false,
        delete: false,
    }
}

#[test]
fn admin_class_is_the_modeled_set() {
    let names: Vec<_> = ADMIN_CLASS.iter().map(NodePermission::as_str).collect();
    assert_eq!(
        names,
        [
            "bypass-dac",
            "add-nac-relation",
            "delete-nac-relation",
            "disable-nac",
            "re-enable-nac",
            "purge-nac",
            "enable-dac",
            "disable-dac",
            "purge-dac"
        ]
    );
}

#[test]
fn delegate_rule_refuses_each_missing_condition() {
    let held = NacRelation::Permission(NodePermission::CollectionPatch);
    assert!(delegate(held).admits());
    assert!(!delegate(NacRelation::Admin).admits());
    assert!(!delegate(NacRelation::Permission(NodePermission::DacBypass)).admits());
    for change in [
        DelegationFacts {
            holds_operation: false,
            ..delegate(held)
        },
        DelegationFacts {
            holds_permission: false,
            ..delegate(held)
        },
        DelegationFacts {
            wildcard_subject: true,
            ..delegate(held)
        },
        DelegationFacts {
            delete: true,
            privileged_subject: true,
            ..delegate(held)
        },
    ] {
        assert!(!change.admits(), "{change:?}");
    }
    assert!(DelegationFacts {
        delete: true,
        ..delegate(held)
    }
    .admits());
    assert!(DelegationFacts {
        requestor_admin: true,
        ..delegate(NacRelation::Admin)
    }
    .admits());
}
