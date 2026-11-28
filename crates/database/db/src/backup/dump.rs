use storage::corekv::{IterOptions, Reader, Store};
use storage::namespace::Namespace;

use super::BackupError;
use crate::database::DB;

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Go's `PrintDump`: every key of the store, one line each naming its
    /// namespace, the key and its value's size. As in Go, it checks no node
    /// permission; an edge that hands the lines to a caller gates them.
    pub async fn print_dump(&self) -> Result<Vec<String>, BackupError> {
        self.dump_lines().await.map_err(BackupError::Dump)
    }

    async fn dump_lines(&self) -> Result<Vec<String>, String> {
        let store = self.store();
        let txn = store
            .new_txn(true)
            .await
            .map_err(|e| format!("failed to create read txn: {}", e))?;

        let mut lines = Vec::new();

        let namespaces = [
            Namespace::Datastore,
            Namespace::Blockstore,
            Namespace::Headstore,
            Namespace::Systemstore,
            Namespace::Peerstore,
            Namespace::Encstore,
            Namespace::Acpstore,
        ];

        for ns in &namespaces {
            let prefix = vec![ns.prefix()];
            let opts = IterOptions::new().with_prefix(prefix);

            let mut iter = txn
                .iterator(opts)
                .await
                .map_err(|e| format!("iterator error: {}", e))?;

            while let Some(kv) = iter
                .next()
                .await
                .map_err(|e| format!("iteration error: {}", e))?
            {
                let key_display = String::from_utf8_lossy(&kv.key);
                lines.push(format!(
                    "[{:?}] {} ({} bytes)",
                    ns,
                    key_display,
                    kv.value.len()
                ));
            }

            iter.close()
                .await
                .map_err(|e| format!("close error: {}", e))?;
        }

        Ok(lines)
    }
}
