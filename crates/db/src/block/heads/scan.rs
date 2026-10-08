use async_trait::async_trait;
use storage::corekv::{Error, IterOptions, Iterator, KvPair, Reader, Result};
use storage::keys::headstore::{HeadstoreColKey, HeadstoreColSuperseded};

// DEFRALEVEL(S2): Drop the collection_head_entries branch and Rows shim; keep only the two native prefix iterators.
pub(super) async fn head_iterators<R: Reader + ?Sized>(
    reader: &R,
    collection: u32,
) -> Result<(Box<dyn Iterator>, Box<dyn Iterator>)> {
    if let Some(entries) = reader
        .collection_head_entries(
            &HeadstoreColKey::collection_prefix(collection),
            &HeadstoreColSuperseded::collection_prefix(collection),
        )
        .await?
    {
        return Ok((
            Box::new(Rows::new(entries.heads)),
            Box::new(Rows::new(entries.markers)),
        ));
    }
    let heads = reader
        .iterator(IterOptions::new().with_prefix(HeadstoreColKey::collection_prefix(collection)))
        .await?;
    let markers = reader
        .iterator(
            IterOptions::new()
                .with_prefix(HeadstoreColSuperseded::collection_prefix(collection))
                .with_keys_only(true),
        )
        .await?;
    Ok((heads, markers))
}

struct Rows {
    rows: Vec<KvPair>,
    position: usize,
    closed: bool,
}

impl Rows {
    fn new(rows: Vec<KvPair>) -> Self {
        Self {
            rows,
            position: 0,
            closed: false,
        }
    }
    fn ensure_open(&self) -> Result<()> {
        if self.closed {
            Err(Error::Iterator("iterator is closed".into()))
        } else {
            Ok(())
        }
    }
}
impl storage::corekv::private::Sealed for Rows {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for Rows {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        self.ensure_open()?;
        let row = self.rows.get(self.position).cloned();
        if row.is_some() {
            self.position += 1;
        }
        Ok(row)
    }
    async fn close(&mut self) -> Result<()> {
        self.closed = true;
        self.rows.clear();
        Ok(())
    }
    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        self.ensure_open()?;
        self.position = self.rows.partition_point(|row| row.key.as_slice() < key);
        Ok(self.position < self.rows.len())
    }
    async fn reset(&mut self) -> Result<()> {
        self.ensure_open()?;
        self.position = 0;
        Ok(())
    }
    fn is_valid(&self) -> bool {
        !self.closed
    }
}
