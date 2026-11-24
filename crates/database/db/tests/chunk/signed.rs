//! A signed create's blocks hold the chunk lists its write stores.

use crypto::{Key, PrivateKey};
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use document::NormalValue;
use query::mutator::DocMutator;

use super::sync::{article, blank, book, list_of, modified, setup, text_of, CONFIG};

fn signing() -> RequestContext {
    let key = crypto::generate_ed25519().unwrap();
    let public_key = key.public_key();
    RequestContext::ambient().with_signing(Some(SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: key.raw_owned(),
        public_key_bytes: public_key.raw_owned(),
        public_key_hex: hex::encode(public_key.raw()),
        remote_signer: None,
        signing_authorization: None,
    }))
}

// The shorter list loses a tie with the longer one, so it shows the first
// update landing at the create's priority.
#[test]
fn a_shorter_update_of_a_signed_create_stores_its_list() {
    workers::block_on(async {
        let (mutator, _db, _bus) = setup().await;
        let context = signing();
        let prepared = mutator
            .prepare_write_as(&context, "Article", article("t", &text_of(&book())), None)
            .await
            .unwrap();
        let created = mutator
            .create_as(&context, "Article", prepared)
            .await
            .unwrap();

        let text = text_of(&book()[..1]);
        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("body", NormalValue::String(text.clone())).unwrap();
        mutator
            .update_as(&context, "Article", doc, modified(&["body"]))
            .await
            .unwrap();

        let chunks = db::chunk::split::split(&text, &CONFIG);
        assert_eq!(
            list_of(&mutator, &created.doc_id).await.unwrap(),
            db::chunk::list::chunk_list(&text, &chunks)
        );
    })
    .unwrap()
}
