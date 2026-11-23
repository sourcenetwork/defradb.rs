use futures::executor::block_on;
use serde_json::json;

use super::*;
use crate::engine::tests::IDENTITY_PASSTHROUGH_WASM;
use crate::feed::tests::{doc, next_from};
use crate::LensFeed;

fn identity_config() -> LensConfig {
    let module = LensModule {
        module: Some(IDENTITY_PASSTHROUGH_WASM.to_vec()),
        ..Default::default()
    };
    LensConfig::new("v1", "v2", module)
}

#[test]
fn test_wasm_store_creation() {
    let store = WasmTransformStore::new();
    assert!(store.is_ok());
}

#[test]
fn test_wasm_store_with_sandbox() {
    let store = WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive()));
    assert!(store.is_ok());
}

#[test]
fn a_store_builds_its_engine_with_its_first_lens() {
    let store = WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive())).unwrap();
    let local = store.empty_with_shared_engine();
    assert!(block_on(store.list()).unwrap().is_empty());
    assert!(!store.has_transform(&TransformId::new("missing")));
    assert!(
        !store.engine.is_built(),
        "a store with no lens builds no engine"
    );

    block_on(local.add(identity_config())).unwrap();
    assert!(
        store.engine.is_built(),
        "the first lens builds the engine every isolated store shares"
    );
}

#[test]
fn a_store_refuses_limits_it_cannot_run_when_it_is_made() {
    let mut sandbox = WasmSandboxConfig::restrictive();
    sandbox.epoch_tick_period = Some(std::time::Duration::ZERO);
    assert!(matches!(
        WasmTransformStore::with_sandbox(Some(sandbox)),
        Err(Error::InvalidConfig(_))
    ));
    let mut sandbox = WasmSandboxConfig::restrictive();
    sandbox.max_concurrent_calls = Some(0);
    assert!(matches!(
        WasmTransformStore::with_sandbox(Some(sandbox)),
        Err(Error::InvalidConfig(_))
    ));
}

#[test]
fn an_isolated_store_shares_the_engine_and_its_clock() {
    let base = WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive())).unwrap();
    let local = base.empty_with_shared_engine();
    assert!(wasmtime::Engine::same(
        base.engine.engine(),
        local.engine.engine()
    ));
    let clock = base.engine.ticks_handle().unwrap();
    assert!(clock.ptr_eq(&local.engine.ticks_handle().unwrap()));
    drop(base);
    assert!(clock.upgrade().is_some(), "the isolated store keeps it");
    drop(local);
    assert!(clock.upgrade().is_none());
}

#[test]
fn a_call_in_flight_keeps_the_clock_until_it_ends() {
    let store = WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive())).unwrap();
    let id = block_on(store.add(identity_config())).unwrap();
    let clock = store.engine.ticks_handle().unwrap();
    let (input, docs) = futures::channel::mpsc::unbounded::<LensDoc>();
    let mut output = store.transform(&id, Box::pin(docs)).unwrap();

    input.unbounded_send(doc(1)).unwrap();
    assert_eq!(block_on(output.next()).unwrap().unwrap(), doc(1));
    drop(store);
    assert!(clock.upgrade().is_some(), "the call holds the engine");
    drop(input);
    assert!(block_on(output.next()).is_none());
    assert!(clock.upgrade().is_none(), "the ended call let it go");
}

#[test]
fn a_lens_removed_during_a_call_lives_until_the_call_ends() {
    let store = WasmTransformStore::new().unwrap();
    let id = block_on(store.add(identity_config())).unwrap();
    let held = store.modules.get(&id).unwrap();
    let (input, docs) = futures::channel::mpsc::unbounded::<LensDoc>();
    let mut output = store.transform(&id, Box::pin(docs)).unwrap();

    input.unbounded_send(doc(1)).unwrap();
    assert_eq!(block_on(output.next()).unwrap().unwrap(), doc(1));

    block_on(store.remove(&id)).unwrap();
    assert!(!store.has_transform(&id));
    assert!(matches!(
        store.transform(&id, Box::pin(futures::stream::empty())),
        Err(Error::TransformNotFound(_))
    ));

    input.unbounded_send(doc(2)).unwrap();
    assert_eq!(
        block_on(output.next()).unwrap().unwrap(),
        doc(2),
        "the call in flight still runs the removed lens"
    );
    // This test's reference and the call's; the map's own copy goes when
    // the removed entry is reclaimed, which may be later.
    let in_flight = Arc::strong_count(&held);
    assert!(in_flight >= 2, "the call holds the lens");

    drop(input);
    assert!(block_on(output.next()).is_none());
    assert!(
        Arc::strong_count(&held) < in_flight,
        "the ended call let the lens go"
    );
}

