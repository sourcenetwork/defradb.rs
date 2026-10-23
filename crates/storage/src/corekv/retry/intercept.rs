//! The intercepting attempt: a decorator that sees every write an
//! engine-owned retry attempt stages, before it is staged.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use super::{RetryAttempt, RetryAttemptFn, RetryAttemptFuture, RetryAttemptIO, RetryTxn};
use crate::corekv::callbacks::TxnCallbacks;
use crate::corekv::errors::{Result, TransactionConflict};
use crate::corekv::iterator::Iterator;
use crate::corekv::traits::{private, MaybeSendSync, Reader, Writer};
use crate::corekv::types::{AppendLog, IterOptions};

/// Sees a write an engine-owned attempt stages, its key and its value (`None`
/// for a delete), before it is staged; an error refuses the write. `Send` and
/// `Sync` on native targets ([`MaybeSendSync`]).
pub trait InterceptWrite: Fn(&[u8], Option<&[u8]>) -> Result<()> + MaybeSendSync {}

impl<F> InterceptWrite for F where F: Fn(&[u8], Option<&[u8]>) -> Result<()> + MaybeSendSync {}

/// Intercepts a write staged by an engine-owned attempt. The interceptor runs
/// before the underlying write is staged.
pub type RetryWriteInterceptor = Arc<dyn InterceptWrite>;

/// Wrap a retry attempt with a safe write interceptor while preserving the
/// backend-owned attempt IO and commit lifecycle. Useful for decorators that
/// inject failures or observe writes without implementing the sealed IO type.
pub fn intercept_retry_writes(
    attempt: RetryAttempt,
    interceptor: RetryWriteInterceptor,
) -> RetryAttempt {
    Box::new(InterceptingRetryAttempt {
        inner: attempt,
        interceptor,
    })
}

struct InterceptingRetryAttempt {
    inner: RetryAttempt,
    interceptor: RetryWriteInterceptor,
}

impl private::Sealed for InterceptingRetryAttempt {}

impl RetryAttemptFn for InterceptingRetryAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut wrapped = InterceptingRetryTxn {
                inner: txn,
                interceptor: Arc::clone(&self.interceptor),
            };
            self.inner.run(&mut wrapped).await
        })
    }
}

struct InterceptingRetryTxn<'a> {
    inner: &'a mut dyn RetryTxn,
    interceptor: RetryWriteInterceptor,
}

impl private::Sealed for InterceptingRetryTxn<'_> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl RetryTxn for InterceptingRetryTxn<'_> {
    fn previous_conflict(&self) -> Option<TransactionConflict> {
        self.inner.previous_conflict()
    }

    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }
    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }
    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        (self.interceptor)(key, Some(value))?;
        self.inner.set(key, value).await
    }
    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        (self.interceptor)(key, None)?;
        self.inner.delete(key).await
    }
    fn begin_attempt_io(&mut self) -> Result<Box<dyn RetryAttemptIO>> {
        Ok(Box::new(InterceptingRetryAttemptIO {
            inner: self.inner.begin_attempt_io()?,
            interceptor: Arc::clone(&self.interceptor),
        }))
    }
    fn finish_attempt_io(
        &mut self,
        io: Box<dyn RetryAttemptIO>,
        callbacks: TxnCallbacks,
    ) -> Result<()> {
        self.inner.finish_attempt_io(io, callbacks)
    }
}

struct InterceptingRetryAttemptIO {
    inner: Box<dyn RetryAttemptIO>,
    interceptor: RetryWriteInterceptor,
}

impl private::Sealed for InterceptingRetryAttemptIO {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for InterceptingRetryAttemptIO {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }
    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.inner.get_parts(key, parts).await
    }
    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }
    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }
    async fn iterator(&self, options: IterOptions) -> Result<Box<dyn Iterator>> {
        self.inner.iterator(options).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for InterceptingRetryAttemptIO {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        (self.interceptor)(key, Some(operand))?;
        self.inner.merge_shared(key, operand).await
    }
    async fn append_shared(
        &self,
        log: &AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        (self.interceptor)(log.head_key(), Some(entry))?;
        self.inner.append_shared(log, entry, once_key).await
    }
    async fn truncate_log_shared(
        &self,
        log: &AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        (self.interceptor)(log.head_key(), None)?;
        self.inner.truncate_log_shared(log, positions).await
    }
    async fn appended(&self, log: &AppendLog) -> Result<Vec<Vec<u8>>> {
        self.inner.appended(log).await
    }
    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        (self.interceptor)(key, Some(value))?;
        self.inner.set_shared(key, value).await
    }
    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        (self.interceptor)(key, None)?;
        self.inner.delete_shared(key).await
    }
}

impl RetryAttemptIO for InterceptingRetryAttemptIO {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self.inner.into_any()
    }
}
