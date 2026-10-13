//! Per-attempt cancellation for setup, which outlives the activity that started it.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};

/// Attempts are numbered upward by the setup activity; cancelling one also cancels older ones.
#[derive(Debug, Default)]
pub(crate) struct Cancellation {
    cancelled_through: AtomicU64,
    running: AtomicU64,
    flag: AtomicBool,
}

impl Cancellation {
    pub(crate) const fn new() -> Self {
        Self {
            cancelled_through: AtomicU64::new(0),
            running: AtomicU64::new(0),
            flag: AtomicBool::new(false),
        }
    }

    /// Starts `attempt`, returning false when it was cancelled before it began.
    pub(crate) fn begin(&self, attempt: u64) -> bool {
        self.running.store(attempt, SeqCst);
        let cancelled = self.cancelled_through.load(SeqCst) >= attempt;
        self.flag.store(cancelled, SeqCst);
        !cancelled
    }

    /// Cancels `attempt`, before or while it runs.
    pub(crate) fn cancel(&self, attempt: u64) {
        self.cancelled_through.fetch_max(attempt, SeqCst);
        if self.running.load(SeqCst) == attempt {
            self.flag.store(true, SeqCst);
        }
    }

    /// The running attempt's flag, which preparation polls.
    pub(crate) fn flag(&self) -> &AtomicBool {
        &self.flag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An attempt cancelled while it waited to start never runs, and stays cancelled.
    #[test]
    fn a_cancellation_before_the_attempt_starts_is_kept() {
        let cancellation = Cancellation::new();
        cancellation.cancel(1);
        assert!(!cancellation.begin(1));
        assert!(cancellation.flag().load(SeqCst));
        assert!(cancellation.begin(2));
        assert!(!cancellation.flag().load(SeqCst));
    }

    /// Cancelling the running attempt stops it; a stale cancellation does not stop a newer one.
    #[test]
    fn only_the_cancelled_attempt_stops() {
        let cancellation = Cancellation::new();
        assert!(cancellation.begin(3));
        cancellation.cancel(2);
        assert!(!cancellation.flag().load(SeqCst));
        cancellation.cancel(3);
        assert!(cancellation.flag().load(SeqCst));
    }
}
