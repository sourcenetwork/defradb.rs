//! Custom CRDT types: a third-party max register registered on the node
//! builder, its contract, its writes and reads, and a node without it.

mod max_register;
mod replication;

use crdt::field::testing::check_field_crdt;
use crdt::RegistryError;
use defra_node::EmbeddedNode;
use document::NormalValue;
use max_register::MaxRegister;
use schema::{CType, CrdtName, FieldDescription, FieldKind};

pub const SDL: &str = r#"type Score { player: String best: Int @crdt(custom: "maxreg") }"#;

fn maxreg() -> CType {
    CType::Custom(CrdtName::new(max_register::NAME).unwrap())
}

#[test]
fn the_max_register_keeps_the_field_crdt_contract() {
    let crdts = crdt::Crdts::new([crdt::Registration::new(MaxRegister::new())]).unwrap();
    let field = FieldDescription::new("1", "best", FieldKind::int(), maxreg());
    let values = [NormalValue::Null, NormalValue::Int(-3), NormalValue::Int(7)];
    let name = CrdtName::new(max_register::NAME).unwrap();
    check_field_crdt(crdts.registered(name).unwrap(), &field, &values, &values);
}

#[test]
fn a_custom_field_keeps_the_greatest_value_written() {
    workers::block_on(async {
        let node = EmbeddedNode::builder()
            .with_crdt(MaxRegister::new())
            .build()
            .await
            .unwrap();
        node.add_collection(SDL).await.unwrap();
        let created = node
            .execute(r#"mutation { add_Score(input: {player: "ann", best: 5}) { _docID best } }"#)
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        for (best, expected) in [(3, 5), (9, 9), (1, 9)] {
            let updated = node
                .execute(&format!(
                    r#"mutation {{ update_Score(input: {{best: {best}}}) {{ best }} }}"#
                ))
                .await;
            assert!(!updated.has_errors(), "{:?}", updated.errors);
            let read = node.execute("{ Score { best } }").await;
            assert_eq!(
                read.data.unwrap()["Score"][0]["best"],
                serde_json::json!(expected),
                "after writing {best}"
            );
        }
        let refused = node
            .execute(r#"mutation { update_Score(input: {best: "high"}) { best } }"#)
            .await;
        assert!(refused.has_errors(), "a string is not a max register value");
        node.shutdown().await;
    })
    .unwrap()
}

#[test]
fn a_node_without_the_type_keeps_the_definition_and_hides_the_field() {
    workers::block_on(async {
        let with_type = EmbeddedNode::builder()
            .with_crdt(MaxRegister::new())
            .build()
            .await
            .unwrap();
        let node = EmbeddedNode::builder().build().await.unwrap();
        node.add_collection(SDL).await.unwrap();
        let field = node
            .get_collection("Score")
            .unwrap()
            .unwrap()
            .field_by_name("best")
            .cloned()
            .unwrap();
        assert_eq!(field.crdt_type, maxreg());

        let written = node
            .execute(r#"mutation { add_Score(input: {player: "ann", best: 5}) { _docID } }"#)
            .await;
        assert!(
            written.has_errors(),
            "the field is not in this node's schema"
        );
        let read = node.execute("{ Score { best } }").await;
        let message = format!("{:?}", read.errors);
        assert!(
            message.contains(r#"Cannot query field \"best\" on type \"Score\""#),
            "{message}"
        );

        let other = node
            .execute(r#"mutation { add_Score(input: {player: "bob"}) { player } }"#)
            .await;
        assert!(!other.has_errors(), "{:?}", other.errors);
        node.shutdown().await;
        with_type.shutdown().await;
    })
    .unwrap()
}

#[test]
fn a_name_registered_twice_fails_the_build() {
    workers::block_on(async {
        let twice = EmbeddedNode::builder()
            .with_crdt(MaxRegister::new())
            .with_crdt(MaxRegister::new())
            .build()
            .await;
        assert!(matches!(
            twice,
            Err(defra_node::Error::CustomCrdt(
                RegistryError::Duplicate { .. }
            ))
        ));

        let first = EmbeddedNode::builder()
            .with_crdt(MaxRegister::new())
            .build()
            .await
            .unwrap();
        let other = EmbeddedNode::builder()
            .with_crdt(MaxRegister::<Impostor>::claiming(maxreg()))
            .build()
            .await
            .expect("each node has its own types, so another node may name one alike");
        let native = EmbeddedNode::builder()
            .with_crdt(MaxRegister::<()>::claiming(CType::LwwRegister))
            .build()
            .await;
        assert!(matches!(
            native,
            Err(defra_node::Error::CustomCrdt(
                RegistryError::NotCustom { .. }
            ))
        ));
        first.shutdown().await;
        other.shutdown().await;
    })
    .unwrap()
}

/// Another Rust type under the max register's name.
struct Impostor;
