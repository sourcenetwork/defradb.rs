use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{Result, WrapErr};

/// A child process that is killed on drop.
///
/// Stores the command components (program, args, envs, log_dir) so the
/// process can be respawned after being killed (e.g. for restart tests).
/// Drop sends SIGTERM, waits up to 500ms, then SIGKILL.
pub struct ManagedProcess {
    name: String,
    child: Option<Child>,
    log_dir: PathBuf,
    program: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
}

impl ManagedProcess {
    /// Spawn a command with stdout/stderr redirected to log files.
    ///
    /// Stores the command components so the process can be respawned later
    /// via [`respawn()`](Self::respawn).
    pub fn spawn(
        name: &str,
        program: &Path,
        args: &[&str],
        envs: &[(&str, &str)],
        log_dir: &Path,
    ) -> Result<Self> {
        fs::create_dir_all(log_dir)
            .wrap_err_with(|| format!("failed to create log dir {}", log_dir.display()))?;

        let stdout_file = File::create(log_dir.join("stdout.log"))
            .wrap_err_with(|| format!("failed to create stdout.log in {}", log_dir.display()))?;
        let stderr_file = File::create(log_dir.join("stderr.log"))
            .wrap_err_with(|| format!("failed to create stderr.log in {}", log_dir.display()))?;

        let mut cmd = Command::new(program);
        cmd.args(args);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::from(stdout_file));
        cmd.stderr(Stdio::from(stderr_file));

        let child = cmd
            .spawn()
            .wrap_err_with(|| format!("failed to spawn {}", name))?;

        tracing::info!("{}: spawned pid={}", name, child.id());

        Ok(Self {
            name: name.to_string(),
            child: Some(child),
            log_dir: log_dir.to_path_buf(),
            program: program.to_path_buf(),
            args: args.iter().map(|s| s.to_string()).collect(),
            envs: envs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        })
    }

    /// Create an empty placeholder (no child process).
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            child: None,
            log_dir: PathBuf::new(),
            program: PathBuf::new(),
            args: Vec::new(),
            envs: Vec::new(),
        }
    }

    /// Kill the current process and spawn a new one with the same arguments.
    ///
    /// Log files are opened in append mode so existing log trackers
    /// seamlessly pick up output from the new process.
    pub fn respawn(&mut self) -> Result<()> {
        self.kill();

        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_dir.join("stdout.log"))
            .wrap_err_with(|| "failed to open stdout.log for append")?;
        let stderr_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_dir.join("stderr.log"))
            .wrap_err_with(|| "failed to open stderr.log for append")?;

        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (k, v) in &self.envs {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::from(stdout_file));
        cmd.stderr(Stdio::from(stderr_file));

        let child = cmd
            .spawn()
            .wrap_err_with(|| format!("failed to respawn {}", self.name))?;

        tracing::info!("{}: respawned pid={}", self.name, child.id());
        self.child = Some(child);
        Ok(())
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        let Some(mut child) = self.child.take() else {
            return false;
        };
        match child.try_wait() {
            Ok(None) => {
                self.child = Some(child);
                true
            }
            Ok(Some(status)) => {
                tracing::info!("{}: exited with {}", self.name, status);
                false
            }
            Err(error) => {
                tracing::warn!("{}: error polling: {}", self.name, error);
                self.child = Some(child);
                false
            }
        }
    }

    /// Kill the process immediately.
    pub fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
            tracing::info!("{}: killed process", self.name);
        }
        self.child = None;
    }

    /// Get the log directory path.
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };

        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }

        let pid = child.id();
        tracing::info!("{}: sending SIGTERM to pid={}", self.name, pid);

        // Send SIGTERM
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }

        // `Drop` cannot await and `Child` has no timed wait, so the exit is
        // polled for 500ms before the process is killed.
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!("{}: exited with {}", self.name, status);
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("{}: error polling: {}", self.name, e);
                    break;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        tracing::warn!("{}: sending SIGKILL to pid={}", self.name, pid);
        let _ = child.kill();
        let _ = child.wait();
    }
}
