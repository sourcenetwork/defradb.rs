//! Collects the registry's family files (`src/registry/*.rs`) into
//! `OUT_DIR/registry_families.rs`: one `mod` per family file, and `FAMILIES`,
//! the files in name order, each a `Family` of its `CRATES` and
//! `PROPERTIES`. A family file dropped into `src/registry/` is picked up
//! with no other edit here; `matrix::every_family_names_its_crates` makes
//! sure it declares its crates.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("src")
        .join("registry");
    // A directory, not each file: on a warm build Cargo rescans a directory's
    // contents, so a family file added later still reruns this script.
    println!("cargo:rerun-if-changed={}", dir.display());

    // Family files in name order: the mod declarations, FAMILIES and the
    // iteration in `registry::properties` all use the same order, so the
    // registry table's order stays reproducible.
    let mut stems: Vec<String> = Vec::new();
    for entry in fs::read_dir(&dir).expect("src/registry is a readable directory") {
        let path = entry.expect("src/registry entries are readable").path();
        let Some(stem) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|_| path.extension().is_some_and(|ext| ext == "rs"))
            .map(str::to_owned)
        else {
            continue; // skip non-Rust files rather than panic: drop-in only.
        };
        stems.push(stem);
    }
    stems.sort();

    let mut text = String::new();
    let mut families = String::new();
    for stem in &stems {
        let file = dir.join(format!("{stem}.rs"));
        text.push_str(&format!(
            "#[path = {:?}]\nmod {stem};\n",
            file.canonicalize().expect("the registry file exists")
        ));
        families.push_str(&format!(
            "    Family {{ crates: {stem}::CRATES, properties: {stem}::PROPERTIES }},\n"
        ));
    }

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("registry_families.rs");
    text.push_str("\npub const FAMILIES: &[Family] = &[\n");
    text.push_str(&families);
    text.push_str("];\n");
    fs::write(out, text).expect("registry_families.rs is writable");
}
