use std::sync::Arc;

use tokio::sync::{Semaphore, mpsc};

use crate::adapters::ProductionNotificationFacade;
use crate::domain::boundary::{NotificationError, NotificationFacade, NotificationPayload};

pub(super) struct QueueFacade<'a>(pub(super) &'a mpsc::Sender<NotificationPayload>);

impl NotificationFacade for QueueFacade<'_> {
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        self.0
            .try_send(payload.clone())
            .map_err(|error| NotificationError {
                detail: format!("bounded notification queue rejected request: {error}"),
            })
    }
}

pub(super) async fn run(
    mut receiver: mpsc::Receiver<NotificationPayload>,
    notifications: ProductionNotificationFacade,
    blocking_lane: Arc<Semaphore>,
) {
    while let Some(payload) = receiver.recv().await {
        let Ok(permit) = Arc::clone(&blocking_lane).acquire_owned().await else {
            return;
        };
        let mut notifications = notifications.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            notifications.send(&payload)
        });
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("[notify] {error}"),
            Err(_) => return,
        }
    }
}
