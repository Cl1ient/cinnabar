//! Completion requests for removal, after frame detachment and off-frame settings retirement.

use super::{Message, after_retirement};
use crossbeam_channel::{Receiver, Sender, bounded};

pub(in crate::modding) struct Client {
    messages: Sender<Message>,
}

impl Client {
    /// Connects the library worker to the registration worker's retirement acknowledgments.
    pub(super) fn new(messages: Sender<Message>) -> Self {
        Self { messages }
    }

    /// Waits off-frame until this selection has applied and all earlier settings writes finish.
    pub(in crate::modding) fn wait(&self, request_id: &str) -> Result<(), String> {
        let (complete, completion) = bounded(1);
        self.messages
            .send(Message::AwaitRetirement {
                request_id: request_id.to_owned(),
                complete,
            })
            .map_err(|_| "The mod host is unavailable; the mod was not removed.".to_owned())?;
        completion
            .recv()
            .unwrap_or_else(|_| Err("The mod host stopped before removal finished.".to_owned()))
    }
}

pub(super) type Completion = Sender<Result<(), String>>;

#[derive(Default)]
pub(super) struct Pending {
    applied: Option<(u64, String)>,
    waiting: Option<(String, Completion)>,
}

impl Pending {
    /// Rejects a removal superseded by a different selection instead of deleting live files.
    pub(super) fn observe(&mut self, request_id: Option<&str>) {
        if self
            .waiting
            .as_ref()
            .is_some_and(|(waiting, _)| Some(waiting.as_str()) != request_id)
            && let Some((_, complete)) = self.waiting.take()
        {
            let _ = complete.send(Err(
                "The mod selection changed before removal finished.".into()
            ));
        }
    }

    /// Records a full frame transition, distinct from an early authority-revocation notice.
    pub(super) fn applied(
        &mut self,
        generation: u64,
        request_id: String,
        fences: &Sender<Completion>,
    ) {
        self.applied = Some((generation, request_id.clone()));
        if self
            .waiting
            .as_ref()
            .is_some_and(|(waiting, _)| *waiting == request_id)
            && let Some((_, complete)) = self.waiting.take()
        {
            let _ = fences.send(complete);
        }
    }

    /// Queues a retirement fence immediately only when this exact selection already applied.
    pub(super) fn wait(
        &mut self,
        generation: u64,
        request_id: String,
        complete: Completion,
        fences: &Sender<Completion>,
    ) {
        if self
            .applied
            .as_ref()
            .is_some_and(|(applied, id)| *applied == generation && *id == request_id)
        {
            let _ = fences.send(complete);
        } else if let Some((_, previous)) = self.waiting.replace((request_id, complete)) {
            let _ = previous.send(Err("Another removal replaced this request.".into()));
        }
    }
}

/// Completes a fence only after every queued host has dropped and joined its settings writer.
pub(super) fn finish<T>(retired: &Receiver<T>, complete: Completion) {
    after_retirement(retired, || {
        let _ = complete.send(Ok(()));
    });
}

#[cfg(test)]
mod tests;
