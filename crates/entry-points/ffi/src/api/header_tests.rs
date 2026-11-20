//! `defra.h`'s inline wrappers, kept in `cbindgen.toml`'s trailer, against
//! the `_sized` calls this module exports: one wrapper per call, taking the
//! options by value and passing their address and size.

use std::fs;
use std::path::Path;

const SOURCES: &[&str] = &[
    "acp.rs",
    "collection.rs",
    "document.rs",
    "identity.rs",
    "index.rs",
    "node.rs",
    "p2p.rs",
    "query.rs",
    "schema.rs",
];

/// The C spelling of a Rust parameter or return type of a `_sized` call.
fn c_type(rust: &str) -> &'static str {
    match rust {
        "usize" => "uintptr_t",
        "u32" => "uint32_t",
        "i64" => "int64_t",
        "c_int" => "int",
        "*const c_char" => "const char *",
        "*const u8" => "const uint8_t *",
        "DefraResultCallback" => "DefraResultCallback",
        "DefraResult" => "struct DefraResult",
        "DefraNewNodeResult" => "struct DefraNewNodeResult",
        "DefraNewTxnResult" => "struct DefraNewTxnResult",
        "DefraNewIdentityResult" => "struct DefraNewIdentityResult",
        other => panic!("no C spelling for `{other}`: add it here"),
    }
}

/// The options struct's name in C (cbindgen's `[export.rename]`).
fn c_options(rust: &str) -> &str {
    match rust {
        "NodeInitOptions" => "DefraNodeInitOptions",
        other => other,
    }
}

/// The wrapper `defra.h` must hold for one `_sized` signature.
fn wrapper(name: &str, params: &str, ret: Option<&str>) -> String {
    let mut c_params = Vec::new();
    let mut c_args = Vec::new();
    for param in params.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (param, rust) = param.split_once(':').expect("a typed parameter");
        let (param, rust) = (param.trim(), rust.trim());
        match param {
            "opts_size" => c_args.push("(uint32_t)sizeof opts".to_string()),
            "opts" => {
                let options = c_options(rust.trim_start_matches("*const "));
                c_params.push(format!("{options} opts"));
                c_args.push("&opts".to_string());
            }
            _ => {
                let c = c_type(rust);
                let gap = if c.ends_with('*') { "" } else { " " };
                c_params.push(format!("{c}{gap}{param}"));
                c_args.push(param.to_string());
            }
        }
    }
    let (ret, keyword) = match ret {
        Some(ret) => (c_type(ret), "return "),
        None => ("void", ""),
    };
    format!(
        "static inline {ret} {name}({}) {{\n    {keyword}{name}_sized({});\n}}",
        c_params.join(", "),
        c_args.join(", ")
    )
}

/// Every `_sized` call's expected wrapper, in source order.
fn expected() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api");
    let mut wrappers = Vec::new();
    for source in SOURCES {
        let text = fs::read_to_string(dir.join(source)).expect("an api source");
        let marker = "pub unsafe extern \"C\" fn ";
        for (start, _) in text.match_indices(marker) {
            let rest = &text[start + marker.len()..];
            let open = rest.find('(').expect("a parameter list");
            let close = rest.find(')').expect("a parameter list's end");
            let body = rest.find('{').expect("a body");
            let Some(name) = rest[..open].strip_suffix("_sized") else {
                continue;
            };
            let ret = rest[close + 1..body]
                .trim()
                .strip_prefix("->")
                .map(str::trim);
            wrappers.push(wrapper(name, &rest[open + 1..close], ret));
        }
    }
    wrappers
}

/// The trailer `cbindgen.toml` gives the header.
fn trailer() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("cbindgen.toml");
    let config = fs::read_to_string(path)
        .expect("cbindgen.toml")
        .replace("\r\n", "\n");
    let start = config.find("trailer = \"\"\"\n").expect("a trailer") + "trailer = \"\"\"\n".len();
    let end = start + config[start..].find("\"\"\"").expect("the trailer's end");
    config[start..end].trim().to_string()
}

#[test]
fn every_sized_call_has_its_inline_wrapper() {
    let expected = expected().join("\n\n");
    assert_eq!(
        trailer(),
        expected,
        "cbindgen.toml's trailer is out of date; replace it with:\n\n{expected}\n"
    );
}

#[test]
fn the_api_has_seventy_calls_with_options() {
    assert_eq!(expected().len(), 70);
}
