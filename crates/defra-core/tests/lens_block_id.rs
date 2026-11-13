use defra_core::{build_lens_ipld_blocks, LensModuleParts};

fn id(modules: &[LensModuleParts<'_>]) -> String {
    build_lens_ipld_blocks(modules).unwrap().0.to_string()
}

fn module<'module>(
    wasm_bytes: &'module [u8],
    arguments: &'module [(String, String)],
) -> LensModuleParts<'module> {
    LensModuleParts {
        wasm_bytes,
        inverse: false,
        arguments,
    }
}

#[test]
fn an_empty_lens_has_gos_config_block_cid() {
    assert_eq!(
        id(&[]),
        "bafyreiagp6nl5qdyhlqvwgj3l2dvxz6nqte2btv7i7qhhjgatufehkvmka"
    );
}

#[test]
fn every_module_and_argument_value_counts() {
    let wasm = b"\0asm\x01\0\0\0";
    let big = [("value".to_string(), "9007199254740993".to_string())];
    let near = [("value".to_string(), "9007199254740992".to_string())];
    let one = id(&[module(wasm, &big)]);
    assert_ne!(one, id(&[module(wasm, &near)]));
    assert_ne!(one, id(&[module(wasm, &big), module(wasm, &near)]));
    assert_eq!(one, id(&[module(wasm, &big)]));
}
