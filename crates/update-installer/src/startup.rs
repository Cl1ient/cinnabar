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
/// Longest silence, before startup or since the last preparation heartbeat, that counts as stalled;
/// time spent awaiting the user never counts.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(300);
/// How often a preparing client looks for new progress; well under the stall timeout.
pub const PROGRESS_POLL: Duration = Duration::from_secs(10);
const PREPARING: &str = "preparing";
/// Heartbeat content while setup waits on the user; counted heartbeats are digits, never this.
const AWAITING_USER: &str = "awaiting-user";
/// How long a stopped client may exit cleanly before the helper kills what remains.
#[cfg(unix)]
const STOP_GRACE: Duration = Duration::from_secs(10);

/// The marker path handed to the client launched from `directory`.
pub fn marker(directory: &Path) -> PathBuf {
    directory.join(STARTED)
}

/// Records heartbeat `content` beside `marker`; each new content restarts the helper's deadline.
fn beat(marker: &Path, content: &str) -> io::Result<()> {
    let directory = marker
        .parent()
        .ok_or_else(|| io::Error::other("startup marker has no directory"))?;
    fs::write(directory.join(PREPARING), content)
}

/// Client side: one heartbeat per change to the setup's progress report, so a hung setup stalls,
/// and an awaiting-user state while the report says setup waits on the user.
pub struct Relay {
    marker: PathBuf,
    progress: PathBuf,
    awaiting_user: fn(&[u8]) -> bool,
    last: Option<Vec<u8>>,
    waiting: bool,
    count: u64,
}

impl Relay {
    /// Starts relaying `progress`; entering preparation is itself progress.
    pub fn start(marker: PathBuf, progress: PathBuf, awaiting_user: fn(&[u8]) -> bool) -> Self {
        let last = fs::read(&progress).ok();
        let mut relay = Self {
            marker,
            progress,
            awaiting_user,
            last,
            waiting: false,
            count: 0,
        };
        relay.beat();
        relay
    }

    /// Beats when the progress report changed or stopped awaiting the user.
    pub fn poll(&mut self) {
        let report = fs::read(&self.progress).ok();
        let waiting = report.as_deref().is_some_and(self.awaiting_user);
        let changed = report.is_some() && report != self.last;
        if report.is_some() {
            self.last = report;
        }
        if waiting {
            if !self.waiting {
                self.waiting = true;
                let _ = beat(&self.marker, AWAITING_USER);
            }
        } else if changed || self.waiting {
            self.beat();
        }
    }

    /// Ends preparation, giving the rest of startup a fresh deadline.
    pub fn finish(mut self) {
        self.beat();
    }

    fn beat(&mut self) {
        self.waiting = false;
        self.count += 1;
        let _ = beat(&self.marker, &self.count.to_string());
    }
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
        // Leaving this state is a new heartbeat, so the deadline restarts when the user answers.
        if self.beat.as_deref() == Some(AWAITING_USER) {
            return Progress::Waiting;
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
