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
    let sdl = collection_sdl(fields);
    let cases=["direct","merged","parse_then_direct","direct_then_parse","merged_then_parse",
        "parse_on_thread","merged_on_thread","direct_on_thread","direct_then_sleep","merged_then_sleep",
        "direct_then_read","merged_then_read","direct_second","merged_second"];
    for round in 0..20 {
        for offset in 0..cases.len() {
            let i=if round%2==0 {(offset+round/2)%cases.len()} else {(cases.len()-1-offset+round/2)%cases.len()};
            let case=cases[i];
            let mut samples=Vec::new();
            for seq in 1000..1025 {
                let db=Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
                let register=|| {
                    if case.starts_with("merged") {
                        rt.block_on(db.add_collection(&defra_core::RequestContext::ambient(), &sdl)).unwrap();
                    } else {
                        let v=if case=="parse_then_direct" { query::parse_sdl(&sdl).unwrap().remove(0) } else { parsed.clone() };
                        rt.block_on(db.create_collection(v)).unwrap();
                    }
                };
                if case.ends_with("_on_thread") && case!="parse_on_thread" {
                    std::thread::scope(|scope|scope.spawn(register).join().unwrap());
                } else {
                    if case=="parse_on_thread" { std::thread::scope(|scope|scope.spawn(||black_box(query::parse_sdl(&sdl).unwrap())).join().unwrap()); }
                    register();
                }
                if case.ends_with("then_parse") {black_box(query::parse_sdl(&sdl).unwrap());}
                if case.ends_with("then_sleep") {std::thread::sleep(std::time::Duration::from_millis(2));}
                if case.ends_with("then_read") { black_box(rt.block_on(db.new_txn(true)).unwrap()); }
                let m=db::write::autocommit::AutoCommitMutator::new(db);
                if case.ends_with("_second"){black_box(rt.block_on(m.create(COLLECTION,document(fields,0,&parsed))).unwrap());}
                let before=usage();
                let start=Instant::now();
                let (enter,make,write,exit)=rt.block_on(async{
                    let enter=start.elapsed();
                    let doc=document(fields,seq,&parsed);
                    let make=start.elapsed();
                    black_box(m.create(COLLECTION,doc).await.unwrap());
                    (enter,make,start.elapsed(),())
                });
                black_box(exit);
                let elapsed=start.elapsed();
                let after=usage();
                let mut row=vec![elapsed.as_nanos() as i64,enter.as_nanos() as i64,(make-enter).as_nanos() as i64,(write-make).as_nanos() as i64,(elapsed-write).as_nanos() as i64,after.0-before.0,after.1-before.1,after.2-before.2]; row.extend(db::write_phases()); samples.push(row);
            }
            let medians=(0..16).map(|col|{let mut x=samples.iter().map(|r|r[col]).collect::<Vec<_>>();x.sort_unstable();x[x.len()/2].to_string()}).collect::<Vec<_>>();
            println!("{fields},{round},{},{}",case,medians.join(","));
        }
    }
}

#[cfg(target_os="linux")]
fn usage()->(i64,i64,i64){
    let mut data=std::mem::MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(unsafe{libc::getrusage(libc::RUSAGE_THREAD,data.as_mut_ptr())},0);
    let v=unsafe{data.assume_init()};
    (v.ru_minflt,v.ru_nvcsw,v.ru_nivcsw)
}
#[cfg(not(target_os="linux"))]
fn usage()->(i64,i64,i64){(0,0,0)}
fn document_original(field_count: usize, seq: usize) -> Document {
    let mut doc = Document::new(old(field_count));
    for name in field_names(field_count) {
        doc.set(&name, NormalValue::String(format!("{name}-{seq}")))
            .unwrap();
    }
    doc
}

