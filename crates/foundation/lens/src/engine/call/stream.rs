//! A call as the stream its caller polls: the input feed, every module's
//! stage, and the output, driven together by each poll.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use regolith::sync::OwnedSemaphorePermit;
use serde::Serialize;

use super::channel::{channel, ByteReceiver, ByteSender};
use super::stage::{self, Stage};
use super::Pipeline;
use crate::engine::LensEngine;
use crate::{Error, Result};

type Work = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// The documents a call yields, as JSON values. Each poll runs whatever part
/// of the call can make progress: the feed of input documents, the modules,
/// in order, and the output. The call holds its pipeline and its permit
/// until its last module ends, and stops wherever it is when dropped.
pub(crate) struct CallStream {
    work: Option<Work>,
    output: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    permit: Option<OwnedSemaphorePermit>,
    failed: bool,
}

/// Starts a call of `pipeline` over `input`, backwards when `inverse`.
pub(in crate::engine) fn start<S>(
    engine: &LensEngine,
    pipeline: Pipeline,
    input: S,
    inverse: bool,
    permit: Option<OwnedSemaphorePermit>,
) -> CallStream
where
    S: Stream + Send + 'static,
    S::Item: Serialize + Send,
{
    let ticking = engine.ticking();
    let modules = pipeline.len();
    let (feed, mut documents) = channel();
    let mut work: Vec<Work> = Vec::with_capacity(modules + 1);
    work.push(Box::pin(pump(input, feed)));
    for step in 0..modules {
        let (output, next) = channel();
        let stage = Stage {
            pipeline: Arc::clone(&pipeline),
            index: if inverse { modules - 1 - step } else { step },
            inverse,
            last: step + 1 == modules,
        };
        work.push(Box::pin(stage::run(
            engine.clone(),
            stage,
            documents,
            output,
        )));
        documents = next;
    }

    CallStream {
        work: Some(Box::pin(async move {
            ticking?;
            futures::future::try_join_all(work).await.map(drop)
        })),
        output: Box::pin(futures::stream::unfold(documents, receive)),
        permit,
        failed: false,
    }
}

async fn receive(documents: ByteReceiver) -> Option<(Vec<u8>, ByteReceiver)> {
    documents.recv().await.map(|document| (document, documents))
}

/// Feeds the call its input, one document at a time, as JSON.
async fn pump<S>(input: S, feed: ByteSender) -> Result<()>
where
    S: Stream,
    S::Item: Serialize,
{
    let mut input = std::pin::pin!(input);
    while let Some(document) = input.next().await {
        feed.send(serde_json::to_vec(&document)?).await;
    }
    Ok(())
}

impl CallStream {
    /// Drops what runs the call: its stores, its hold on the pipeline and its
    /// permit.
    fn finish(&mut self) {
        self.work = None;
        self.permit = None;
    }
}

impl Stream for CallStream {
    type Item = Result<serde_json::Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        if let Some(work) = this.work.as_mut() {
            if let Poll::Ready(outcome) = work.as_mut().poll(cx) {
                this.finish();
                if let Err(error) = outcome {
                    this.failed = true;
                    return Poll::Ready(Some(Err(error)));
                }
            }
        }
        if this.failed {
            return Poll::Ready(None);
        }
        match this.output.as_mut().poll_next(cx) {
            Poll::Ready(Some(json)) => Poll::Ready(Some(
                serde_json::from_slice(&json).map_err(|e| Error::WasmExecution(e.to_string())),
            )),
            Poll::Ready(None) => {
                this.finish();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
