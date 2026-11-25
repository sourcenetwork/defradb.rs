use std::path::PathBuf;

use rustc_lint::LintContext;
use rustc_span::{ExpnKind, FileName, MacroKind, Span};

pub(crate) fn generated<Context>(context: &Context, span: Span) -> bool
where
    Context: LintContext,
{
    let mut syntax = span.ctxt();
    while !syntax.is_root() {
        let expansion = syntax.outer_expn_data();
        if !matches!(expansion.kind, ExpnKind::Macro(MacroKind::Bang, _))
            || !expansion
                .macro_def_id
                .is_some_and(|definition| definition.is_local())
        {
            return true;
        }
        syntax = expansion.call_site.ctxt();
    }
    let FileName::Real(file) = context.sess().source_map().span_to_filename(span) else {
        return true;
    };
    let Some(path) = file.local_path() else {
        return true;
    };
    // Included protocol bindings are files rather than macro expansion spans.
    std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .is_some_and(|output| path.starts_with(output))
}

#[cfg(test)]
pub(crate) fn assert_generated_scope() {
    use std::fs;
    use std::process::Command;

    let fixture = tempfile::tempdir().expect("create compiler fixture");
    let output = fixture.path().join("generated");
    fs::create_dir(&output).expect("create build-script output directory");
    let driver_root = std::env::var_os("DYLINT_DRIVER_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("home directory")).join(".dylint_drivers")
        });
    let driver = driver_root
        .join(env!("RUSTUP_TOOLCHAIN"))
        .join(format!("dylint-driver{}", std::env::consts::EXE_SUFFIX));
    let source = "pub struct Item<Value: std::fmt::Debug>(pub Value);\n";
    let root = fixture.path().join("root.rs");
    for (path, generated) in [
        (output.join("bindings.rs"), true),
        (fixture.path().join("authored.rs"), false),
    ] {
        fs::write(&path, source).expect("write compiler fixture");
        fs::write(&root, format!("include!({path:?});\n")).expect("include compiler fixture");
        let result = Command::new(&driver)
            .arg(&root)
            .args([
                "--crate-type=lib",
                "--emit=metadata",
                "--error-format=json",
                "-Dinline_generic_bounds",
            ])
            .arg("--out-dir")
            .arg(fixture.path())
            .env("OUT_DIR", &output)
            .output()
            .expect("run Dylint compiler driver");
        let diagnostics = String::from_utf8_lossy(&result.stderr);
        assert_eq!(result.status.success(), generated, "{diagnostics}");
        assert_eq!(diagnostics.contains("inline_generic_bounds"), !generated);
    }
}
