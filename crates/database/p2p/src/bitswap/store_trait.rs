//! The bitswap `Store` impl a block source gets from its typed reads.
//!
//! A store implements the typed methods `block_size`, `block` and `contains`, each returning p2p's
//! own `Result`, and invokes [`impl_bitswap_store`] to get its `Store` impl.

macro_rules! impl_bitswap_store {
    ([$($generics:tt)*] $ty:ty) => {
        #[async_trait::async_trait]
        impl<$($generics)*> ::bitswap::Store for $ty {
            async fn get_size(&self, cid: &cid::Cid) -> ::bitswap::store::Result<usize> {
                self.block_size(cid)
                    .await
                    .map_err(|error| ::bitswap::StoreError(error.to_string()))
            }

            async fn get(&self, cid: &cid::Cid) -> ::bitswap::store::Result<::bitswap::Block> {
                self.block(cid)
                    .await
                    .map_err(|error| ::bitswap::StoreError(error.to_string()))
            }

            async fn has(&self, cid: &cid::Cid) -> ::bitswap::store::Result<bool> {
                self.contains(cid)
                    .await
                    .map_err(|error| ::bitswap::StoreError(error.to_string()))
            }
        }
    };
}

pub(crate) use impl_bitswap_store;
