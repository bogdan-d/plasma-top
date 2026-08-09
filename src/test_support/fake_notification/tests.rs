use super::*;
use crate::domain::boundary::{NotificationTimeout, NotificationUrgency};

fn payload(body: &str) -> NotificationPayload {
    NotificationPayload {
        title: "PlasmaTop".to_owned(),
        body: body.to_owned(),
        icon: "dialog-error".to_owned(),
        urgency: NotificationUrgency::Critical,
        timeout: NotificationTimeout::Never,
    }
}

#[test]
fn records_order_even_when_a_queued_call_fails() {
    let mut fake = FakeNotificationFacade::new();
    fake.push_result(Err(NotificationError {
        detail: "service absent".to_owned(),
    }));

    assert!(fake.send(&payload("first")).is_err());
    assert!(fake.send(&payload("second")).is_ok());
    assert_eq!(fake.calls(), &[payload("first"), payload("second")]);
}
