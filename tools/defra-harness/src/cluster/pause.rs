//! Freeze a node's process without killing it.
//!
//! A paused node (SIGSTOP) keeps its stores, ports and open connections, but
//! answers nothing until it is resumed (SIGCONT). Its peers see a provider
//! that goes silent mid-stream rather than one that disconnects.
//!
//! [`Throttle`] alternates the two: the node runs for `run`, then is frozen
//! for `stop`, over and over. That is a provider which is productive but slow:
//! each of its peers' fetch windows still sees some progress, which a plain
//! pause or a stop cannot produce on localhost.

use std::time::Duration;

use crate::error::{Result, WrapErr};
use tokio::task::JoinHandle;

use super::runtime::TestCluster;

fn send(pid: u32, signal: libc::c_int) -> std::io::Result<()> {
    // SAFETY: kill(2) only delivers a signal; it touches no memory of ours.
    let rc = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// A node held in a run/stop duty cycle by [`TestCluster::throttle_node`].
///
/// [`Throttle::release`] ends the cycle and leaves the node running. Dropping
/// the handle does the same on a best-effort basis.
pub struct Throttle {
    pid: u32,
    task: Option<JoinHandle<()>>,
}

impl Throttle {
    fn start(pid: u32, run: Duration, stop: Duration) -> Self {
        let task = tokio::spawn(async move {
            loop {
                let _ = send(pid, libc::SIGCONT);
                tokio::time::sleep(run).await;
                let _ = send(pid, libc::SIGSTOP);
                tokio::time::sleep(stop).await;
            }
        });
        Self {
            pid,
            task: Some(task),
        }
    }

    /// End the duty cycle and leave the node running.
    pub async fn release(mut self) -> Result<()> {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        send(self.pid, libc::SIGCONT).wrap_err("resume throttled node")
    }
}

impl Drop for Throttle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = send(self.pid, libc::SIGCONT);
        }
    }
}

impl TestCluster {
    fn node_pid(&self, index: usize) -> Result<u32> {
        let node = &self.nodes[index];
        node.process
            .id()
            .ok_or_else(|| crate::error::err!("{}: no running process", node.name))
    }

    /// Freeze the node at `index` (SIGSTOP). It keeps its connections and
    /// state but answers nothing until [`resume_node`](Self::resume_node).
    pub fn pause_node(&self, index: usize) -> Result<()> {
        let pid = self.node_pid(index)?;
        send(pid, libc::SIGSTOP).wrap_err_with(|| format!("{}: pause", self.nodes[index].name))
    }

    /// Let a node frozen by [`pause_node`](Self::pause_node) run again.
    pub fn resume_node(&self, index: usize) -> Result<()> {
        let pid = self.node_pid(index)?;
        send(pid, libc::SIGCONT).wrap_err_with(|| format!("{}: resume", self.nodes[index].name))
    }

    /// Hold the node at `index` in a duty cycle: run for `run`, freeze for
    /// `stop`, repeat, until the returned [`Throttle`] is released.
    pub fn throttle_node(&self, index: usize, run: Duration, stop: Duration) -> Result<Throttle> {
        Ok(Throttle::start(self.node_pid(index)?, run, stop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};

    fn state(child: &Child) -> String {
        let out = Command::new("ps")
            .args(["-o", "stat=", "-p", &child.id().to_string()])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    async fn wait_for_state(child: &Child, stopped: bool) -> bool {
        for _ in 0..100 {
            if state(child).starts_with('T') == stopped {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    #[tokio::test]
    async fn signals_stop_and_continue_a_process() {
        let mut child = Command::new("sleep").arg("30").spawn().expect("spawn");
        send(child.id(), libc::SIGSTOP).expect("stop");
        assert!(wait_for_state(&child, true).await, "SIGSTOP must freeze");
        send(child.id(), libc::SIGCONT).expect("continue");
        assert!(wait_for_state(&child, false).await, "SIGCONT must thaw");
        child.kill().expect("kill");
        child.wait().expect("reap");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn released_throttle_leaves_the_process_running() {
        let mut child = Command::new("sleep").arg("30").spawn().expect("spawn");
        let throttle = Throttle::start(
            child.id(),
            Duration::from_millis(5),
            Duration::from_millis(50),
        );
        assert!(
            wait_for_state(&child, true).await,
            "the duty cycle must freeze the process"
        );
        throttle.release().await.expect("release");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !state(&child).starts_with('T'),
            "a released throttle must leave the process running"
        );
        child.kill().expect("kill");
        child.wait().expect("reap");
    }
}
