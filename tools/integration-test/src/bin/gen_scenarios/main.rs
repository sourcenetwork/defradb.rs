//! Generates the Rust conformance tests from the scenario corpus, and checks
//! that exporting them again gives the corpus back.
//!
//!   gen_scenarios generate --scenarios DIR --tests DIR --mantra DIR [--mantra DIR] --drift FILE [--area NAME]...
//!   gen_scenarios check --scenarios DIR --exported DIR [--drift FILE] [--area NAME]...

mod check;
mod corpus;
mod emit;
mod error;
mod lifecycle;
mod literal;
mod mantra;
mod names;
mod not_applying;
mod parallel;
mod text;
mod tree;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use error::{io, GenError, Result};
use rapidhash::RapidHashSet;
use tree::Tree;

/// Go test files that are ported by hand, with their own test file. They are
/// not generated, but stay in the module tree.
const HAND_PORTED: &[&str] = &["tests/integration/collection_version/similarity_test.go"];

#[derive(Default)]
struct Args {
    values: BTreeMap<String, Vec<String>>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Self> {
        let mut args = Args::default();
        let mut iter = raw.iter();
        while let Some(flag) = iter.next() {
            let name = flag
                .strip_prefix("--")
                .ok_or_else(|| GenError::Usage(format!("unexpected argument {flag}")))?;
            let value = iter
                .next()
                .ok_or_else(|| GenError::Usage(format!("--{name} needs a value")))?;
            args.values
                .entry(name.to_owned())
                .or_default()
                .push(value.clone());
        }
        Ok(args)
    }

    fn one(&self, name: &str) -> Result<PathBuf> {
        self.values
            .get(name)
            .and_then(|v| v.first())
            .map(PathBuf::from)
            .ok_or_else(|| GenError::Usage(format!("--{name} is required")))
    }

    fn many(&self, name: &str) -> Vec<String> {
        self.values.get(name).cloned().unwrap_or_default()
    }
}

fn drift(args: &Args) -> Result<RapidHashSet<String>> {
    let Ok(path) = args.one("drift") else {
        return Ok(RapidHashSet::default());
    };
    let text = io(&path, std::fs::read_to_string(&path))?;
    Ok(text.lines().map(|l| l.trim().to_owned()).collect())
}

fn selected(loaded: &corpus::Loaded, areas: &[String]) -> bool {
    areas.is_empty() || areas.iter().any(|a| loaded.dir.first() == Some(a))
}

