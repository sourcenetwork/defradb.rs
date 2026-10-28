#[cfg(test)]
mod tests {
    use db::DB;
    use defra_core::RequestContext;
    use storage::RegolithStore;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn bad_sdl_leaves_no_collection() {
        let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();

        let result = db
            .add_collection(
                &RequestContext::default(),
                "type Good { n: Int } type Bad { x: NotAType }",
            )
            .await;

        assert!(result.is_err());
        assert!(db.list_collections().unwrap().is_empty());
    }
}
