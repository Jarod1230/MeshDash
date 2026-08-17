//! A transport that plays back a script instead of talking to hardware.
//!
//! This is part of the architecture, not a test aid: without it neither the
//! link nor a module can be exercised, and nobody without a node on the USB
//! port could work on the project. See `docs/testing.md`.
//!
//! # What it can and cannot prove
//!
//! The mock replays **our assumption** about the firmware. If the assumption is
//! wrong, the tests stay green and the software is still wrong — which is why
//! protocol values are never guessed. It proves the shape of our own logic,
//! not the behaviour of a real node.

use std::sync::{Arc, Mutex};

use crate::{Transport, TransportError};

/// A view of what a [`MockTransport`] was told to send.
///
/// Cloning it shares the same record, so a test can keep watching after the
/// transport itself has been handed to an actor and is out of reach.
#[derive(Debug, Clone, Default)]
pub struct SentFrames(Arc<Mutex<Vec<Vec<u8>>>>);

impl SentFrames {
    /// The frames recorded so far, in order.
    pub fn snapshot(&self) -> Vec<Vec<u8>> {
        self.lock().clone()
    }

    /// How many frames were sent.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing has been sent yet.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Appends a frame to the record.
    fn record(&self, frame: &[u8]) {
        self.lock().push(frame.to_vec());
    }

    /// Takes the lock, recovering from a poisoned mutex.
    ///
    /// A panic in another test thread must not turn every later assertion into
    /// a second, confusing panic — the record itself is still intact.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Vec<u8>>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One step in a mock script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The node emits this frame.
    Emit(Vec<u8>),

    /// The link drops, with the given reason. Everything after this step is
    /// only reachable after a reconnect.
    Drop(String),

    /// Holds the script until the caller has sent this many frames in total.
    ///
    /// Needed for anything request/response: a reader that is idle would
    /// otherwise consume the answer before the question was asked, and a real
    /// node only answers once asked. Without this, the correlation cases in
    /// `docs/testing.md` cannot be exercised at all.
    AwaitSent(usize),
}

/// A scripted stand-in for a companion node.
#[derive(Debug, Default)]
pub struct MockTransport {
    script: Vec<Step>,
    position: usize,
    connected: bool,
    sent: SentFrames,
    connect_count: usize,
    failing_connects: usize,
    rejected_connects: usize,
}

impl MockTransport {
    /// Creates a transport that will play back `script` once connected.
    pub fn new(script: Vec<Step>) -> Self {
        Self {
            script,
            ..Self::default()
        }
    }

    /// Frames the caller has sent, in order.
    pub fn sent(&self) -> Vec<Vec<u8>> {
        self.sent.snapshot()
    }

    /// A handle to the send record that outlives moving this transport.
    ///
    /// Needed to observe an actor that has taken ownership of the transport.
    pub fn sent_frames(&self) -> SentFrames {
        self.sent.clone()
    }

    /// Makes the next `count` connection attempts fail before one succeeds.
    ///
    /// Stands in for a node that is unplugged, still booting, or not yet
    /// reachable over the network — the situation a reconnect strategy exists
    /// for. Without it, retry behaviour cannot be observed at all.
    pub fn failing_connects(mut self, count: usize) -> Self {
        self.failing_connects = count;
        self
    }

    /// How often a connection attempt was rejected.
    pub fn rejected_connects(&self) -> usize {
        self.rejected_connects
    }

    /// How often [`Transport::connect`] succeeded — the reconnect counter.
    pub fn connect_count(&self) -> usize {
        self.connect_count
    }

    /// Fails unless the link is currently up.
    fn require_connection(&self) -> Result<(), TransportError> {
        if self.connected {
            Ok(())
        } else {
            Err(TransportError::Disconnected {
                reason: "mock transport is not connected".into(),
            })
        }
    }
}

