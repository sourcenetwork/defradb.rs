//! The decision engine: the owner task of the task queue and the per-peer ledgers.
//!
//! Every mutation happens here, one command at a time. Each 100 ms tick pops at most one message worth
//! of tasks per free worker slot; an envelope task turns a popped batch into at most one message.

use std::time::Duration;

use cid::Cid;
use kovan_channel::unbounded;
use libp2p::PeerId;
use rapidhash::RapidHashMap;
use tokio::sync::oneshot;
use tokio::time::{interval_at, Instant};
use tracing::debug;

use super::config::ServerConfig;
use super::envelope::{self, BlockWant};
use super::ledger::Ledger;
use super::peer_task_queue::{Config as QueueConfig, PeerTaskQueue, Task};
use super::task_merger::{TaskData, TaskMerger};
use crate::message::{BitswapMessage, BlockPresence, Entry, WantType};
use crate::network::Network;
use crate::store::Store;

const TICK: Duration = Duration::from_millis(100);

/// A want with the block size the receive stage found for it.
pub(crate) struct Want {
    pub entry: Entry,
    pub size: Option<usize>,
}

/// Everything one inbound message contributes.
pub(crate) struct Received {
    pub peer: PeerId,
    pub full: bool,
    pub cancels: Vec<Cid>,
    pub denials: Vec<Entry>,
    pub wants: Vec<Want>,
}

/// Input from outside the engine.
pub(crate) enum Command {
    Received(Received),
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    LedgerWants(PeerId, oneshot::Sender<Option<usize>>),
}

/// Progress reports from envelope tasks.
pub(crate) enum Report {
    MessageSent {
        peer: PeerId,
        blocks: Vec<Cid>,
        haves: Vec<Cid>,
    },
    TasksDone {
        peer: PeerId,
        tasks: Vec<Task<Cid, TaskData>>,
    },
}

pub(crate) struct Engine<S: Store> {
    queue: PeerTaskQueue<Cid, TaskData, TaskMerger>,
    ledgers: RapidHashMap<PeerId, Ledger>,
    send_dont_haves: bool,
    max_replace_size: usize,
    max_queued_wants: usize,
    target_message_size: usize,
    worker_count: usize,
    in_flight: usize,
    store: S,
    network: Network,
    reports: unbounded::Sender<Report>,
}

impl<S: Store> Engine<S> {
    pub(crate) fn new(
        config: &ServerConfig,
        store: S,
        network: Network,
        reports: unbounded::Sender<Report>,
    ) -> Self {
        Engine {
            queue: PeerTaskQueue::new(
                TaskMerger::default(),
                QueueConfig {
                    max_outstanding_work_per_peer: config.max_outstanding_bytes_per_peer,
                    ignore_freezing: true,
                },
            ),
            ledgers: Default::default(),
            send_dont_haves: config.send_dont_haves,
            max_replace_size: config.max_replace_size,
            max_queued_wants: config.max_queued_wantlist_entries_per_peer,
            target_message_size: config.target_message_size,
            worker_count: config.worker_count,
            in_flight: 0,
            store,
            network,
            reports,
        }
    }

