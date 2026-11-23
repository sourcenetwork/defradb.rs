use std::{sync::Arc, hint::black_box, time::Instant};
use db::DB;
use document::{Document,NormalValue};
use schema::{CollectionVersion,FieldDescription,FieldKind};
use query::mutator::DocMutator;
use storage::RegolithStore;
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
const COLLECTION: &str = "Users";
const SCHEMA_VERSION_ID: &str = "bafkdocwrite";
const COLLECTION_ID: &str = "docwrite";
const FIELD_COUNTS: [usize; 3] = [4, 16, 64];

fn field_names(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("field_{i}")).collect()
}

fn old(field_count: usize) -> CollectionVersion {
    let mut fields = vec![FieldDescription::new(
        "1",
        "_docID",
        FieldKind::doc_id(),
        schema::CType::LwwRegister,
    )];
    for (index, name) in field_names(field_count).iter().enumerate() {
        fields.push(FieldDescription::new(
            (index + 2).to_string(),
            name,
            FieldKind::string(),
            schema::CType::LwwRegister,
        ));
    }
    CollectionVersion::new(COLLECTION, SCHEMA_VERSION_ID, COLLECTION_ID, fields)
}

fn collection_sdl(field_count: usize) -> String {
    let fields = field_names(field_count)
        .iter()
        .map(|name| format!("{name}: String"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("type {COLLECTION} {{ {fields} }}")
}

fn branchable_collection_sdl(field_count: usize) -> String {
    collection_sdl(field_count).replacen('{', "@branchable {", 1)
}

fn collection_version(field_count: usize, branchable: bool) -> schema::CollectionVersion {
    let sdl = if branchable {
        branchable_collection_sdl(field_count)
    } else {
        collection_sdl(field_count)
    };
    query::parse_sdl(&sdl)
        .expect("the benchmark schema to parse")
        .remove(0)
}

/// A document whose every field is distinct per `seq`, so two documents in one
/// run never collide on a content-addressed id.
fn document(field_count: usize, seq: usize, schema: &schema::CollectionVersion) -> Document {
    let mut doc = Document::new(schema.clone());
    for name in field_names(field_count) {
        doc.set(&name, NormalValue::String(format!("{name}-{seq}")))
            .unwrap();
    }
    doc
}


fn main() {
    let rt = nuclei::Runtime::new().unwrap();
    let fields:usize=std::env::args().nth(1).unwrap_or("4".into()).parse().unwrap();
    let parsed = collection_version(fields,false);
    let old = old(fields);
    eprintln!("version {} vs {}, collection {} vs {}, field {} vs {}",
        old.version_id, parsed.version_id, old.collection_id, parsed.collection_id, old.fields[1].id,parsed.fields[1].id);
    assert!(cid::Cid::try_from(parsed.version_id.as_str()).is_ok());
    let cases=["original","short","long_invalid","version_only","collection_only","fields_only","all_ids","parsed_short_version","parsed_direct","merged"];
    let mut versions=Vec::new();
    for case in cases {
        let mut v=old.clone();
        if case=="long_invalid" {v.version_id="!".repeat(parsed.version_id.len());}
        if matches!(case,"version_only"|"all_ids") {v.version_id=parsed.version_id.clone();}
        if matches!(case,"collection_only"|"all_ids") {v.collection_id=parsed.collection_id.clone();}
        if matches!(case,"fields_only"|"all_ids") {
            for f in &mut v.fields {f.id=parsed.fields.iter().find(|p| p.name==f.name).unwrap().id.clone();}
        }
        if matches!(case,"parsed_direct"|"merged"){v=parsed.clone();}
        if case=="parsed_short_version" { v=parsed.clone(); v.version_id=old.version_id.clone(); }
        versions.push(v);
    }
    // A fresh store for each create, like the checked-in Criterion group.
    // Rotate and reverse order; every case sees the same document values in a round.
    for round in 0..20 {
        for offset in 0..cases.len() {
            let i=if round%2==0 {(offset+round/2)%cases.len()} else {(cases.len()-1-offset+round/2)%cases.len()};
            let mut samples=Vec::new();
            for seq in 1000..1050 {
                let db=Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
                if cases[i]=="merged" {
                    rt.block_on(db.add_collection(&defra_core::RequestContext::ambient(), &collection_sdl(fields))).unwrap();
                } else {
                    rt.block_on(db.create_collection(versions[i].clone())).unwrap();
                }
                if round==0 && seq==1000 {
                    let actual=db.get_collection(COLLECTION).unwrap().unwrap();
                    assert_eq!(actual.version_id(), versions[i].version_id, "unexpected stored version for {}",cases[i]);
                    assert_eq!(actual.collection_id(), versions[i].collection_id, "unexpected stored collection for {}",cases[i]);
                }
                let m=db::write::autocommit::AutoCommitMutator::new(db);
                let start=Instant::now();
                rt.block_on(async {
                    let doc=if cases[i]=="original"{document_original(fields,seq)}else{document(fields,seq,&versions[i])};
                    black_box(m.create(COLLECTION,doc).await.unwrap());
                });
                samples.push(start.elapsed().as_nanos());
            }
            samples.sort_unstable();
            println!("{fields},{round},{},{},{}",cases[i],samples[25],samples.iter().sum::<u128>()/50);
        }
    }
}
fn document_original(field_count: usize, seq: usize) -> Document {
    let mut doc = Document::new(old(field_count));
    for name in field_names(field_count) {
        doc.set(&name, NormalValue::String(format!("{name}-{seq}")))
            .unwrap();
    }
    doc
}

