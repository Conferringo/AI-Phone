use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::{sync::Notify, time::Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const HOLD_TIME: Duration = Duration::from_secs(5);
type Turn = (Uuid, Uuid, String);

#[derive(Default)]
pub(crate) struct SpeechActivity {
    holds: Mutex<HashMap<Turn, Instant>>,
    changed: Notify,
}

impl SpeechActivity {
    pub(crate) fn heard(&self, conversation: Uuid, session: Uuid, utterance: String) {
        let now = Instant::now();
        let mut holds = self.holds.lock().expect("speech activity");
        holds.retain(|_, deadline| *deadline > now);
        holds.insert((conversation, session, utterance), now + HOLD_TIME);
    }

    pub(crate) fn accepted(&self, conversation: Uuid, session: Uuid, utterance: &str) {
        if self
            .holds
            .lock()
            .expect("speech activity")
            .remove(&(conversation, session, utterance.to_owned()))
            .is_some()
        {
            self.changed.notify_waiters();
        }
    }

    /// Return true if the worker waited, so it can read newly committed steering first.
    pub(crate) async fn wait(
        &self,
        conversation: Uuid,
        cancel: &CancellationToken,
    ) -> Result<bool, &'static str> {
        let mut waited = false;
        loop {
            let changed = self.changed.notified();
            let now = Instant::now();
            let deadline = self
                .holds
                .lock()
                .expect("speech activity")
                .iter()
                .filter(|((id, _, _), until)| *id == conversation && **until > now)
                .map(|(_, until)| *until)
                .max();
            let Some(deadline) = deadline else {
                return Ok(waited);
            };
            waited = true;
            tokio::select! {
                _ = cancel.cancelled() => return Err("canceled"),
                _ = changed => {},
                _ = tokio::time::sleep_until(deadline) => {},
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::poll_fn, task::Poll};

    #[tokio::test]
    async fn unrelated_receipts_and_conversations_do_not_change_the_hold() {
        let activity = SpeechActivity::default();
        let conversation = Uuid::from_u128(1);
        let session = Uuid::from_u128(2);
        let other = Uuid::from_u128(3);
        let cancel = CancellationToken::new();
        activity.heard(conversation, session, "spoken".into());
        assert!(!activity.wait(other, &cancel).await.unwrap());

        let pending = activity.wait(conversation, &cancel);
        tokio::pin!(pending);
        assert!(
            poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        activity.accepted(conversation, other, "spoken");
        activity.accepted(conversation, session, "another utterance");
        assert!(
            poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        activity.accepted(conversation, session, "spoken");
        assert!(pending.await.unwrap());
    }
}
