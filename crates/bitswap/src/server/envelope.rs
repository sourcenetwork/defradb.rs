//! One popped batch of tasks becomes at most one outbound message.

use cid::Cid;
use kovan_channel::unbounded;
use libp2p::PeerId;
use tracing::debug;

use super::engine::Report;
use super::peer_task_queue::Task;
use super::task_merger::TaskData;
use crate::message::BitswapMessage;
use crate::network::Network;
use crate::store::Store;

/// A block the envelope has to fetch.
pub(crate) struct BlockWant {
    pub cid: Cid,
    pub send_dont_have: bool,
}

/// Reports the batch done on every exit path, including a panic in the store.
struct DoneGuard {
    reports: unbounded::Sender<Report>,
    peer: PeerId,
    tasks: Vec<Task<Cid, TaskData>>,
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        self.reports.send(Report::TasksDone {
            peer: self.peer,
            tasks: std::mem::take(&mut self.tasks),
        });
    }
}

pub(crate) async fn run<S: Store>(
    store: S,
    network: Network,
    reports: unbounded::Sender<Report>,
    peer: PeerId,
    mut message: BitswapMessage,
    block_wants: Vec<BlockWant>,
    tasks: Vec<Task<Cid, TaskData>>,
) {
    let _done = DoneGuard {
        reports: reports.clone(),
        peer,
        tasks,
    };

    for want in block_wants {
        match store.get(&want.cid).await {
            Ok(block) => message.add_block(block),
            Err(_) if want.send_dont_have => message.add_dont_have(want.cid),
            Err(_) => {}
        }
    }

    if message.is_empty() {
        return;
    }

    reports.send(Report::MessageSent {
        peer,
        blocks: message.blocks().map(|block| block.cid).collect(),
        haves: message.haves().copied().collect(),
    });

    if let Err(err) = network.send_message(peer, message).await {
        debug!(%peer, "failed to send message: {}", err);
    }
}
