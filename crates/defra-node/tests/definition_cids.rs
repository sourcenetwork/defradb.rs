//! Definitions are content-addressed, so a change to how a definition encodes
//! changes every collection ID that peers already share. These SDLs, between
//! them every native CRDT type, relation kind and field flag, keep the IDs
//! they had before custom CRDT types existed (aee81b327).

use defra_node::EmbeddedNode;

const SCHEMAS: &[(&str, &str)] = &[
    (
        "Registers",
        "type Registers { name: String age: Int score: Float64 ok: Boolean at: DateTime raw: Blob tags: [String] }",
    ),
    (
        "Counters",
        r#"type Counters { hits: Int @crdt(type: "pncounter") views: Int @crdt(type: "pcounter") ratio: Float64 @crdt(type: "pncounter") }"#,
    ),
    (
        "Edits",
        r#"type Edits { body: String @crdt(type: "text") meta: JSON @crdt(type: "json") plain: JSON }"#,
    ),
    (
        "Flagged",
        "type Flagged @branchable { owner: String @immutable label: String @index }",
    ),
    (
        "Author",
        "type Author { name: String books: [Book] } type Book { title: String author: Author }",
    ),
];

const GOLDEN: &[(&str, &str, &str)] = &[
    (
        "Registers",
        "bafyreiagwesl72o2reiuzc3l5nx2fy4nr4ui44ift5aztouvqnqyrluha4",
        "bafyreiagwesl72o2reiuzc3l5nx2fy4nr4ui44ift5aztouvqnqyrluha4",
    ),
    (
        "Counters",
        "bafyreihzpcsjapek2n5yrtf7zqjhoauognbj4d4tnaesin5wei7tbyqm3e",
        "bafyreihzpcsjapek2n5yrtf7zqjhoauognbj4d4tnaesin5wei7tbyqm3e",
    ),
    (
        "Edits",
        "bafyreigrdu47o4gvfd6zjphfnrnwbsz43f6h7y2u7brfx7eaw2uuovd7au",
        "bafyreigrdu47o4gvfd6zjphfnrnwbsz43f6h7y2u7brfx7eaw2uuovd7au",
    ),
    (
        "Flagged",
        "bafyreicimjcirob763amtb5woxbaz5ithv3hdagmmk3agz5ks6vgqihkye",
        "bafyreicimjcirob763amtb5woxbaz5ithv3hdagmmk3agz5ks6vgqihkye",
    ),
    (
        "Author",
        "bafyreibvcavbxqwimz5vdxe5q5href63g3skc6ytg45hm4fqh6wsx57wmq",
        "bafyreibvcavbxqwimz5vdxe5q5href63g3skc6ytg45hm4fqh6wsx57wmq",
    ),
    (
        "Book",
        "bafyreieifr5ciohlbzxlrqy4laqdz7nsmnuxzqya44i2d22isyr7nyomja",
        "bafyreieifr5ciohlbzxlrqy4laqdz7nsmnuxzqya44i2d22isyr7nyomja",
    ),
];

#[test]
fn native_definitions_keep_their_ids() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        for (_, sdl) in SCHEMAS {
            node.add_collection(sdl).await.unwrap();
        }
        let mut names: Vec<&str> = SCHEMAS.iter().map(|(name, _)| *name).collect();
        names.push("Book");
        let actual: Vec<(String, String, String)> = names
            .into_iter()
            .map(|name| {
                let collection = node.get_collection(name).unwrap().unwrap();
                (
                    name.to_owned(),
                    collection.collection_id.clone(),
                    collection.version_id.clone(),
                )
            })
            .collect();
        let golden: Vec<(String, String, String)> = GOLDEN
            .iter()
            .map(|(name, id, version)| ((*name).into(), (*id).into(), (*version).into()))
            .collect();
        assert_eq!(actual, golden);
        node.shutdown().await;
    })
    .unwrap()
}
