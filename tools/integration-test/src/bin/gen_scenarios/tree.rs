use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::error::{io, Result};

/// The module tree of one testo area, as nested inline `mod` blocks.
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

    /// The area file `tests/testo/<area>.rs`: the area's module tree, whose
    /// files sit under `tests/testo/<area>/`.
    pub fn root_source(&self) -> String {
        let mut out = String::new();
        for (child, tree) in &self.children {
            tree.write(child, 0, &mut out);
        }
        out
    }
}

pub fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        io(parent, std::fs::create_dir_all(parent))?;
    }
    io(path, std::fs::write(path, text))
}

/// `tests/testo/<area>/<modules[1..]>.rs`.
pub fn file_path(tests: &Path, area: &str, modules: &[String]) -> PathBuf {
    let mut path = tests.join(area);
    for module in &modules[1..] {
        path.push(module);
    }
    path.set_extension("rs");
    path
}