#[test]
fn a_document_call_and_a_json_batch_run_the_lens() {
    let store = WasmTransformStore::with_sandbox(Some(WasmSandboxConfig::restrictive())).unwrap();
    let id = block_on(store.add(identity_config())).unwrap();
    assert_eq!(
        block_on(store.transform_doc(&id, doc(7), false)).unwrap(),
        doc(7)
    );
    let values = vec![json!({"n": 1}), serde_json::Value::Null, json!({"n": 2})];
    assert_eq!(
        block_on(store.transform_json(&id, values.clone())).unwrap(),
        values
    );
    assert!(matches!(
        block_on(store.transform_doc(&TransformId::new("missing"), doc(1), false)),
        Err(Error::TransformNotFound(_))
    ));
}

#[test]
fn a_streamed_output_must_be_objects() {
    let store = WasmTransformStore::new().unwrap();
    let id = block_on(store.add(identity_config())).unwrap();
    let output = store
        .engine
        .call(
            store.modules.get(&id).unwrap(),
            futures::stream::iter([serde_json::Value::Null]),
            false,
            None,
        )
        .map(|value| value.and_then(into_doc));
    let outcome = block_on(first_output(output));
    assert!(matches!(outcome, Err(Error::WasmExecution(message)) if message.contains("got null")));
}

#[test]
fn a_feed_pulls_its_source_only_as_the_lens_asks() {
    let store = WasmTransformStore::new().unwrap();
    let id = block_on(store.add(identity_config())).unwrap();
    let mut feed = LensFeed::transform(&store, &id).unwrap();
    let mut pulls = 0;
    let mut source = (0..5).map(doc);

    let first = next_from(&mut feed, &mut source, &mut pulls)
        .unwrap()
        .unwrap();
    assert_eq!(first, doc(0));
    assert_eq!(pulls, 1, "one document in, one out");
    let mut rest = Vec::new();
    while let Some(next) = next_from(&mut feed, &mut source, &mut pulls) {
        rest.push(next.unwrap());
    }
    assert_eq!(rest, (1..5).map(doc).collect::<Vec<_>>());
    assert_eq!(pulls, 6, "five documents and the end");
}

#[test]
fn test_invalid_lens_config() {
    let store = WasmTransformStore::new().unwrap();
    let config = LensConfig::new("v1", "v2", LensModule::default());

    let first_lens = config.lens().cloned().unwrap_or_default();
    let result = store.load_module(&first_lens);
    assert!(matches!(result, Err(Error::InvalidConfig(_))));
}

#[test]
fn test_validate_wasm_path_rejects_relative() {
    let result = WasmTransformStore::validate_wasm_path("relative/path.wasm");
    assert!(matches!(result, Err(Error::PathNotAllowed(_))));
}

#[test]
fn test_validate_wasm_path_rejects_traversal() {
    let result = WasmTransformStore::validate_wasm_path("/safe/../../etc/passwd.wasm");
    assert!(matches!(result, Err(Error::PathNotAllowed(_))));
}

#[test]
fn test_validate_wasm_path_rejects_non_wasm_extension() {
    let result = WasmTransformStore::validate_wasm_path("/path/to/module.so");
    assert!(matches!(result, Err(Error::PathNotAllowed(_))));
}

#[test]
fn test_validate_wasm_path_accepts_valid() {
    let result = WasmTransformStore::validate_wasm_path("/path/to/transform.wasm");
    assert!(result.is_ok());
}

#[test]
fn test_validate_wasm_path_rejects_no_extension() {
    let result = WasmTransformStore::validate_wasm_path("/path/to/module");
    assert!(matches!(result, Err(Error::PathNotAllowed(_))));
}
