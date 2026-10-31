//! Tests for issue #859: collection patching, schema migration, and
//! introspection exposed through [`Node`].

use defra_core::RequestContext;
use defra_node::Node;
use defra_node::Result;
#[cfg(feature = "wasmtime-runtime")]
use defra_node::{LensConfig, LensModule};
use schema::CollectionVersion;

const USER_SDL: &str = r#"
type User {
    name: String
    age: Int
}
"#;

// Minimal valid WebAssembly module — just the 4-byte `\0asm` magic plus the
// 4-byte version (`1`). Wasmtime accepts this as an empty module, which is
// all we need to prove [`Node::set_migration`] wires through to the
// underlying lens store.
#[cfg(feature = "wasmtime-runtime")]
const EMPTY_WASM: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

/// Apply `patch` to `User` through the node and return its active version.
async fn patch_user(node: &Node, patch: &str) -> Result<CollectionVersion> {
    node.patch_collection_as(&RequestContext::ambient(), patch, None)
        .await?;
    Ok(node
        .get_collection("User")?
        .expect("User exists after a patch"))
}

#[test]
fn list_collections_reports_added_schema() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;

        assert!(node.list_collections()?.is_empty());

        node.add_collection(USER_SDL).await?;

        let names = node.list_collections()?;
        assert_eq!(names, vec!["User".to_string()]);
        Ok(())
    })
    .unwrap()
}

#[test]
fn get_collection_returns_active_schema() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let collection = node
            .get_collection("User")?
            .expect("User collection should exist");
        assert_eq!(collection.name, "User");
        assert!(collection.is_active);
        assert!(collection.fields.iter().any(|f| f.name == "name"));
        assert!(collection.fields.iter().any(|f| f.name == "age"));

        assert!(node.get_collection("NotAType")?.is_none());
        Ok(())
    })
    .unwrap()
}

#[test]
fn patch_collection_adds_field_and_bumps_version() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let original = node
            .get_collection("User")?
            .expect("User should exist after add_collection");

        let patch = r#"[
        {"op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": "String"}}
    ]"#;
        let patched = patch_user(&node, patch).await?;

        assert_ne!(patched.version_id, original.version_id);
        assert!(patched.fields.iter().any(|f| f.name == "email"));

        let current = node
            .get_collection("User")?
            .expect("User should still exist after patch");
        assert_eq!(current.version_id, patched.version_id);
        assert!(current.fields.iter().any(|f| f.name == "email"));

        Ok(())
    })
    .unwrap()
}

#[test]
fn patch_collection_chained_evolutions_produce_distinct_versions() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let v0 = node
            .get_collection("User")?
            .expect("User should exist")
            .version_id;

        let v1 = patch_user(
            &node,
            r#"[{"op":"add","path":"/User/Fields/-","value":{"Name":"email","Kind":"String"}}]"#,
        )
        .await?
        .version_id;
        let v2 = patch_user(
            &node,
            r#"[{"op":"add","path":"/User/Fields/-","value":{"Name":"phone","Kind":"String"}}]"#,
        )
        .await?
        .version_id;

        assert_ne!(v0, v1);
        assert_ne!(v1, v2);
        assert_ne!(v0, v2);

        Ok(())
    })
    .unwrap()
}

#[test]
fn get_collection_by_version_id_finds_prior_and_new() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let original_version = node
            .get_collection("User")?
            .expect("User should exist")
            .version_id;

        let patch = r#"[
        {"op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": "String"}}
    ]"#;
        let patched = patch_user(&node, patch).await?;

        let by_new = node
            .get_collection_by_version_id(&patched.version_id)
            .await?
            .expect("new version should be retrievable");
        assert_eq!(by_new.version_id, patched.version_id);

        let by_old = node
            .get_collection_by_version_id(&original_version)
            .await?
            .expect("old version should still be retrievable");
        assert_eq!(by_old.version_id, original_version);

        Ok(())
    })
    .unwrap()
}

#[test]
fn get_all_collection_versions_returns_history() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let initial = node.get_all_collection_versions().await?;
        assert_eq!(initial.len(), 1);

        let patch = r#"[
        {"op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": "String"}}
    ]"#;
        patch_user(&node, patch).await?;

        let after_patch = node.get_all_collection_versions().await?;
        assert!(
            after_patch.len() >= 2,
            "expected at least two versions after patching, got {}",
            after_patch.len()
        );
        Ok(())
    })
    .unwrap()
}

#[test]
fn set_active_collection_version_round_trips_to_original() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let original_version = node
            .get_collection("User")?
            .expect("User should exist")
            .version_id;

        let patch = r#"[
        {"op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": "String"}}
    ]"#;
        let patched = patch_user(&node, patch).await?;
        assert_ne!(patched.version_id, original_version);
        assert_eq!(
            node.get_collection("User")?
                .expect("User should exist")
                .version_id,
            patched.version_id,
        );

        node.set_active_collection_version_as(&RequestContext::ambient(), &original_version)
            .await?;
        assert_eq!(
            node.get_collection("User")?
                .expect("User should exist")
                .version_id,
            original_version,
        );

        Ok(())
    })
    .unwrap()
}

#[cfg(feature = "wasmtime-runtime")]
#[test]
fn set_migration_registers_lens_between_versions() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let original_version = node
            .get_collection("User")?
            .expect("User should exist")
            .version_id;

        let patch = r#"[
        {"op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": "String"}}
    ]"#;
        let patched = patch_user(&node, patch).await?;

        let module = LensModule::from_bytes(EMPTY_WASM.to_vec());
        let config = LensConfig::new(&original_version, &patched.version_id, module);

        let config = serde_json::to_value(config).expect("a lens config serializes");
        let transform_id = node
            .set_migration_as(&RequestContext::ambient(), &config)
            .await?;
        assert!(
            !transform_id.is_empty(),
            "transform id should be a non-empty CID"
        );
        Ok(())
    })
    .unwrap()
}

#[test]
fn materialize_collection_is_exposed_on_node() -> Result<()> {
    workers::block_on(async {
        let node = Node::builder().build().await?;
        node.add_collection(USER_SDL).await?;

        let patch =
            r#"[{"op":"add","path":"/User/Fields/-","value":{"Name":"email","Kind":"String"}}]"#;
        patch_user(&node, patch).await?;

        assert_eq!(node.materialize_collection("User").await?, 0);
        Ok(())
    })
    .unwrap()
}

#[test]
fn introspection_supports_idempotent_schema_ensure() -> Result<()> {
    workers::block_on(async {
        // Demonstrates the idempotent bootstrap pattern the issue calls out:
        // the application checks whether a collection exists before falling
        // back to add_collection. Running twice must not error.
        let node = Node::builder().build().await?;

        ensure_user_schema(&node).await?;
        ensure_user_schema(&node).await?;

        assert_eq!(node.list_collections()?, vec!["User".to_string()]);
        Ok(())
    })
    .unwrap()
}

async fn ensure_user_schema(node: &Node) -> Result<()> {
    if node.get_collection("User")?.is_none() {
        node.add_collection(USER_SDL).await?;
    }
    Ok(())
}
