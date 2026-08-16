//! TCP connection to a companion node over WiFi.
//!
//! The node listens on a port and speaks the same framing as the serial link —
//! the firmware reuses the serial header deliberately so a client can delimit
//! frames the same way. All of that lives in [`StreamTransport`]; this module
//! only knows how to open and reopen the socket.

use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::{Transport, TransportError, stream::StreamTransport};

/// A companion node reachable over TCP.
///
/// Holds the address so [`Transport::connect`] can be called again after a
/// drop — which is the whole point of keeping reconnection in the transport.
#[derive(Debug)]
pub struct TcpTransport {
    address: SocketAddr,
    inner: Option<StreamTransport<TcpStream>>,
}

impl TcpTransport {
    /// Names a node without contacting it yet.
    ///
    /// Nothing happens on the network until [`Transport::connect`] is called,
    /// so constructing this cannot fail.
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            inner: None,
        }
    }

    /// The address this transport connects to.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Fails unless a connection has been established.
    fn inner_mut(&mut self) -> Result<&mut StreamTransport<TcpStream>, TransportError> {
        self.inner
            .as_mut()
            .ok_or_else(|| TransportError::Disconnected {
                reason: "not connected yet".into(),
            })
    }
}

#[async_trait::async_trait]
impl Transport for TcpTransport {
    async fn connect(&mut self) -> Result<(), TransportError> {
        // Drop any previous socket first, so a reconnect never leaves two.
        self.inner = None;

        let socket = TcpStream::connect(self.address).await?;

        // Frames are small and answers are awaited, so waiting to coalesce
        // writes only adds latency.
        socket.set_nodelay(true)?;

        tracing::debug!(address = %self.address, "connected to node over TCP");
        self.inner = Some(StreamTransport::new(socket));
        Ok(())
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.inner_mut()?.send(frame).await
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let result = self.inner_mut()?.recv().await;

        // A dead link stays dead until someone reconnects; holding on to the
        // socket would only produce confusing follow-up errors.
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
    use meshdash_proto::frame::{MARKER_APP_TO_RADIO, MARKER_RADIO_TO_APP};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn radio_frame(payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(payload.len()).unwrap();
        let mut frame = vec![MARKER_RADIO_TO_APP];
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    /// Starts a listener on a free port, standing in for the node.
    async fn fake_node() -> (TcpListener, SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        (listener, address)
    }

    #[tokio::test]
    async fn exchanges_frames_with_a_listening_node() {
        let (listener, address) = fake_node().await;

        let node = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            let mut seen = [0u8; 4];
            socket.read_exact(&mut seen).await.unwrap();

            socket.write_all(&radio_frame(&[0x0C, 0x01])).await.unwrap();
            seen
        });

        let mut transport = TcpTransport::new(address);
        transport.connect().await.unwrap();
        transport.send(&[0x14]).await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0x0C, 0x01]);
        assert_eq!(node.await.unwrap(), [MARKER_APP_TO_RADIO, 0x01, 0x00, 0x14]);
    }

    #[tokio::test]
    async fn reports_a_refused_connection() {
        let (listener, address) = fake_node().await;
        drop(listener);

        let mut transport = TcpTransport::new(address);

        assert!(transport.connect().await.is_err());
    }

    #[tokio::test]
    async fn notices_when_the_node_hangs_up() {
        let (listener, address) = fake_node().await;

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(socket);
        });

        let mut transport = TcpTransport::new(address);
        transport.connect().await.unwrap();

        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }

    #[tokio::test]
    async fn can_be_reconnected_after_a_drop() {
        let (listener, address) = fake_node().await;

        tokio::spawn(async move {
            // Accept twice: the client is expected to come back.
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                socket.write_all(&radio_frame(&[0xAA])).await.unwrap();
                socket.flush().await.unwrap();
            }
        });

        let mut transport = TcpTransport::new(address);
        transport.connect().await.unwrap();
        assert_eq!(transport.recv().await.unwrap(), vec![0xAA]);

        transport.disconnect().await.unwrap();
        transport.connect().await.unwrap();
        assert_eq!(transport.recv().await.unwrap(), vec![0xAA]);
    }

    #[tokio::test]
    async fn refuses_to_work_before_connecting() {
        let (_listener, address) = fake_node().await;
        let mut transport = TcpTransport::new(address);

        assert!(matches!(
            transport.send(&[0x01]).await,
            Err(TransportError::Disconnected { .. })
        ));
    }
}
