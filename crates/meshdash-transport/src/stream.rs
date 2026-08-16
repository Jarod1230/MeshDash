//! Framing over any byte stream.
//!
//! Serial and TCP differ in how a connection is opened, not in what travels
//! over it: both carry the same `[marker][len][payload]` framing. That part
//! lives here once, generic over anything that reads and writes bytes, so the
//! error-prone half can be tested in memory — no socket, no hardware.
//!
//! BLE will **not** use this type. Its frames are delimited by the
//! characteristic itself, which is exactly why the [`Transport`] trait carries
//! whole frames instead of bytes.

use meshdash_proto::frame::{self, Decoder};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Transport, TransportError};

/// How many bytes to take from the stream per read.
///
/// A node frame is at most 176 bytes plus a 3-byte header, so this holds
/// several frames without being wasteful.
const READ_CHUNK: usize = 1024;

/// Wraps a byte stream and turns it into a frame-oriented [`Transport`].
///
/// The decoder keeps its buffer across calls, so a frame split over several
/// reads is reassembled rather than lost.
#[derive(Debug)]
pub struct StreamTransport<S> {
    stream: Option<S>,
    decoder: Decoder,
}

impl<S> StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Wraps an already-open stream.
    ///
    /// Opening is the caller's business: a TCP connection and a serial port are
    /// established very differently, and this type is about what happens after.
    pub fn new(stream: S) -> Self {
        Self {
            stream: Some(stream),
            decoder: Decoder::new(),
        }
    }

    /// Fails unless the stream is still in place.
    fn stream_mut(&mut self) -> Result<&mut S, TransportError> {
        self.stream
            .as_mut()
            .ok_or_else(|| TransportError::Disconnected {
                reason: "stream was closed".into(),
            })
    }
}

#[async_trait::async_trait]
impl<S> Transport for StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// The stream was handed over already open, so there is nothing to do.
    /// Reopening belongs to whoever knows how to build the stream.
    async fn connect(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        let encoded = frame::encode(frame)?;
        let stream = self.stream_mut()?;
        stream.write_all(&encoded).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut chunk = [0u8; READ_CHUNK];

        loop {
            // Bytes from an earlier read may already hold a whole frame.
            if let Some(frame) = self.decoder.next_frame() {
                return Ok(frame);
            }

            let read = self.stream_mut()?.read(&mut chunk).await?;
            if read == 0 {
                self.stream = None;
                return Err(TransportError::Disconnected {
                    reason: "stream reached end of file".into(),
                });
            }

            self.decoder.push(&chunk[..read]);
        }
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        if let Some(mut stream) = self.stream.take() {
            // A failure here means the peer is already gone, which is what we
            // wanted anyway.
            let _ = stream.shutdown().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshdash_proto::frame::{MARKER_APP_TO_RADIO, MARKER_RADIO_TO_APP, MAX_FRAME_SIZE};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    /// Builds a frame the way the radio would send it.
    fn radio_frame(payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(payload.len()).unwrap();
        let mut frame = vec![MARKER_RADIO_TO_APP];
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[tokio::test]
    async fn writes_a_properly_framed_command() {
        let (ours, mut theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        transport.send(&[0x16, 0x03]).await.unwrap();

        let mut seen = [0u8; 5];
        theirs.read_exact(&mut seen).await.unwrap();
        assert_eq!(seen, [MARKER_APP_TO_RADIO, 0x02, 0x00, 0x16, 0x03]);
    }

    #[tokio::test]
    async fn reads_a_frame_arriving_whole() {
        let (ours, mut theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        theirs.write_all(&radio_frame(&[0x0C, 0xFF])).await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0x0C, 0xFF]);
    }

    #[tokio::test]
    async fn reassembles_a_frame_split_across_reads() {
        let (ours, mut theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);
        let frame = radio_frame(&[0x01, 0x02, 0x03, 0x04]);

        // Deliver the frame in two chunks, splitting inside the payload.
        theirs.write_all(&frame[..4]).await.unwrap();
        theirs.flush().await.unwrap();
        theirs.write_all(&frame[4..]).await.unwrap();

        assert_eq!(
            transport.recv().await.unwrap(),
            vec![0x01, 0x02, 0x03, 0x04]
        );
    }

    #[tokio::test]
    async fn returns_frames_one_at_a_time_from_a_single_read() {
        let (ours, mut theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        let mut both = radio_frame(&[0xAA]);
        both.extend_from_slice(&radio_frame(&[0xBB, 0xCC]));
        theirs.write_all(&both).await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0xAA]);
        assert_eq!(transport.recv().await.unwrap(), vec![0xBB, 0xCC]);
    }

    #[tokio::test]
    async fn skips_console_noise_before_a_frame() {
        let (ours, mut theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        let mut stream = b"boot: ready\r\n".to_vec();
        stream.extend_from_slice(&radio_frame(&[0x42]));
        theirs.write_all(&stream).await.unwrap();

        assert_eq!(transport.recv().await.unwrap(), vec![0x42]);
    }

    #[tokio::test]
    async fn reports_a_closed_stream_as_disconnected() {
        let (ours, theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        drop(theirs);

        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }

    #[tokio::test]
    async fn refuses_a_payload_the_node_would_drop() {
        let (ours, _theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        let error = transport.send(&[0u8; MAX_FRAME_SIZE + 1]).await;

        assert!(matches!(error, Err(TransportError::Frame(_))));
    }

    #[tokio::test]
    async fn refuses_to_work_after_disconnecting() {
        let (ours, _theirs) = duplex(4096);
        let mut transport = StreamTransport::new(ours);

        transport.disconnect().await.unwrap();

        assert!(matches!(
            transport.send(&[0x01]).await,
            Err(TransportError::Disconnected { .. })
        ));
        assert!(matches!(
            transport.recv().await,
            Err(TransportError::Disconnected { .. })
        ));
    }
}