    /// Runs until every command sender is gone.
    pub(crate) async fn run(
        mut self,
        commands: unbounded::Receiver<Command>,
        reports: unbounded::Receiver<Report>,
    ) {
        let mut ticker = interval_at(Instant::now() + TICK, TICK);
        loop {
            tokio::select! {
                command = commands.recv_async() => match command {
                    Some(command) => self.handle_command(command),
                    None => return,
                },
                Some(report) = reports.recv_async() => self.handle_report(report),
                _ = ticker.tick() => self.dispatch(),
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Received(received) => self.message_received(received),
            Command::PeerConnected(peer) => {
                self.ledgers
                    .entry(peer)
                    .or_insert_with(|| Ledger::new(peer));
            }
            Command::PeerDisconnected(peer) => {
                self.ledgers.remove(&peer);
            }
            Command::LedgerWants(peer, reply) => {
                reply
                    .send(self.ledgers.get(&peer).map(|l| l.wantlist().len()))
                    .ok();
            }
        }
    }

    fn handle_report(&mut self, report: Report) {
        match report {
            Report::MessageSent {
                peer,
                blocks,
                haves,
            } => {
                if let Some(ledger) = self.ledgers.get_mut(&peer) {
                    for cid in &blocks {
                        ledger.wantlist_mut().remove_type(cid, WantType::Block);
                    }
                    for cid in &haves {
                        ledger.wantlist_mut().remove_type(cid, WantType::Have);
                    }
                }
            }
            Report::TasksDone { peer, tasks } => {
                self.queue.tasks_done(peer, &tasks);
                self.in_flight = self.in_flight.saturating_sub(1);
            }
        }
    }

    fn send_as_block(&self, want_type: WantType, block_size: usize) -> bool {
        want_type == WantType::Block || block_size <= self.max_replace_size
    }

    fn dont_have_task(&self, entry: &Entry) -> Option<Task<Cid, TaskData>> {
        (self.send_dont_haves && entry.send_dont_have).then(|| Task {
            topic: entry.cid,
            priority: entry.priority as isize,
            work: BlockPresence::encoded_len_for_cid(entry.cid),
            data: TaskData {
                block_size: 0,
                have_block: false,
                is_want_block: entry.want_type == WantType::Block,
                send_dont_have: entry.send_dont_have,
            },
        })
    }

    fn message_received(&mut self, received: Received) {
        let Received {
            peer,
            full,
            cancels,
            denials,
            mut wants,
        } = received;

        let tracked = self.ledgers.remove(&peer);
        let is_tracked = tracked.is_some();
        // A peer without a ledger has disconnected: its message is served from a throwaway ledger.
        let mut ledger = tracked.unwrap_or_else(|| Ledger::new(peer));

        if full {
            ledger.clear_wantlist();
        }

        let mut overflow = Vec::new();
        wants = std::mem::take(&mut wants)
            .into_iter()
            .filter_map(|want| {
                let entry = &want.entry;
                if ledger.try_want(
                    self.max_queued_wants,
                    entry.cid,
                    entry.priority,
                    entry.want_type,
                    want.size.is_some(),
                ) {
                    Some(want)
                } else {
                    overflow.push(want);
                    None
                }
            })
            .collect();
        if !overflow.is_empty() {
            debug!(%peer, overflow = overflow.len(), "wantlist overflow");
            self.handle_overflow(&mut ledger, peer, overflow, &mut wants);
        }

        for cid in &cancels {
            if ledger.cancel_want(cid).is_some() {
                self.queue.remove(cid, peer);
            }
        }

        let mut tasks = Vec::new();
        tasks.extend(
            denials
                .iter()
                .filter_map(|entry| self.dont_have_task(entry)),
        );

        for Want { entry, size } in &wants {
            match size {
                Some(block_size) => {
                    let is_want_block = self.send_as_block(entry.want_type, *block_size);
                    let work = if is_want_block {
                        *block_size
                    } else {
                        BlockPresence::encoded_len_for_cid(entry.cid)
                    };
                    tasks.push(Task {
                        topic: entry.cid,
                        priority: entry.priority as isize,
                        work,
                        data: TaskData {
                            is_want_block,
                            send_dont_have: entry.send_dont_have,
                            block_size: *block_size,
                            have_block: true,
                        },
                    });
                }
                None => tasks.extend(self.dont_have_task(entry)),
            }
        }

        if is_tracked {
            self.ledgers.insert(peer, ledger);
        }
        if !tasks.is_empty() {
            let bound = match self.max_queued_wants {
                0 => usize::MAX,
                bound => bound,
            };
            self.queue.push_tasks_truncated(bound, peer, tasks);
        }
    }

    /// Admits `overflow` wants by evicting existing ones, in the reference order: wants whose block is
    /// absent first, then wants that rank no higher than the overflow. The rest is dropped silently.
    fn handle_overflow(
        &mut self,
        ledger: &mut Ledger,
        peer: PeerId,
        mut overflow: Vec<Want>,
        wants: &mut Vec<Want>,
    ) {
        overflow.sort_by_key(|want| std::cmp::Reverse(want.entry.priority));
        let mut overflow = overflow.into_iter().peekable();
        // The reference sorts the existing wants by descending priority although its comment says
        // ascending; the order is kept as implemented.
        let existing = ledger.wantlist().entries();

        let mut evicted = vec![false; existing.len()];
        for (index, entry) in existing.iter().enumerate() {
            // The flag recorded at admission is used, not a fresh lookup: it keeps store I/O off the engine task,
            // and a queued want is never served later when its block arrives, so a stale flag costs nothing.
            if entry.present {
                continue;
            }
            let Some(admitted) = overflow.next() else {
                return;
            };
            self.evict(ledger, peer, &entry.cid);
            evicted[index] = true;
            self.admit_overflow(ledger, admitted, wants);
            if overflow.peek().is_none() {
                return;
            }
        }

        let mut replace = 0;
        for admitted in overflow {
            while evicted.get(replace).copied().unwrap_or(false) {
                replace += 1;
            }
            let Some(target) = existing.get(replace) else {
                return;
            };
            if admitted.entry.priority < target.priority {
                return;
            }
            replace += 1;
            self.evict(ledger, peer, &target.cid);
            self.admit_overflow(ledger, admitted, wants);
        }
    }

    fn evict(&mut self, ledger: &mut Ledger, peer: PeerId, cid: &Cid) {
        if ledger.cancel_want(cid).is_some() {
            self.queue.remove(cid, peer);
        }
    }

    fn admit_overflow(&self, ledger: &mut Ledger, want: Want, wants: &mut Vec<Want>) {
        let entry = &want.entry;
        if ledger.try_want(
            self.max_queued_wants,
            entry.cid,
            entry.priority,
            entry.want_type,
            want.size.is_some(),
        ) {
            wants.push(want);
        }
    }

    fn dispatch(&mut self) {
        for _ in 0..self.worker_count {
            if self.in_flight >= self.worker_count {
                return;
            }
            let Some((peer, tasks, pending_work)) = self.queue.pop_tasks(self.target_message_size)
            else {
                return;
            };
            if tasks.is_empty() {
                continue;
            }
            debug!(%peer, tasks = tasks.len(), "next envelope");

            let mut message = BitswapMessage::new(false);
            message.set_pending_bytes(pending_work as i32);
            let mut block_wants = Vec::new();
            for task in &tasks {
                let data = &task.data;
                if !data.have_block {
                    message.add_dont_have(task.topic);
                } else if data.is_want_block {
                    block_wants.push(BlockWant {
                        cid: task.topic,
                        send_dont_have: data.send_dont_have,
                    });
                } else {
                    message.add_have(task.topic);
                }
            }

            self.in_flight += 1;
            tokio::spawn(envelope::run(
                self.store.clone(),
                self.network.clone(),
                self.reports.clone(),
                peer,
                message,
                block_wants,
                tasks,
            ));
        }
    }
}
