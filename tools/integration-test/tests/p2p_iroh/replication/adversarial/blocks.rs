//! Every hostile or honest commit the adversarial tests push, built outside
//! any node from the production block types.
//!
//! This is the only place that builds, signs, encodes or tampers blocks. Tests
//! ask for a scenario and push the `Fragment` they get back.

use std::collections::HashMap;

use cid::Cid;
use crypto::keys::PrivateKey;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload, Signature, SignatureHeader,
    SignatureType,
};
use document::{DocID, NormalValue};

pub struct Author {
    key: crypto::Secp256k1PrivateKey,
    pub private_key_hex: String,
    pub public_key_hex: String,
    pub did: String,
}

pub fn author(seed: u8) -> Author {
    let secret = [seed; 32];
    let key = crypto::Secp256k1PrivateKey::from_bytes(&secret).expect("a valid scalar");
    let public_key_hex = hex::encode(key.public_key().raw());
    let did = crypto::public_key_from_string(crypto::KeyType::Secp256k1, &public_key_hex)
        .expect("public key parses")
        .did()
        .expect("public key has a DID")
        .to_string();
    Author {
        key,
        private_key_hex: hex::encode(secret),
        public_key_hex,
        did,
    }
}

#[derive(Clone, Copy)]
pub enum Signing {
    Unsigned,
    CompositeOnly,
    EveryBlock,
}

const ACTIVE: u8 = 1;
const DELETED: u8 = 2;

pub struct Fields(Vec<(&'static str, NormalValue)>);

pub fn user(name: &str, age: i64) -> Fields {
    Fields(vec![
        ("name", NormalValue::String(name.into())),
        ("age", NormalValue::Int(age)),
    ])
}

pub fn named(name: &str) -> Fields {
    Fields(vec![("name", NormalValue::String(name.into()))])
}

pub fn aged(age: i64) -> Fields {
    Fields(vec![("age", NormalValue::Int(age))])
}

/// One commit as it goes on the wire: the blocks, children before the root,
/// and the document ID and creator the PushLog envelope claims for them.
#[derive(Clone)]
pub struct Fragment {
    pub doc_id: String,
    pub creator: String,
    pub root: Cid,
    pub blocks: Vec<(Cid, Vec<u8>)>,
}

/// The heads an update extends: a document's latest composite, the latest
/// block of each field, and the composite's height.
pub struct Parent {
    doc_id: String,
    composite: Cid,
    fields: HashMap<String, Cid>,
    height: u64,
}

pub fn genesis_parent(doc_id: &str, composite: Cid, fields: HashMap<String, Cid>) -> Parent {
    Parent {
        doc_id: doc_id.to_string(),
        composite,
        fields,
        height: 1,
    }
}

/// The heads a genesis fragment leaves once merged.
pub fn genesis_parent_of(genesis: &Fragment) -> Parent {
    genesis_parent(&genesis.doc_id, genesis.root, field_cids(genesis))
}

/// The heads `parent` has once `fragment`, an update extending it, is merged.
pub fn child_of(parent: &Parent, fragment: &Fragment) -> Parent {
    let mut fields = parent.fields.clone();
    fields.extend(field_cids(fragment));
    Parent {
        doc_id: parent.doc_id.clone(),
        composite: fragment.root,
        fields,
        height: parent.height + 1,
    }
}

pub fn genesis(fields: &Fields, version_id: &str, signer: &Author, signing: Signing) -> Fragment {
    commit(fields, version_id, signer, signing, None, ACTIVE)
}

pub fn update(
    parent: &Parent,
    fields: &Fields,
    version_id: &str,
    signer: &Author,
    signing: Signing,
) -> Fragment {
    commit(fields, version_id, signer, signing, Some(parent), ACTIVE)
}

/// A signed delete: a composite with the deleted status and no field links.
pub fn delete(parent: &Parent, version_id: &str, signer: &Author) -> Fragment {
    commit(
        &Fields(Vec::new()),
        version_id,
        signer,
        Signing::EveryBlock,
        Some(parent),
        DELETED,
    )
}

/// A genesis whose root links a signature `impostor` made over other content.
/// The root is relinked, so every block still hashes to its CID and only
/// signature verification can tell.
pub fn forged_signature(
    fields: &Fields,
    version_id: &str,
    signer: &Author,
    impostor: &Author,
) -> Fragment {
    let genuine = genesis(fields, version_id, signer, Signing::CompositeOnly);
    let decoy = genesis(
        &named("decoy"),
        version_id,
        impostor,
        Signing::CompositeOnly,
    );
    let decoy_signature = root_block(&decoy).signature.expect("decoy is signed");
    let genuine_root = root_block(&genuine);
    let genuine_signature = genuine_root.signature.expect("genuine is signed");

    let mut forged_root = genuine_root;
    forged_root.signature = Some(decoy_signature);
    let (root, root_bytes) = encode(&forged_root);

    let mut blocks: Vec<(Cid, Vec<u8>)> = genuine
        .blocks
        .iter()
        .filter(|(cid, _)| *cid != genuine.root && *cid != genuine_signature)
        .cloned()
        .collect();
    blocks.push((decoy_signature, bytes_of(&decoy, &decoy_signature).to_vec()));
    blocks.push((root, root_bytes));

    Fragment {
        doc_id: DocID::new_v0(root).to_string(),
        creator: signer.did.clone(),
        root,
        blocks,
    }
}

/// `fragment` with one integer field block's value changed and its original
/// CID kept. Returns the fragment and the CID now advertised over wrong bytes.
pub fn tampered_field(fragment: &Fragment, field: &str, value: i64) -> (Fragment, Cid) {
    let mut tampered = fragment.clone();
    let (cid, bytes) = tampered
        .blocks
        .iter_mut()
        .find(|(_, bytes)| {
            Block::from_dag_cbor(bytes).is_ok_and(
                |block| matches!(&block.delta, CrdtDelta::Lww(p) if p.field_name == field),
            )
        })
        .expect("the fragment has the field");
    let mut block = Block::from_dag_cbor(bytes).expect("field decodes");
    if let CrdtDelta::Lww(payload) = &mut block.delta {
        payload.data = cbor(&NormalValue::Int(value));
    }
    *bytes = block.to_dag_cbor().expect("field encodes");
    let cid = *cid;
    (tampered, cid)
}

pub fn with_doc_id(fragment: &Fragment, doc_id: &str) -> Fragment {
    Fragment {
        doc_id: doc_id.to_string(),
        ..fragment.clone()
    }
}

pub fn with_creator(fragment: &Fragment, did: &str) -> Fragment {
    Fragment {
        creator: did.to_string(),
        ..fragment.clone()
    }
}

fn commit(
    fields: &Fields,
    version_id: &str,
    signer: &Author,
    signing: Signing,
    parent: Option<&Parent>,
    status: u8,
) -> Fragment {
    let priority = parent.map_or(1, |parent| parent.height + 1);
    let mut blocks = Vec::new();
    let mut links = Vec::new();

    for (name, value) in &fields.0 {
        let heads = parent
            .and_then(|parent| parent.fields.get(*name))
            .map(|head| vec![*head])
            .unwrap_or_default();
        let mut field = Block::new(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: (*name).to_string(),
                priority,
                schema_version_id: version_id.to_string(),
                data: cbor(value),
            }),
            heads,
            vec![],
        );
        if matches!(signing, Signing::EveryBlock) {
            let (sig_cid, sig_bytes) = sign(&field, &signer.key);
            blocks.push((sig_cid, sig_bytes));
            field.signature = Some(sig_cid);
        }
        let (cid, bytes) = encode(&field);
        links.push(DAGLink::new(*name, cid));
        blocks.push((cid, bytes));
    }

    let mut composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: version_id.to_string(),
            priority,
            status,
        }),
        parent
            .map(|parent| vec![parent.composite])
            .unwrap_or_default(),
        links,
    );
    if !matches!(signing, Signing::Unsigned) {
        let (sig_cid, sig_bytes) = sign(&composite, &signer.key);
        blocks.push((sig_cid, sig_bytes));
        composite.signature = Some(sig_cid);
    }
    let (root, root_bytes) = encode(&composite);
    blocks.push((root, root_bytes));

    Fragment {
        doc_id: parent.map_or_else(
            || DocID::new_v0(root).to_string(),
            |parent| parent.doc_id.clone(),
        ),
        creator: signer.did.clone(),
        root,
        blocks,
    }
}

