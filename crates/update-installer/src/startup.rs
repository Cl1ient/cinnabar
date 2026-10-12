//! First-start handshake between the helper and the updated client it launched.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};

/// Names the `started` marker the launched client creates after its first complete frames.
pub const STARTUP_ENV: &str = "CINNABAR_UPDATE_STARTUP_FILE";
/// The marker's filename; the client accepts no other path from the environment.
pub const STARTED: &str = "started";
/// Longest silence, before startup or since the last preparation heartbeat, that counts as stalled.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(300);
/// How often a client preparing assets proves it is alive; well under the stall timeout.
pub const HEARTBEAT: Duration = Duration::from_secs(10);
const PREPARING: &str = "preparing";
/// How long a stopped client may exit cleanly before the helper kills what remains.
#[cfg(unix)]
const STOP_GRACE: Duration = Duration::from_secs(10);

/// The marker path handed to the client launched from `directory`.
pub fn marker(directory: &Path) -> PathBuf {
    directory.join(STARTED)
}

/// Records preparation heartbeat `count` beside `marker`; each new count restarts the deadline.
pub fn beat(marker: &Path, count: u64) -> io::Result<()> {
    let directory = marker
        .parent()
        .ok_or_else(|| io::Error::other("startup marker has no directory"))?;
    fs::write(directory.join(PREPARING), count.to_string())
}

/// How far the launched client's first start has progressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    Started,
    Waiting,
    Stalled,
}

/// Helper-side deadline that every distinct preparation heartbeat restarts.
pub struct Monitor {
    deadline: Instant,
    beat: Option<String>,
}

impl Monitor {
    pub fn new(now: Instant) -> Self {
        Self {
            deadline: now + STALL_TIMEOUT,
            beat: None,
        }
    }

    /// Reads the handshake in `directory`; `now` comes from the caller.
    pub fn poll(&mut self, directory: &Path, now: Instant) -> Progress {
        if directory.join(STARTED).is_file() {
            return Progress::Started;
        }
        let beat = fs::read_to_string(directory.join(PREPARING)).ok();
        if beat.is_some() && beat != self.beat {
            self.beat = beat;
            self.deadline = now + STALL_TIMEOUT;
        }
        if now >= self.deadline {
            Progress::Stalled
        } else {
            Progress::Waiting
        }
    }
}

/// Starts the client in its own process group, so `stop` also reaches its setup process.
pub fn isolate(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

/// Returns once the client and every process left in its group have exited.
pub fn stop(child: &mut Child) -> io::Result<()> {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
        let group = Pid::from_child(child);
        for signal in [Signal::TERM, Signal::KILL] {
            let _ = kill_process_group(group, signal);
            let deadline = Instant::now() + STOP_GRACE;
            // Reaping the leader keeps its zombie from holding the group open.
            while child.try_wait()?.is_none() || test_kill_process_group(group).is_ok() {
                if signal == Signal::TERM && Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        child.wait()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        child.wait().map(drop)
    }
}

#[cfg(test)]
mod tests;
