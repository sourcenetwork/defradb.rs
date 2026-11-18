use super::*;
use query::mutator::DocMutator;
use std::sync::atomic::{AtomicUsize, Ordering};

struct IdentityKms(AtomicUsize);

#[async_trait::async_trait]
impl kms::KmsService for IdentityKms {
    async fn get_keys(
        &self,
        context: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<kms::KeyResults> {
        assert_eq!(context.user_identity().unwrap().as_str(), "did:key:reader");
        self.0.fetch_add(1, Ordering::SeqCst);
        let (results, sender) = kms::KeyResults::new(cids.len());
        for cid in cids {
            sender.send(Ok((*cid, [7; 32]))).await.unwrap();
        }
        drop(sender);
        Ok(results)
    }

    async fn generate_key(
        &self,
        _: &kms::RequestContext,
        _: kms::KeyScope,
    ) -> kms::Result<(kms::EncryptionCid, [u8; 32])> {
        panic!("historical reads never generate keys")
    }

    async fn serve_request(
        &self,
        _: kms::PeerIdentity,
        _: kms::FetchEncryptionKeyRequest,
    ) -> kms::Result<kms::FetchEncryptionKeyReply> {
        panic!("unexpected peer request")
    }
}

#[test]
fn historical_read_key_miss_exits_attempt_and_is_scoped_to_caller() {
    workers::block_on(async {
        let db = Arc::new(crate::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let kms = Arc::new(IdentityKms(AtomicUsize::new(0)));
        db.set_kms(kms.clone());
        let memo = query::prepared::PreparedInputMemo::default();
        let fetcher = VersionedFetcher::with_kms(
            None,
            Some(kms.clone()),
            Some(identity::Did::new("did:key:reader").unwrap()),
        )
        .with_prepared_inputs(Some(memo.clone()));
        let cid = defra_core::block::generate_cid_from_bytes(b"encryption metadata").unwrap();
        let (ciphertext, _) =
            crypto::encryption::aes::encrypt_aes(b"secret", &[7; 32], &[], true).unwrap();
        let txn = db.new_txn(false).await.unwrap();
        let error = fetcher
            .decrypt_block_data(&txn, &ciphertext, &cid)
            .await
            .unwrap_err();
        let Error::Query(query::error::QueryError::NeedsInput(request)) = error else {
            panic!("expected a typed input miss, got {error:?}");
        };
        assert_eq!(kms.0.load(Ordering::SeqCst), 0);
        txn.discard().unwrap();

        let resolver = crate::AutoCommitMutator::new(db.clone());
        let response = resolver.resolve_prepared_input(&request).await.unwrap();
        memo.insert(request, response);
        assert_eq!(kms.0.load(Ordering::SeqCst), 1);
        let txn = db.new_txn(false).await.unwrap();
        assert_eq!(
            fetcher
                .decrypt_block_data(&txn, &ciphertext, &cid)
                .await
                .unwrap(),
            b"secret"
        );
        assert_eq!(kms.0.load(Ordering::SeqCst), 1);

        let other = VersionedFetcher::with_kms(
            None,
            Some(kms.clone()),
            Some(identity::Did::new("did:key:other").unwrap()),
        )
        .with_prepared_inputs(Some(memo));
        assert!(matches!(
            other.decrypt_block_data(&txn, &ciphertext, &cid).await,
            Err(Error::Query(query::error::QueryError::NeedsInput(_)))
        ));
        assert_eq!(kms.0.load(Ordering::SeqCst), 1);
        txn.discard().unwrap();
    })
    .unwrap()
}