fn field_cids(fragment: &Fragment) -> HashMap<String, Cid> {
    fragment
        .blocks
        .iter()
        .filter_map(
            |(cid, bytes)| match Block::from_dag_cbor(bytes).ok()?.delta {
                CrdtDelta::Lww(payload) => Some((payload.field_name, *cid)),
                _ => None,
            },
        )
        .collect()
}

fn cbor(value: &NormalValue) -> Vec<u8> {
    let mut data = Vec::new();
    ciborium::into_writer(value, &mut data).expect("value encodes");
    data
}

fn encode(block: &Block) -> (Cid, Vec<u8>) {
    let bytes = block.to_dag_cbor().expect("block encodes");
    (
        defra_core::block::generate_cid_from_bytes(&bytes).expect("block CID"),
        bytes,
    )
}

/// The preimage is the block without its own signature link, which is what
/// the receiving node reconstructs to verify.
fn sign(block: &Block, key: &crypto::Secp256k1PrivateKey) -> (Cid, Vec<u8>) {
    let mut unsigned = block.clone();
    unsigned.signature = None;
    let preimage = unsigned.to_dag_cbor().expect("preimage encodes");
    let signature = Signature::new(
        SignatureHeader::new(
            SignatureType::ES256K,
            hex::encode(key.public_key().raw()).into_bytes(),
        ),
        key.sign(&preimage).expect("signs"),
    );
    let bytes = signature.to_dag_cbor().expect("signature encodes");
    (
        defra_core::block::generate_cid_from_bytes(&bytes).expect("signature CID"),
        bytes,
    )
}

fn root_block(fragment: &Fragment) -> Block {
    Block::from_dag_cbor(bytes_of(fragment, &fragment.root)).expect("root decodes")
}

fn bytes_of<'a>(fragment: &'a Fragment, cid: &Cid) -> &'a [u8] {
    fragment
        .blocks
        .iter()
        .find(|(candidate, _)| candidate == cid)
        .map(|(_, bytes)| bytes.as_slice())
        .expect("the block is in the fragment")
}
