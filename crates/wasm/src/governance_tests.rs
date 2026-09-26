//! Browser tests for governance: a client created with `governance` judges
//! its own writes by the rule module its collection's version names, on
//! wasmi, before and after a peer runs.

#[cfg(test)]
mod tests {
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::*;

    use crate::bindings::ClientConfig;
    use crate::governance::GovernanceConfig;
    use crate::DefraClient;

    wasm_bindgen_test_configure!(run_in_browser);

    /// `{"verdict":"accept"}`
    const ACCEPT: &str = r#"\a1\67verdict\66accept"#;
    /// `{"verdict":"reject","reason":"forged"}`
    const REJECT: &str = r#"\a2\67verdict\66reject\66reason\66forged"#;

    /// A guest answering `response`, CBOR in WAT string syntax, at every step.
    fn guest(response: &str) -> Vec<u8> {
        // A `\xx` escape is one byte.
        let mut len = 0u32;
        let mut chars = response.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                chars.next();
                chars.next();
            }
            len += 1;
        }
        let prefix: String = len
            .to_le_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect();
        wat::parse_str(format!(
            r#"(module
  (memory (export "memory") 1)
  (data (i32.const 1024) "{prefix}{response}")
  (func (export "alloc") (param i32) (result i32) (i32.const 4096))
  (func (export "judge") (param i32) (param i32) (result i32) (i32.const 1024)))"#
        ))
        .expect("guest WAT")
    }

    fn module_cid(bytes: &[u8]) -> String {
        db::merge::governance::rule::module_cid(bytes).to_string()
    }

    async fn governed(name: &str, modules: Vec<Vec<u8>>) -> DefraClient {
        DefraClient::create(
            serde_wasm_bindgen::to_value(&ClientConfig {
                db_name: Some(name.to_string()),
                governance: Some(GovernanceConfig {
                    collections: vec!["Note".to_string()],
                    rule_modules: modules
                        .into_iter()
                        .map(serde_bytes::ByteBuf::from)
                        .collect(),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn note_schema(client: &mut DefraClient, rule: &str) {
        client
            .add_schema(&format!(
                r#"type Note @governed(root: "arena", rule: "{rule}") {{ text: String }}"#
            ))
            .await
            .unwrap();
    }

    async fn write(client: &DefraClient, text: &str) -> Result<JsValue, String> {
        client
            .mutate(&format!(
                r#"mutation {{ create_Note(input: {{text: "{text}"}}) {{ _docID }} }}"#
            ))
            .await
            .map_err(|error| error.as_string().unwrap_or_default())
    }

    fn no_relays() -> JsValue {
        js_sys::JSON::parse(r#"{"relay_urls":["http://127.0.0.1:9"]}"#).unwrap()
    }

    #[wasm_bindgen_test]
    async fn a_write_the_rule_rejects_is_refused_before_during_and_after_p2p() {
        let reject = guest(REJECT);
        let mut client = governed("governed_reject", vec![reject.clone()]).await;
        note_schema(&mut client, &module_cid(&reject)).await;

        let refused = write(&client, "before").await.unwrap_err();
        assert!(refused.contains("forged"), "{refused}");
        client.start_p2p(no_relays()).await.unwrap();
        assert!(write(&client, "during").await.is_err());
        client.stop_p2p().await.unwrap();
        assert!(
            write(&client, "after").await.is_err(),
            "a write after the peer stopped committed unjudged"
        );
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn a_write_the_rule_accepts_commits() {
        let accept = guest(ACCEPT);
        let mut client = governed("governed_accept", vec![accept.clone()]).await;
        note_schema(&mut client, &module_cid(&accept)).await;
        write(&client, "hello").await.unwrap();
        client.close().await.unwrap();
    }

    /// A rule named but not held defers every write, so a local write is
    /// refused naming it; once the module is put, the write commits.
    #[wasm_bindgen_test]
    async fn a_module_put_later_is_the_rule_from_then_on() {
        let accept = guest(ACCEPT);
        let cid = module_cid(&accept);
        let mut client = governed("governed_put_later", Vec::new()).await;
        note_schema(&mut client, &cid).await;
        let refused = write(&client, "early").await.unwrap_err();
        assert!(refused.contains("not held"), "{refused}");

        assert_eq!(client.put_rule_module(&accept).await.unwrap(), cid);
        write(&client, "late").await.unwrap();

        let status: serde_json::Value =
            serde_wasm_bindgen::from_value(client.governance().await.unwrap()).unwrap();
        assert_eq!(status["collections"], serde_json::json!(["Note"]));
        assert_eq!(status["engine"], "wasmi");
        assert_eq!(status["modules"], serde_json::json!([cid]));
        assert_eq!(status["rules"][0]["rule"], cid.as_str());
        assert_eq!(status["rules"][0]["held"], true);
        assert_eq!(status["sweep"], "local");
        client.close().await.unwrap();
    }

    #[wasm_bindgen_test]
    async fn a_module_that_does_not_compile_is_refused_at_create() {
        let error = DefraClient::create(
            serde_wasm_bindgen::to_value(&ClientConfig {
                db_name: Some("governed_bad_module".to_string()),
                governance: Some(GovernanceConfig {
                    collections: vec!["Note".to_string()],
                    rule_modules: vec![serde_bytes::ByteBuf::from(b"not wasm".to_vec())],
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .err()
        .expect("a module that does not compile was accepted");
        assert!(
            error
                .as_string()
                .unwrap_or_default()
                .contains("does not compile"),
            "{error:?}"
        );
    }

    #[wasm_bindgen_test]
    async fn an_ungoverned_client_reports_nothing_and_holds_no_modules() {
        let mut client = DefraClient::create(JsValue::UNDEFINED).await.unwrap();
        assert!(client.governance().await.unwrap().is_null());
        assert!(client.put_rule_module(&guest(ACCEPT)).await.is_err());
        client.close().await.unwrap();
    }

    /// The sweep holds the database while it runs, which would stop `close`
    /// from closing it exclusively; stopping the sweep lets go of it.
    #[wasm_bindgen_test]
    async fn stopping_the_sweep_lets_go_of_the_database() {
        let mut plain = DefraClient::create(
            serde_wasm_bindgen::to_value(&ClientConfig {
                db_name: Some("governed_refs_plain".to_string()),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap();
        let base = std::sync::Arc::strong_count(plain.ensure_open().unwrap());

        let mut client = governed("governed_refs", vec![guest(ACCEPT)]).await;
        let held = || std::sync::Arc::strong_count(client.ensure_open().unwrap());
        assert_eq!(held(), base + 1, "the sweep does not hold the database");
        client.governance.as_mut().unwrap().stop().await;
        assert_eq!(
            std::sync::Arc::strong_count(client.ensure_open().unwrap()),
            base,
            "the stopped sweep still holds the database"
        );
        client.close().await.unwrap();
        plain.close().await.unwrap();
    }
}
