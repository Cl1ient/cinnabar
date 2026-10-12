use super::*;

#[test]
fn a_silent_start_stalls_after_the_timeout() {
    let directory = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let mut monitor = Monitor::new(start);
    let before = start + STALL_TIMEOUT - Duration::from_secs(1);
    assert_eq!(monitor.poll(directory.path(), before), Progress::Waiting);
    assert_eq!(
        monitor.poll(directory.path(), start + STALL_TIMEOUT),
        Progress::Stalled
    );
}

#[test]
fn only_changed_setup_progress_restarts_the_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let progress = directory.path().join("status.json");
    fs::write(&progress, "downloading 0").unwrap();
    let start = Instant::now();
    let mut monitor = Monitor::new(start);
    let mut relay = Relay::start(marker(directory.path()), progress.clone());
    let mut now = start + STALL_TIMEOUT - Duration::from_secs(1);
    assert_eq!(monitor.poll(directory.path(), now), Progress::Waiting);
    // A setup that reports nothing new is hung, however often the client polls.
    for _ in 0..3 {
        relay.poll();
    }
    now += STALL_TIMEOUT;
    assert_eq!(monitor.poll(directory.path(), now), Progress::Stalled);
    fs::write(&progress, "downloading 1").unwrap();
    relay.poll();
    assert_eq!(monitor.poll(directory.path(), now), Progress::Waiting);
}

#[test]
fn preparation_heartbeats_restart_the_deadline_until_they_stop() {
    let directory = tempfile::tempdir().unwrap();
    let marker = marker(directory.path());
    let start = Instant::now();
    let mut monitor = Monitor::new(start);
    // Asset preparation lasting several stall timeouts keeps the client alive.
    let mut now = start;
    for count in 1..=4 {
        now += STALL_TIMEOUT - Duration::from_secs(1);
        beat(&marker, count).unwrap();
        assert_eq!(monitor.poll(directory.path(), now), Progress::Waiting);
    }
    // A repeated count is not progress, so a hung client still stalls.
    assert_eq!(
        monitor.poll(directory.path(), now + STALL_TIMEOUT),
        Progress::Stalled
    );
    fs::write(&marker, "").unwrap();
    assert_eq!(
        monitor.poll(directory.path(), now + STALL_TIMEOUT * 2),
        Progress::Started
    );
}

#[cfg(unix)]
#[test]
fn stopping_a_client_also_ends_the_setup_process_it_started() {
    use rustix::process::{Pid, test_kill_process_group};
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 600 & wait"]);
    isolate(&mut command);
    let mut child = command.spawn().unwrap();
    let group = Pid::from_child(&child);
    stop(&mut child).unwrap();
    assert!(test_kill_process_group(group).is_err());
}
