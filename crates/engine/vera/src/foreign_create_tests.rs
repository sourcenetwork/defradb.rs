//! Creates on the node's own deployment against the routes being added
//! (`proofs/tla/ForeignPolicyCreate.tla`): a create names its policy before
//! it is sent, and neither a create nor an add of one policy lands beside
//! the other.

use acp::DocumentACP as _;

use super::tests::{acp_over, route, route_at, Answers, ACTOR, OWN, SHARED, YAML};
use crate::provider::{ProviderError, VeraProvider};

/// The node's own deployment creates or adopts no policy routed elsewhere,
/// or being routed, and the refusal comes before anything is sent.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_routed_policy_is_never_created_on_the_nodes_own_deployment() {
    let nonce = acp::PolicyNonce([3; 32]);
    let predicted = crate::vera_rs::predict_policy_id(YAML, nonce, ACTOR).expect("ID");
    let own = Answers::creating(ACTOR);
    let node = acp_over(own.clone());
    route(&node, &predicted, acp_over(Answers::new(Some(true), ""))).await;

    let refused = node.add_policy(ACTOR, YAML, Some(nonce)).await;
    assert!(
        matches!(&refused, Err(ProviderError::PolicyRoutedElsewhere { policy }) if *policy == predicted),
        "{refused:?}"
    );
    assert_eq!(own.created(), 0, "nothing is sent for a routed ID");

    assert!(node.foreign_policies().remove(&predicted));
    let claim = node.foreign_policies().claim(&predicted).expect("claim");
    assert!(
        matches!(
            node.add_policy(ACTOR, YAML, Some(nonce)).await,
            Err(ProviderError::PolicyRoutedElsewhere { .. })
        ),
        "a policy an add is routing now is refused too"
    );
    assert_eq!(own.created(), 0);
    drop(claim);
    assert_eq!(
        node.add_policy(ACTOR, YAML, Some(nonce))
            .await
            .expect("create"),
        predicted
    );
}

/// A create with no nonce is sent with one drawn for it, so its ID is known
/// and checked before anything is sent; while a route holds, such a create
/// makes a policy of another ID, never the routed one.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_create_with_no_nonce_names_its_policy_before_it_is_sent() {
    let own = Answers::creating(ACTOR);
    let node = acp_over(own.clone());
    route(&node, SHARED, acp_over(Answers::new(Some(true), ""))).await;
    let created = node.add_policy(ACTOR, YAML, None).await.expect("create");
    assert_ne!(created, SHARED);
    assert_eq!(own.created(), 1);
    assert!(node.is_foreign_policy(SHARED), "the route still holds");
    assert!(
        own.query_policy(SHARED).await.expect("read").is_none(),
        "the node's own deployment holds no routed policy"
    );
}

/// A create whose ID the provider names only once it is made could make a
/// routed policy: while any policy is routed or being routed it is refused
/// before it is sent, and once none is, it is sent as before.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_create_named_only_once_made_is_refused_while_a_route_holds() {
    let own = Answers::naming_on_create(SHARED);
    let node = acp_over(own.clone());
    route(&node, SHARED, acp_over(Answers::new(Some(true), ""))).await;
    let refused = node.add_policy(ACTOR, YAML, None).await;
    assert!(
        matches!(&refused, Err(ProviderError::UnnamedPolicyCreate)),
        "{refused:?}"
    );
    assert_eq!(own.created(), 0, "nothing is sent");

    assert!(node.foreign_policies().remove(SHARED));
    let claim = node.foreign_policies().claim(OWN).expect("claim");
    assert!(matches!(
        node.add_policy(ACTOR, YAML, None).await,
        Err(ProviderError::UnnamedPolicyCreate)
    ));
    drop(claim);
    assert_eq!(
        node.add_policy(ACTOR, YAML, None).await.expect("create"),
        SHARED
    );
}

/// A route to a policy a create of the node's own deployment is making now
/// is refused, and once the create has ended the route is refused as held:
/// the route never lands beside the create.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_route_is_refused_while_its_policy_is_being_created_and_after() {
    let nonce = acp::PolicyNonce([4; 32]);
    let predicted = crate::vera_rs::predict_policy_id(YAML, nonce, ACTOR).expect("ID");
    let node = acp_over(Answers::creating_slowly(ACTOR));
    let home = acp_over(Answers::new(Some(true), ""));
    let (created, routed) = futures::join!(
        node.add_policy(ACTOR, YAML, Some(nonce)),
        route_at(&node, &predicted, "home", "http://home", home.clone()),
    );
    assert_eq!(created.expect("create"), predicted);
    assert!(
        matches!(&routed, Err(ProviderError::PolicyBeingCreated { policy }) if *policy == predicted),
        "{routed:?}"
    );
    assert!(!node.is_foreign_policy(&predicted));

    let refused = route_at(&node, &predicted, "home", "http://home", home).await;
    assert!(
        matches!(&refused, Err(ProviderError::OwnPolicyRouted { .. })),
        "{refused:?}"
    );
}

/// A route stays refused while a create whose caller failed is still in the
/// provider's journal.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_route_is_refused_while_a_retained_create_can_still_land() {
    let nonce = acp::PolicyNonce([5; 32]);
    let predicted = crate::vera_rs::predict_policy_id(YAML, nonce, ACTOR).expect("ID");
    let node = acp_over(Answers::with_pending_create(&predicted));
    let refused = route_at(
        &node,
        &predicted,
        "home",
        "http://home",
        acp_over(Answers::new(Some(true), "")),
    )
    .await;
    assert!(
        matches!(&refused, Err(ProviderError::PolicyBeingCreated { policy }) if *policy == predicted),
        "{refused:?}"
    );
    assert!(!node.is_foreign_policy(&predicted));
}
