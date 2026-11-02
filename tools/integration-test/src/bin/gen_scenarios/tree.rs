use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::error::{io, Result};

/// The module tree of one test binary, as nested inline `mod` blocks.
#[derive(Default)]
pub struct Tree {
    children: BTreeMap<String, Tree>,
}

impl Tree {
    pub fn insert(&mut self, path: &[String]) {
        if let Some((head, rest)) = path.split_first() {
            self.children.entry(head.clone()).or_default().insert(rest);
        }
    }

    fn write(&self, name: &str, depth: usize, out: &mut String) {
        let pad = "    ".repeat(depth);
        if self.children.is_empty() {
            out.push_str(&format!("{pad}mod {name};\n"));
            return;
        }
        out.push_str(&format!("{pad}mod {name} {{\n"));
        for (child, tree) in &self.children {
            tree.write(child, depth + 1, out);
        }
        out.push_str(&format!("{pad}}}\n"));
    }

    pub fn root_source(&self, top: &str, directory: Option<&str>) -> String {
        let mut out = String::from(concat!(
            "#![recursion_limit = \"4096\"]\n",
            "#![allow(clippy::approx_constant, clippy::needless_update, clippy::module_inception)]\n\n"
        ));
        if let Some(directory) = directory {
            out.push_str(&format!("#[path = \"{directory}\"]\n"));
        }
        self.write(top, 0, &mut out);
        out
    }
}

pub fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        io(parent, std::fs::create_dir_all(parent))?;
    }
    io(path, std::fs::write(path, text))
}

/// `tests/<bin>/<modules[1..]>.rs`.
pub fn file_path(tests: &Path, bin: &str, modules: &[String]) -> PathBuf {
    let mut path = tests.join(bin);
    for module in &modules[1..] {
        path.push(module);
    }
    path.set_extension("rs");
    path
}
