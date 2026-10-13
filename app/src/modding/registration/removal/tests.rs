use super::*;
use std::{fs, path::PathBuf, thread};

/// Removal cannot delete a companion until a previously queued save has completed.
#[test]
fn completion_waits_for_the_retired_hosts_final_settings_write() {
    #[derive(Debug)]
    struct PendingSave {
        path: PathBuf,
        started: Sender<()>,
        release: Receiver<()>,
    }

    impl Drop for PendingSave {
        fn drop(&mut self) {
            let _ = self.started.send(());
            if self.release.recv().is_ok() {
                fs::write(&self.path, b"pending preferences").unwrap();
            }
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let companion = directory.path().join("zoom.settings.json");
    let (retired, retiring) = bounded(1);
    let (started, started_save) = bounded(1);
    let (release, released) = bounded(1);
    let (complete, completion) = bounded(1);
    retired
        .send(PendingSave {
            path: companion.clone(),
            started,
            release: released,
        })
        .unwrap();
    let worker = thread::spawn(move || finish(&retiring, complete));
    test_time::eventually("retired host's pending save starts", || {
        started_save.try_recv().is_ok()
    });
    assert!(completion.try_recv().is_err());
    release.send(()).unwrap();
    completion.recv().unwrap().unwrap();
    fs::remove_file(&companion).unwrap();
    worker.join().unwrap();
    assert!(!companion.exists());
}

/// A queued removal waits for full frame detachment, not just an early disabled notice.
#[test]
fn a_removal_waits_for_its_matching_applied_selection() {
    let (fences, waiting_fence) = bounded(1);
    let (complete, completion) = bounded(1);
    let mut pending = Pending::default();
    pending.wait(1, "remove".into(), complete, &fences);
    pending.observe(Some("remove"));
    assert!(waiting_fence.try_recv().is_err());
    assert!(completion.try_recv().is_err());
    pending.applied(1, "remove".into(), &fences);
    let (_, retiring) = bounded::<()>(1);
    finish(&retiring, waiting_fence.try_recv().unwrap());
    completion.recv().unwrap().unwrap();
}

/// An applied selection remains usable when its acknowledgment arrives before the request.
#[test]
fn a_late_removal_wait_uses_only_the_current_generation() {
    let (fences, waiting_fence) = bounded(1);
    let mut pending = Pending::default();
    pending.applied(1, "remove".into(), &fences);
    let (complete, completion) = bounded(1);
    pending.wait(2, "remove".into(), complete, &fences);
    assert!(waiting_fence.try_recv().is_err());
    pending.applied(2, "remove".into(), &fences);
    let (_, retiring) = bounded::<()>(1);
    finish(&retiring, waiting_fence.try_recv().unwrap());
    completion.recv().unwrap().unwrap();
    let (complete, completion) = bounded(1);
    pending.wait(2, "remove".into(), complete, &fences);
    finish(&retiring, waiting_fence.try_recv().unwrap());
    completion.recv().unwrap().unwrap();
}

/// Superseded selections fail removal instead of leaving a worker waiting indefinitely.
#[test]
fn a_superseded_removal_reports_failure_without_a_retirement_fence() {
    let (fences, waiting_fence) = bounded(1);
    let (complete, completion) = bounded(1);
    let mut pending = Pending::default();
    pending.wait(1, "remove".into(), complete, &fences);
    pending.observe(Some("replacement"));
    assert!(completion.recv().unwrap().is_err());
    assert!(waiting_fence.try_recv().is_err());
}
