use crate::domain::boundary::{DbusOutput, DbusRequest, UpowerDeviceProperties};

use super::*;

#[test]
fn typed_replies_are_fifo_and_calls_are_recorded() {
    let request = DbusRequest::UpowerDeviceProperties {
        object_path: "/battery".to_owned(),
    };
    let first = DbusOutput::UpowerDeviceProperties(UpowerDeviceProperties {
        percentage: Some(25.0),
        ..UpowerDeviceProperties::default()
    });
    let second = DbusOutput::UpowerDeviceProperties(UpowerDeviceProperties {
        percentage: Some(50.0),
        ..UpowerDeviceProperties::default()
    });
    let mut fake = FakeDbus::new();
    fake.enqueue_request(request.clone(), first.clone())
        .enqueue_request(request.clone(), second.clone());

    assert_eq!(DbusFacade::call(&mut fake, request.clone()), Ok(first));
    assert_eq!(DbusFacade::call(&mut fake, request.clone()), Ok(second));
    assert_eq!(fake.call_trace(), &[request.clone(), request]);
}

#[test]
fn unqueued_typed_call_returns_contextual_error_and_is_traced() {
    let mut fake = FakeDbus::new();
    let request = DbusRequest::UpowerEnumerate;
    let error = match DbusFacade::call(&mut fake, request.clone()) {
        Ok(output) => panic!("unexpected output: {output:?}"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("EnumerateDevices"));
    assert_eq!(fake.next_call(), Some(&request));
}
