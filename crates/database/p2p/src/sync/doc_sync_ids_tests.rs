use async_trait::async_trait;
use cid::Cid;

use super::*;
use crate::error::Error;

struct Followers;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentHeadProvider for Followers {
    async fn get_document_heads(&self, _doc_id: &str) -> Result<Vec<Cid>> {
        Ok(Vec::new())
    }

    async fn get_collection_heads(&self, _collection_id: &str) -> Result<Vec<Cid>> {
        Ok(Vec::new())
    }

    async fn following_doc_ids(&self, doc_id: &str) -> Result<Vec<String>> {
        match doc_id {
            "book" => Ok((0..5).map(|i| format!("chunk{i}")).collect()),
            "broken" => Err(Error::HeadProvider("lookup failed".into())),
            _ => Ok(Vec::new()),
        }
    }
}

#[test]
fn the_limit_counts_requested_ids_only() {
    assert!(check_doc_id_limit(2, 2).is_ok());
    let Err(Error::InvalidConfig(message)) = check_doc_id_limit(3, 2) else {
        panic!("a request above the limit must be refused");
    };
    assert!(message.contains("3 doc IDs"), "{message}");
    assert!(message.contains("limit of 2"), "{message}");
}

#[nuclei::test]
async fn followers_come_after_the_requested_ids() {
    let requested = vec!["book".to_string(), "plain".to_string()];
    let all = with_followers(&Followers, &requested).await;
    let mut expected = requested.clone();
    expected.extend((0..5).map(|i| format!("chunk{i}")));
    assert_eq!(all, expected);
}

#[nuclei::test]
async fn a_failed_follower_lookup_keeps_the_requested_id() {
    let requested = vec!["broken".to_string(), "book".to_string()];
    let all = with_followers(&Followers, &requested).await;
    assert_eq!(all.len(), 2 + 5);
    assert_eq!(&all[..2], requested.as_slice());
}
