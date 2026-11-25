//! Which views a refresh touches, and which it refuses.
//!
//! The selector precedence itself is `schema::CollectionSelector`; these are the
//! refresh-specific rules layered on top of it.

use db::is_refreshable_view;
use db::CollectionSelector;
use schema::CollectionVersion;
use schema::QuerySource;

/// The collection the views read.
const ORDERS_SDL: &str = "type Orders { total: Int }";

fn orders() -> CollectionVersion {
    query::parse_sdl(ORDERS_SDL).unwrap().remove(0)
}

/// A materialized view named `name` over `Orders`, built as `AddView` builds
/// one: its SDL, then the query it reads.
fn materialized(name: &str) -> CollectionVersion {
    let mut view = query::parse_sdl(&format!("type {name} {{ total: Int }}"))
        .unwrap()
        .remove(0);
    let select = query::parse_query_with_limits(
        "query { Orders { total } }",
        None,
        query::QueryLimits::default(),
    )
    .unwrap()
    .remove(0);
    view.query = Some(QuerySource::new(query::select_to_go_json(&select)));
    view
}

/// A database holding `Orders` and the materialized view `name` over it.
async fn with_view(name: &str) -> db::DB<storage::backends::RegolithStore> {
    let db = db::DB::open(storage::backends::RegolithStore::in_memory().unwrap())
        .await
        .expect("open");
    db.add_collection(&defra_core::RequestContext::ambient(), ORDERS_SDL)
        .await
        .expect("store the source");
    db.create_view_collections_as(
        &defra_core::RequestContext::ambient(),
        vec![materialized(name)],
        None,
        None,
    )
    .await
    .expect("store view");
    db
}

/// Eligibility is separate from selection: a version has to be a materialized,
/// queryable view before any selector is consulted.
#[test]
fn only_materialized_views_are_refreshable() {
    assert!(is_refreshable_view(&materialized("OrdersView")));
    assert!(
        !is_refreshable_view(&orders()),
        "a plain collection is not a view"
    );

    let mut unmaterialized = materialized("OrdersView");
    unmaterialized.is_materialized = false;
    assert!(
        !is_refreshable_view(&unmaterialized),
        "an unmaterialized view has no cache to rebuild"
    );
}

/// Rust excludes embedded-only views where Go does not. That difference is
/// deliberate, because an embedded-only view cannot be queried, so this pins it
/// rather than leaving it to be read as an oversight.
#[test]
fn an_embedded_only_view_is_never_refreshed() {
    let mut embedded = materialized("OrdersView");
    embedded.is_embedded_only = true;
    assert!(!is_refreshable_view(&embedded));

    assert!(
        CollectionSelector::with_names(vec!["OrdersView".to_string()]).selects(&embedded),
        "the selector still matches it; eligibility is what excludes it"
    );
}

/// A selector that matches nothing is a caller mistake, not an empty refresh.
///
/// Go looks a version up directly and propagates not-found
/// (`internal/db/collection.go:211`), so a typo must not read as success. The
/// filter alone returned `Ok(())` over an empty list.
#[test]
fn an_unknown_version_id_is_an_error_not_a_silent_success() {
    workers::block_on(async {
        let db = db::DB::open(storage::backends::RegolithStore::in_memory().unwrap())
            .await
            .expect("open");

        let error = db
            .refresh_views(CollectionSelector {
                version_id: Some("bae-does-not-exist".to_string()),
                ..CollectionSelector::all()
            })
            .await
            .expect_err("an unknown version must be reported");

        assert!(
            error.to_string().contains("bae-does-not-exist"),
            "the error must name the version: {error}"
        );
    })
    .unwrap()
}

#[test]
fn a_name_precedes_an_unknown_version_id() {
    workers::block_on(async {
        let db = with_view("OrdersView").await;

        db.refresh_views(CollectionSelector {
            names: Some(vec!["OrdersView".to_string()]),
            version_id: Some("bae-does-not-exist".to_string()),
            ..CollectionSelector::all()
        })
        .await
        .expect("the version id only filters the active name candidate");
    })
    .unwrap()
}

#[test]
fn an_unknown_collection_id_is_an_error() {
    workers::block_on(async {
        let db = db::DB::open(storage::backends::RegolithStore::in_memory().unwrap())
            .await
            .expect("open");

        let error = db
            .refresh_views(CollectionSelector {
                collection_id: Some("no-such-collection".to_string()),
                ..CollectionSelector::all()
            })
            .await
            .expect_err("an unknown collection must be reported");

        assert!(error.to_string().contains("no-such-collection"), "{error}");
    })
    .unwrap()
}

#[test]
fn get_inactive_is_allowed_when_no_inactive_view_is_selected() {
    workers::block_on(async {
        let db = db::DB::open(storage::backends::RegolithStore::in_memory().unwrap())
            .await
            .expect("open");

        db.refresh_views(CollectionSelector {
            get_inactive: true,
            ..CollectionSelector::all()
        })
        .await
        .expect("including inactive candidates must not fail an active-only selection");
    })
    .unwrap()
}

/// Refusing beats corrupting. `build_view_cache` resolves against the active
/// schemas and matches by name, so refreshing an actually inactive version
/// would clear the shared cache and rebuild it from the active definition.
#[test]
fn refreshing_a_selected_inactive_view_is_refused() {
    workers::block_on(async {
        // A patch makes a second version active and leaves the first inactive.
        let db = with_view("InactiveView").await;
        db.patch_collection(
            &defra_core::RequestContext::default(),
            r#"[{"op":"add","path":"/InactiveView/Fields/-","value":{"Name":"note","Kind":"String"}}]"#,
            None,
        )
        .await
        .expect("patch the view");

        let error = db
            .refresh_views(CollectionSelector {
                get_inactive: true,
                ..CollectionSelector::all()
            })
            .await
            .expect_err("inactive selection must be refused, not silently wrong");

        assert!(
            error.to_string().contains("not supported"),
            "the refusal must say why: {error}"
        );
    })
    .unwrap()
}
