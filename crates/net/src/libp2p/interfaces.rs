//! The interface addresses a listener on an unspecified address is
//! reachable at, as they come and go.
//!
//! libp2p-tcp learns of changes from the system's routing socket through a
//! tokio watcher; here the interfaces are read again on a period, which
//! costs one `getifaddrs` call per listener each time.

use std::collections::VecDeque;
use std::io;
use std::net::IpAddr;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use nuclei::time::{Interval, MissedTickBehavior};
use rapidhash::{HashSetExt, RapidHashSet};

/// How often the interfaces are read again.
const RESCAN: Duration = Duration::from_secs(5);

pub(crate) enum Change {
    Up(IpAddr),
    Down(IpAddr),
    Failed(io::Error),
}

pub(crate) struct Interfaces {
    ipv4: bool,
    known: RapidHashSet<IpAddr>,
    changes: VecDeque<Change>,
    rescan: Interval,
}

impl Interfaces {
    /// The interfaces of `unspecified`'s family: a listener on `0.0.0.0`
    /// is reachable at IPv4 addresses only, one on `::` at IPv6 ones.
    pub(crate) fn new(unspecified: IpAddr) -> Self {
        let mut rescan = nuclei::time::interval(RESCAN);
        rescan.set_missed_tick_behavior(MissedTickBehavior::Skip);
        Self {
            ipv4: unspecified.is_ipv4(),
            known: RapidHashSet::new(),
            changes: VecDeque::new(),
            rescan,
        }
    }

    pub(crate) fn addresses(&self) -> impl Iterator<Item = IpAddr> + '_ {
        self.known.iter().copied()
    }

    /// The next address that came or went. The first read reports every
    /// address there is.
    pub(crate) fn poll_change(&mut self, cx: &mut Context<'_>) -> Poll<Change> {
        loop {
            if let Some(change) = self.changes.pop_front() {
                return Poll::Ready(change);
            }
            ready!(self.rescan.poll_tick(cx));
            self.read();
        }
    }

    fn read(&mut self) {
        let interfaces = match if_addrs::get_if_addrs() {
            Ok(interfaces) => interfaces,
            Err(error) => {
                self.changes.push_back(Change::Failed(error));
                return;
            }
        };
        let now: RapidHashSet<IpAddr> = interfaces
            .iter()
            .map(if_addrs::Interface::ip)
            .filter(|ip| ip.is_ipv4() == self.ipv4)
            .collect();
        self.changes
            .extend(self.known.difference(&now).copied().map(Change::Down));
        self.changes
            .extend(now.difference(&self.known).copied().map(Change::Up));
        self.known = now;
    }
}
