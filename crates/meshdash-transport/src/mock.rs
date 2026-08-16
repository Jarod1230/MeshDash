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

use crate::{Transport, TransportError};

/// One step in a mock script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The node emits this frame.
    Emit(Vec<u8>),
    /// The link drops, with the given reason. Everything after this step is
    /// only reachable after a reconnect.
    Drop(String),
}

/// A scripted stand-in for a companion node.
#[derive(Debug, Default)]
pub struct MockTransport {
    script: Vec<Step>,
    position: usize,
    connected: bool,
    sent: Vec<Vec<u8>>,
    connect_count: usize,
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
    pub fn sent(&self) -> &[Vec<u8>] {
        &self.sent
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
        self.connected = true;
        self.connect_count += 1;
        Ok(())
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.require_connection()?;
        self.sent.push(frame.to_vec());
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        self.require_connection()?;

        match self.script.get(self.position) {
            Some(Step::Emit(frame)) => {
                self.position += 1;
                Ok(frame.clone())
            }
            // Consume the step, so a reconnect resumes after it rather than
            // dropping the link again forever.
            Some(Step::Drop(reason)) => {
                let reason = reason.clone();
                self.position += 1;
                self.connected = false;
                Err(TransportError::Disconnected { reason })
            }
            // A finished script is an ended link, not an endless wait: the
            // caller must be able to decide something.
            None => {
                self.connected = false;
                Err(TransportError::Disconnected {
                    reason: "mock script exhausted".into(),
                })
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

        assert_eq!(transport.sent(), &[vec![0x16, 0x03], vec![0x0A]]);
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
