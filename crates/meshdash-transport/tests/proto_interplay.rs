//! Checks that a transport and the protocol codec fit together.
//!
//! The layers are deliberately separate: the transport hands over frames and
//! knows nothing about their meaning, the codec knows meaning and does no I/O.
//! This test is where the seam between them is exercised — without hardware.

use meshdash_proto::opcode::{Command, Push, Response};
use meshdash_transport::{
    Transport, TransportError,
    mock::{MockTransport, Step},
};

/// A node that announces a pending message, then delivers it after the sync.
#[tokio::test]
async fn drives_the_message_pickup_sequence() {
    let mut node = MockTransport::new(vec![
        Step::Emit(vec![u8::from(Push::MsgWaiting)]),
        Step::Emit(vec![u8::from(Response::ContactMsgRecvV3), 0x14, 0, 0]),
        Step::Emit(vec![u8::from(Response::NoMoreMessages)]),
    ]);
    node.connect().await.unwrap();

    // The push only signals that something is waiting.
    let frame = node.recv().await.unwrap();
    assert_eq!(Push::from(frame[0]), Push::MsgWaiting);

    // So the app asks for it.
    node.send(&[u8::from(Command::SyncNextMessage)])
        .await
        .unwrap();

    let frame = node.recv().await.unwrap();
    assert_eq!(Response::from(frame[0]), Response::ContactMsgRecvV3);
    // SNR is stored multiplied by four, see the research notes.
    assert_eq!(i32::from(frame[1] as i8) / 4, 5);

    let frame = node.recv().await.unwrap();
    assert_eq!(Response::from(frame[0]), Response::NoMoreMessages);

    assert_eq!(node.sent(), &[vec![u8::from(Command::SyncNextMessage)]]);
}

/// An opcode this firmware table does not know must survive the trip.
#[tokio::test]
async fn passes_an_unknown_opcode_through_untouched() {
    let mut node = MockTransport::new(vec![Step::Emit(vec![0xF7, 0x01, 0x02])]);
    node.connect().await.unwrap();

    let frame = node.recv().await.unwrap();

    assert_eq!(Push::from(frame[0]), Push::Unknown(0xF7));
    assert_eq!(&frame[1..], &[0x01, 0x02], "payload must stay intact");
}

/// A dropped cable is a reconnect, not the end of the program.
#[tokio::test]
async fn survives_a_drop_and_carries_on_after_reconnecting() {
    let mut node = MockTransport::new(vec![
        Step::Emit(vec![u8::from(Response::Ok)]),
        Step::Drop("usb cable pulled".into()),
        Step::Emit(vec![u8::from(Response::SelfInfo)]),
    ]);
    node.connect().await.unwrap();

    assert_eq!(Response::from(node.recv().await.unwrap()[0]), Response::Ok);

    let error = node.recv().await.unwrap_err();
    assert!(matches!(error, TransportError::Disconnected { .. }));

    node.connect().await.unwrap();
    assert_eq!(
        Response::from(node.recv().await.unwrap()[0]),
        Response::SelfInfo
    );
    assert_eq!(node.connect_count(), 2);
}