#[async_trait::async_trait]
impl Transport for MockTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        if self.failing_connects > 0 {
            self.failing_connects -= 1;
            self.rejected_connects += 1;
            return Err(TransportError::Disconnected {
                reason: "mock refuses this connection attempt".into(),
            });
        }

        self.connected = true;
        self.connect_count += 1;
        Ok(())
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.require_connection()?;
        self.sent.record(frame);
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        self.require_connection()?;

        loop {
            match self.script.get(self.position) {
                Some(Step::Emit(frame)) => {
                    let frame = frame.clone();
                    self.position += 1;
                    return Ok(frame);
                }
                // Consume the step, so a reconnect resumes after it rather than
                // dropping the link again forever.
                Some(Step::Drop(reason)) => {
                    let reason = reason.clone();
                    self.position += 1;
                    self.connected = false;
                    return Err(TransportError::Disconnected { reason });
                }
                // Wait for the caller to ask before answering. Yielding rather
                // than sleeping keeps tests quick; the caller runs on another
                // task, so it makes progress meanwhile.
                Some(&Step::AwaitSent(expected)) => {
                    if self.sent.len() >= expected {
                        self.position += 1;
                        continue;
                    }
                    tokio::task::yield_now().await;
                }
                // A finished script is an ended link, not an endless wait: the
                // caller must be able to decide something.
                None => {
                    self.connected = false;
                    return Err(TransportError::Disconnected {
                        reason: "mock script exhausted".into(),
                    });
                }
            }
        }
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emit(bytes: &[u8]) -> Step {
        Step::Emit(bytes.to_vec())
    }

    #[tokio::test]
    async fn replays_scripted_frames_in_order() {
        let mut transport = MockTransport::new(vec![emit(&[0x01]), emit(&[0x02, 0x03])]);
        transport.connect().await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0x01]);
        assert_eq!(transport.recv().await.unwrap(), vec![0x02, 0x03]);
    }

    #[tokio::test]
    async fn records_what_the_caller_sent() {
        let mut transport = MockTransport::new(vec![]);
        transport.connect().await.unwrap();

        transport.send(&[0x16, 0x03]).await.unwrap();
        transport.send(&[0x0A]).await.unwrap();

        assert_eq!(transport.sent(), vec![vec![0x16, 0x03], vec![0x0A]]);
    }

    #[tokio::test]
    async fn keeps_the_send_record_observable_after_giving_the_transport_away() {
        let mut transport = MockTransport::new(vec![]);
        let record = transport.sent_frames();
        assert!(record.is_empty());

        // Hand the transport to something that owns it from now on.
        let owner = tokio::spawn(async move {
            transport.connect().await.unwrap();
            transport.send(&[0x14]).await.unwrap();
        });
        owner.await.unwrap();

        assert_eq!(record.len(), 1);
        assert_eq!(record.snapshot(), vec![vec![0x14]]);
    }

    #[tokio::test]
    async fn refuses_to_work_before_connecting() {
        let mut transport = MockTransport::new(vec![emit(&[0x01])]);

        assert!(matches!(
            transport.send(&[0x01]).await,
            Err(TransportError::Disconnected { .. })
        ));
        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }

    #[tokio::test]
    async fn reports_the_scripted_drop() {
        let mut transport =
            MockTransport::new(vec![emit(&[0x01]), Step::Drop("cable pulled".into())]);
        transport.connect().await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0x01]);

        let error = transport.recv().await.unwrap_err();
        assert!(matches!(
            error,
            TransportError::Disconnected { ref reason } if reason == "cable pulled"
        ));
    }

    #[tokio::test]
    async fn stays_down_after_a_drop_until_reconnected() {
        let mut transport =
            MockTransport::new(vec![Step::Drop("cable pulled".into()), emit(&[0xAA])]);
        transport.connect().await.unwrap();

        transport.recv().await.unwrap_err();

        // Still down: sending must fail rather than silently succeed.
        assert!(matches!(
            transport.send(&[0x01]).await,
            Err(TransportError::Disconnected { .. })
        ));

        transport.connect().await.unwrap();
        assert_eq!(transport.recv().await.unwrap(), vec![0xAA]);
    }

    #[tokio::test]
    async fn holds_the_script_until_the_caller_has_sent() {
        let mut transport = MockTransport::new(vec![Step::AwaitSent(1), emit(&[0xAA])]);
        transport.connect().await.unwrap();

        // Nothing sent yet, so the answer must not arrive.
        let too_early =
            tokio::time::timeout(std::time::Duration::from_millis(50), transport.recv()).await;
        assert!(too_early.is_err(), "the answer came before the question");

        transport.send(&[0x16]).await.unwrap();
        assert_eq!(transport.recv().await.unwrap(), vec![0xAA]);
    }

    #[tokio::test]
    async fn rejects_the_configured_number_of_connection_attempts() {
        let mut transport = MockTransport::new(vec![emit(&[0x01])]).failing_connects(2);

        assert!(transport.connect().await.is_err());
        assert!(transport.connect().await.is_err());
        transport.connect().await.unwrap();

        assert_eq!(transport.rejected_connects(), 2);
        assert_eq!(
            transport.connect_count(),
            1,
            "only the successful one counts"
        );
        assert_eq!(transport.recv().await.unwrap(), vec![0x01]);
    }

    #[tokio::test]
    async fn counts_connections_so_reconnects_are_observable() {
        let mut transport = MockTransport::new(vec![Step::Drop("first".into())]);

        transport.connect().await.unwrap();
        transport.recv().await.unwrap_err();
        transport.connect().await.unwrap();

        assert_eq!(transport.connect_count(), 2);
    }

    #[tokio::test]
    async fn ends_the_link_when_the_script_runs_out() {
        let mut transport = MockTransport::new(vec![emit(&[0x01])]);
        transport.connect().await.unwrap();
        transport.recv().await.unwrap();

        // A finished script must not hang forever — the caller needs a decision.
        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }

    #[tokio::test]
    async fn disconnecting_twice_is_not_an_error() {
        let mut transport = MockTransport::new(vec![]);
        transport.connect().await.unwrap();

        transport.disconnect().await.unwrap();
        transport.disconnect().await.unwrap();
    }
}
