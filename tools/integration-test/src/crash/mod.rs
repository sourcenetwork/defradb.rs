//! Crash a node mid-commit. A node started with `NewNode { own_process }`
//! runs in a process of its own: this test binary run again as
//! [`HOST_TEST`], whose store writes its files through [`CrashEnv`]. The
//! test arms one [`CrashPoint`], and the node's process kills itself with
//! SIGKILL when its log reaches it.

mod env;
pub mod host;
mod process;

pub use env::CrashEnv;
pub use process::NodeProcess;

use serde::{Deserialize, Serialize};

/// The test, in the `testo` binary, that runs a node in its own process.
pub const HOST_TEST: &str = "node::crash_host::serve";

/// Where in a commit the node's process kills itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashPoint {
    /// Before any byte of the commit's log record is written.
    #[default]
    BeforeLogWrite,
    /// After the first half of the commit's log record is written.
    TornLogWrite,
    /// After the log sync that makes the commit's record durable, before the
    /// commit is answered.
    AfterLogSync,
}

impl CrashPoint {
    const ALL: [Self; 3] = [Self::BeforeLogWrite, Self::TornLogWrite, Self::AfterLogSync];

    fn code(self) -> u8 {
        self as u8 + 1
    }

    fn of(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|point| point.code() == code)
    }

    fn name(self) -> &'static str {
        match self {
            Self::BeforeLogWrite => "before-log-write",
            Self::TornLogWrite => "torn-log-write",
            Self::AfterLogSync => "after-log-sync",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|point| point.name() == name)
    }
}

/// End this process at once, as `kill -9` would.
fn kill_self() -> ! {
    #[cfg(unix)]
    // SAFETY: kill(2) on this process's own ID takes no pointers.
    unsafe {
        libc::kill(libc::getpid(), libc::SIGKILL);
    }
    std::process::abort()
}