fn generate(args: &Args) -> Result<()> {
    let tests = args.one("tests")?;
    let leaves = mantra::load(&args.many("mantra"))?;
    let drifted = drift(args)?;
    let areas = args.many("area");
    let all = corpus::load(&args.one("scenarios")?)?;

    let mut skipped_drift: Vec<String> = Vec::new();
    let mut unmapped: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut ops: BTreeMap<String, usize> = BTreeMap::new();
    let mut kept: Vec<Vec<String>> = Vec::new();
    let mut trees: BTreeMap<String, Tree> = BTreeMap::new();
    let mut files: BTreeMap<Vec<String>, Vec<&corpus::Loaded>> = BTreeMap::new();

    for item in all.iter().filter(|l| selected(l, &areas)) {
        let top = item.dir[0].clone();
        if drifted.contains(&item.scenario.source.path) {
            skipped_drift.push(format!(
                "{} ({})",
                item.scenario.id, item.scenario.source.path
            ));
            counts.entry(top).or_default().1 += 1;
            continue;
        }
        if HAND_PORTED.contains(&item.scenario.source.path.as_str()) {
            let modules = names::modules(item);
            if !kept.contains(&modules) {
                kept.push(modules);
            }
            continue;
        }
        counts.entry(top).or_default().0 += 1;
        for step in &item.scenario.steps {
            *ops.entry(step.wire_op()).or_default() += 1;
        }
        files.entry(names::modules(item)).or_default().push(item);
    }

    // A source file named like a directory next to it gets a `_tests` suffix.
    let mut dir_paths: RapidHashSet<Vec<String>> = RapidHashSet::default();
    for path in files.keys() {
        for end in 1..path.len() {
            dir_paths.insert(path[..end].to_vec());
        }
    }
    let mut placed: BTreeMap<Vec<String>, Vec<&corpus::Loaded>> = BTreeMap::new();
    for (mut path, items) in files {
        if dir_paths.contains(&path) {
            if let Some(last) = path.last_mut() {
                last.push_str("_tests");
            }
        }
        placed.entry(path).or_default().extend(items);
    }

    let mut formatted: Vec<PathBuf> = Vec::new();
    for (path, items) in &placed {
        let top = &path[0];
        let bin = names::binary(top);
        trees.entry(bin.clone()).or_default().insert(&path[1..]);
        let mut body = String::new();
        let mut used = emit::Used::default();
        let mut taken: RapidHashSet<String> = RapidHashSet::default();
        for item in items {
            let mut name = emit::function_name(item);
            while !taken.insert(name.clone()) {
                name.push_str("_2");
            }
            let key = mantra::key(&item.scenario.source.path, &item.scenario.source.test);
            let leaf = leaves.get(&key).or_else(|| leaves.get(&item.scenario.id));
            if leaf.is_none() {
                unmapped.push(format!("{} ({key})", item.scenario.id));
            }
            body.push_str(&emit::test_fn(
                item,
                &name,
                leaf.map(|s| s.as_str()),
                &mut used,
            ));
            body.push('\n');
        }
        let header = emit::file_header(items[0], body.contains("m::"), &used, &body);
        let file = tree::file_path(&tests, &bin, path);
        tree::write_file(&file, &format!("{header}{body}"))?;
        formatted.push(file);
    }
    for modules in &kept {
        trees
            .entry(names::binary(&modules[0]))
            .or_default()
            .insert(&modules[1..]);
    }
    for (bin, tree) in &trees {
        let top = tree_top(&placed, bin);
        let directory = (bin != &top).then_some(bin.as_str());
        let root = tests.join(format!("{bin}.rs"));
        tree::write_file(&root, &tree.root_source(&top, directory))?;
    }
    register_binaries(&tests, trees.keys())?;
    for chunk in formatted.chunks(100) {
        let out = io(
            &chunk[0],
            Command::new("rustfmt")
                .args(["--edition", "2021"])
                .args(chunk)
                .output(),
        )?;
        if !out.status.success() {
            return Err(GenError::Rustfmt {
                path: chunk[0].clone(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
    }

    println!("generated by area (generated, drift-skipped):");
    for (area, (generated, drifted)) in &counts {
        println!("  {area}: {generated} {drifted}");
    }
    println!(
        "binaries: {}",
        trees.keys().cloned().collect::<Vec<_>>().join(" ")
    );
    println!("drift-skipped scenarios: {}", skipped_drift.len());
    for line in &skipped_drift {
        println!("  drift: {line}");
    }
    println!("scenarios without a mantra leaf: {}", unmapped.len());
    for line in &unmapped {
        println!("  unmapped: {line}");
    }
    println!("steps by op (corpus names):");
    for (op, count) in &ops {
        println!("  op {op}: {count}");
    }
    Ok(())
}

/// Adds a `[[test]]` entry to the crate's Cargo.toml for each new binary.
fn register_binaries<'a, I>(tests: &std::path::Path, bins: I) -> Result<()>
where
    I: IntoIterator<Item = &'a String>,
{
    let manifest = tests.join("../Cargo.toml");
    let mut text = io(&manifest, std::fs::read_to_string(&manifest))?;
    for bin in bins {
        if !text.contains(&format!("path = \"tests/{bin}.rs\"")) {
            text.push_str(&format!(
                "\n[[test]]\nname = \"{bin}\"\npath = \"tests/{bin}.rs\"\n"
            ));
        }
    }
    io(&manifest, std::fs::write(&manifest, text))
}

fn tree_top(placed: &BTreeMap<Vec<String>, Vec<&corpus::Loaded>>, bin: &str) -> String {
    placed
        .keys()
        .map(|path| path[0].clone())
        .find(|top| names::binary(top) == bin)
        .unwrap_or_else(|| bin.to_owned())
}

fn check(args: &Args) -> Result<()> {
    let scenarios = args.one("scenarios")?;
    let drifted = drift(args)?;
    let areas = args.many("area");
    let loaded: Vec<corpus::Loaded> = corpus::load(&scenarios)?
        .into_iter()
        .filter(|l| {
            selected(l, &areas)
                && !drifted.contains(&l.scenario.source.path)
                && !HAND_PORTED.contains(&l.scenario.source.path.as_str())
        })
        .collect();
    let outcome = check::run(&loaded, &args.one("exported")?, &scenarios)?;
    println!(
        "round trip: {} checked, {} mismatched, {} not exported",
        outcome.checked,
        outcome.mismatched.len(),
        outcome.missing.len()
    );
    for id in &outcome.mismatched {
        println!("  mismatch: {id}");
    }
    for id in &outcome.missing {
        println!("  missing: {id}");
    }
    match outcome.mismatched.is_empty() && outcome.missing.is_empty() {
        true => Ok(()),
        false => Err(GenError::Usage("round trip failed".to_owned())),
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let result = match raw.split_first() {
        Some((command, rest)) => Args::parse(rest).and_then(|args| match command.as_str() {
            "generate" => generate(&args),
            "check" => check(&args),
            other => Err(GenError::Usage(format!("unknown command {other}"))),
        }),
        None => Err(GenError::Usage(
            "usage: gen_scenarios generate|check ...".to_owned(),
        )),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
