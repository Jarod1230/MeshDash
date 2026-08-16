//! USB/serial connection to a companion node.
//!
//! A node on the USB port appears as `/dev/ttyUSB0` or `/dev/ttyACM0` on Linux,
//! as `/dev/cu.usbserial-*` on macOS and as `COM3` and friends on Windows.
//! Everything that happens *after* the port is open — framing, reassembly,
//! resynchronisation — lives in [`StreamTransport`] and is shared with TCP.
//!
//! # Testing without hardware
//!
//! Only the parts that do not need a port are covered here: rejecting a device
//! that is not there, and refusing to work before connecting. The framing is
//! exercised in `stream.rs` against an in-memory pipe, so the untested surface
//! is exactly the call that opens the port — see `docs/testing.md`, section
//! "Was bewusst ungetestet bleibt".

use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::{Transport, TransportError, stream::StreamTransport};

/// Baud rate a companion node runs its USB serial link at.
///
/// Source: `Serial.begin(115200)` in firmware
/// `examples/companion_radio/main.cpp`, MeshCore commit `d929643`.
pub const DEFAULT_BAUD_RATE: u32 = 115_200;

/// A companion node attached to a serial port.
///
/// Keeps the port path so [`Transport::connect`] can reopen it — a node that
/// is unplugged and plugged back in must not require a restart.
#[derive(Debug)]
pub struct SerialTransport {
    path: String,
    baud_rate: u32,
    inner: Option<StreamTransport<SerialStream>>,
}

impl SerialTransport {
    /// Names a port without opening it.
    pub fn new(path: impl Into<String>, baud_rate: u32) -> Self {
        Self {
            path: path.into(),
            baud_rate,
            inner: None,
        }
    }

    /// Names a port at the node's usual baud rate, see [`DEFAULT_BAUD_RATE`].
    pub fn with_default_baud_rate(path: impl Into<String>) -> Self {
        Self::new(path, DEFAULT_BAUD_RATE)
    }

    /// The port this transport opens.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The configured baud rate.
    pub fn baud_rate(&self) -> u32 {
        self.baud_rate
    }

    /// Fails unless the port is open.
    fn inner_mut(&mut self) -> Result<&mut StreamTransport<SerialStream>, TransportError> {
        self.inner
            .as_mut()
            .ok_or_else(|| TransportError::Disconnected {
                reason: format!("serial port {} is not open", self.path),
            })
    }
}

#[async_trait::async_trait]
impl Transport for SerialTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        // Release the old handle first: on a replugged device the stale one
        // points at a port that no longer exists.
        self.inner = None;

        // Keep the crate's own error type out of the public surface: the
        // underlying kind (not found, permission denied) survives the
        // conversion, which is what a diagnosis needs.
        let port = tokio_serial::new(&self.path, self.baud_rate)
            .open_native_async()
            .map_err(std::io::Error::from)?;

        tracing::debug!(
            path = %self.path,
            baud_rate = self.baud_rate,
            "opened serial port to node"
        );
        self.inner = Some(StreamTransport::new(port));
        Ok(())
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.inner_mut()?.send(frame).await
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let result = self.inner_mut()?.recv().await;

        // A pulled cable ends this handle for good; reopening is the caller's
        // job and needs a fresh one.
        if matches!(result, Err(TransportError::Disconnected { .. })) {
            self.inner = None;
        }

        result
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        if let Some(mut inner) = self.inner.take() {
            inner.disconnect().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path that cannot exist as a serial device on any supported platform.
    const MISSING_PORT: &str = "/dev/meshdash-does-not-exist";

    #[test]
    fn keeps_the_port_it_was_given() {
        let transport = SerialTransport::new("/dev/ttyUSB0", 9600);

        assert_eq!(transport.path(), "/dev/ttyUSB0");
        assert_eq!(transport.baud_rate(), 9600);
    }

    #[test]
    fn defaults_to_the_rate_the_firmware_uses() {
        let transport = SerialTransport::with_default_baud_rate("/dev/ttyACM0");

        assert_eq!(transport.baud_rate(), 115_200);
    }

    #[tokio::test]
    async fn reports_a_port_that_is_not_there() {
        let mut transport = SerialTransport::with_default_baud_rate(MISSING_PORT);

        assert!(transport.connect().await.is_err());
    }

    #[tokio::test]
    async fn refuses_to_work_before_the_port_is_open() {
        let mut transport = SerialTransport::with_default_baud_rate(MISSING_PORT);

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
    async fn disconnecting_an_unopened_port_is_not_an_error() {
        let mut transport = SerialTransport::with_default_baud_rate(MISSING_PORT);

        transport.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn stays_unusable_after_a_failed_connect() {
        let mut transport = SerialTransport::with_default_baud_rate(MISSING_PORT);

        transport.connect().await.unwrap_err();

        // A failed open must not leave a half-usable transport behind.
        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }
}
