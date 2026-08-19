//! Frame layer for the serial and TCP transports.
//!
//! A byte stream carries no frame boundaries, so every frame is prefixed with a
//! direction marker and a length:
//!
//! ```text
//! [ marker: u8 ][ len: u16 little-endian ][ payload: len bytes ]
//! ```
//!
//! BLE does **not** use this layer — there the characteristic delimits frames.
//! See `docs/research/meshcore-companion-protocol.md`.

/// Marker byte starting a frame sent from the app to the radio.
///
/// Source: firmware `ArduinoSerialInterface::checkRecvFrame()` matches `'<'`,
/// and `SerialWifiInterface::checkRecvFrame()` states it outright: "'<' is 0x3c
/// which indicates a frame sent from app to radio".
/// MeshCore commit d929643.
pub const MARKER_APP_TO_RADIO: u8 = b'<';

/// Marker byte starting a frame sent from the radio to the app.
///
/// Source: firmware `ArduinoSerialInterface::writeFrame()` and
/// `SerialWifiInterface::checkRecvFrame()` both emit `'>'`.
/// MeshCore commit d929643.
pub const MARKER_RADIO_TO_APP: u8 = b'>';

/// Largest payload the node accepts or emits.
///
/// Source: `MAX_FRAME_SIZE` in firmware `BaseSerialInterface.h`, MeshCore
/// commit d929643, together with `uint8_t rx_buf[MAX_FRAME_SIZE]` in
/// `ArduinoSerialInterface.h` — the receive buffer is exactly this size, and
/// the length field is not counted in it.
///
/// # An oversized frame is truncated, not dropped
///
/// `ArduinoSerialInterface::checkRecvFrame()` keeps reading past the buffer and
/// then cuts the length down:
///
/// ```text
/// if (_frame_len > MAX_FRAME_SIZE) _frame_len = MAX_FRAME_SIZE;    // truncate
/// ```
///
/// So a frame that is too long does not vanish — the node processes its first
/// 176 bytes as though that were the whole thing. For a text message that means
/// a silently shortened message; for anything with fields after the text it
/// would mean garbage. Nothing may be sent that exceeds this, and the sender
/// must not rely on the node rejecting it.
pub const MAX_FRAME_SIZE: usize = 176;

/// Errors produced while encoding a frame.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncodeError {
    /// Payload exceeds what the node accepts, see [`MAX_FRAME_SIZE`].
    #[error("payload of {len} bytes exceeds the maximum frame size of {MAX_FRAME_SIZE}")]
    PayloadTooLarge {
        /// Length that was rejected.
        len: usize,
    },
}

/// Wraps a payload in a frame addressed to the radio.
///
/// The length field counts the payload only — it excludes the marker and the
/// length field itself. There is no checksum. Both verified against the
/// firmware source, see the module documentation.
pub fn encode(payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    if payload.len() > MAX_FRAME_SIZE {
        return Err(EncodeError::PayloadTooLarge { len: payload.len() });
    }

    // The cast is safe: the length is bounded by MAX_FRAME_SIZE above.
    let len = payload.len() as u16;

    let mut frame = Vec::with_capacity(3 + payload.len());
    frame.push(MARKER_APP_TO_RADIO);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Largest frame length this decoder considers plausible on the wire.
///
/// Deliberately larger than [`MAX_FRAME_SIZE`]: that constant is what the node
/// accepts, and clamping reception to it would drop frames a future firmware
/// might legitimately send. The value matches the reference implementation,
/// which treats anything above it as a desynchronised stream.
///
/// Source: `meshcore_py` `SerialConnection.handle_rx()`, commit c487efb.
const MAX_PLAUSIBLE_FRAME_SIZE: usize = 300;

/// Number of bytes preceding the payload: marker plus the 16-bit length.
const HEADER_LEN: usize = 3;

/// Reassembles frames sent by the radio from an arbitrarily chunked byte stream.
///
/// A stream delivers no frame boundaries, so bytes are buffered until a whole
/// frame is available. The decoder is deliberately forgiving about noise:
///
/// - Bytes before a [`MARKER_RADIO_TO_APP`] are discarded. Some nodes interleave
///   console output on the same UART.
/// - A frame announcing an implausible length is dropped and the search for the
///   next marker resumes, rather than blocking the stream forever.
///
/// Both behaviours follow the reference implementation; see
/// [`MAX_PLAUSIBLE_FRAME_SIZE`].
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    /// Creates an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends freshly received bytes to the internal buffer.
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Removes and returns the next complete frame's payload, if one is buffered.
    ///
    /// Returns `None` when more bytes are needed. Call it in a loop until it
    /// returns `None` — a single [`push`](Self::push) can complete several frames.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        loop {
            match self.buf.iter().position(|&b| b == MARKER_RADIO_TO_APP) {
                // Everything before the marker is noise, and without a marker
                // the whole buffer is. Dropping it keeps the buffer from
                // growing without bound on a chatty console.
                None => {
                    self.buf.clear();
                    return None;
                }
                Some(0) => {}
                Some(marker_at) => {
                    self.buf.drain(..marker_at);
                }
            }

            if self.buf.len() < HEADER_LEN {
                return None;
            }

            let len = usize::from(u16::from_le_bytes([self.buf[1], self.buf[2]]));

            // An implausible length means this marker was payload, not a
            // header. Skip past it and look for the next one.
            if len > MAX_PLAUSIBLE_FRAME_SIZE {
                self.buf.drain(..1);
                continue;
            }

            let frame_end = HEADER_LEN + len;
            if self.buf.len() < frame_end {
                return None;
            }

            let payload = self.buf[HEADER_LEN..frame_end].to_vec();
            self.buf.drain(..frame_end);
            return Some(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a frame as the radio would send it, for feeding the decoder.
    fn radio_frame(payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(payload.len()).unwrap();
        let mut frame = vec![MARKER_RADIO_TO_APP];
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn encodes_marker_length_and_payload() {
        let frame = encode(&[0x01, 0x02, 0x03]).unwrap();

        assert_eq!(frame, vec![b'<', 0x03, 0x00, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn encodes_length_as_little_endian() {
        let frame = encode(&[0xAA; 130]).unwrap();

        // 130 = 0x0082, low byte first.
        assert_eq!(&frame[0..3], &[b'<', 0x82, 0x00]);
        assert_eq!(frame.len(), 3 + 130);
    }

    #[test]
    fn encodes_an_empty_payload_as_a_bare_header() {
        let frame = encode(&[]).unwrap();

        assert_eq!(frame, vec![b'<', 0x00, 0x00]);
    }

    #[test]
    fn rejects_a_payload_the_node_would_drop() {
        let too_big = vec![0x00; MAX_FRAME_SIZE + 1];

        let error = encode(&too_big).unwrap_err();

        assert_eq!(
            error,
            EncodeError::PayloadTooLarge {
                len: MAX_FRAME_SIZE + 1
            }
        );
    }

    #[test]
    fn accepts_a_payload_of_exactly_the_maximum_size() {
        let frame = encode(&[0x00; MAX_FRAME_SIZE]).unwrap();

        assert_eq!(frame.len(), 3 + MAX_FRAME_SIZE);
    }

    #[test]
    fn decodes_a_frame_arriving_in_one_piece() {
        let mut decoder = Decoder::new();

        decoder.push(&radio_frame(&[0x0C, 0xFF]));

        assert_eq!(decoder.next_frame(), Some(vec![0x0C, 0xFF]));
        assert_eq!(decoder.next_frame(), None);
    }

    #[test]
    fn waits_for_a_payload_split_across_pushes() {
        let mut decoder = Decoder::new();
        let frame = radio_frame(&[0x01, 0x02, 0x03, 0x04]);

        decoder.push(&frame[..5]);
        assert_eq!(decoder.next_frame(), None, "payload is still incomplete");

        decoder.push(&frame[5..]);
        assert_eq!(decoder.next_frame(), Some(vec![0x01, 0x02, 0x03, 0x04]));
    }

    #[test]
    fn waits_for_a_header_split_across_pushes() {
        let mut decoder = Decoder::new();
        let frame = radio_frame(&[0xAB]);

        // Only the marker and the low length byte.
        decoder.push(&frame[..2]);
        assert_eq!(decoder.next_frame(), None, "header is still incomplete");

        decoder.push(&frame[2..]);
        assert_eq!(decoder.next_frame(), Some(vec![0xAB]));
    }

    #[test]
    fn decodes_several_frames_from_a_single_push() {
        let mut decoder = Decoder::new();
        let mut stream = radio_frame(&[0x11]);
        stream.extend_from_slice(&radio_frame(&[0x22, 0x33]));

        decoder.push(&stream);

        assert_eq!(decoder.next_frame(), Some(vec![0x11]));
        assert_eq!(decoder.next_frame(), Some(vec![0x22, 0x33]));
        assert_eq!(decoder.next_frame(), None);
    }

    #[test]
    fn decodes_an_empty_frame() {
        let mut decoder = Decoder::new();

        decoder.push(&radio_frame(&[]));

        assert_eq!(decoder.next_frame(), Some(vec![]));
    }

    #[test]
    fn discards_console_noise_before_the_marker() {
        let mut decoder = Decoder::new();
        let mut stream = b"boot: radio ready\r\n".to_vec();
        stream.extend_from_slice(&radio_frame(&[0x42]));

        decoder.push(&stream);

        assert_eq!(decoder.next_frame(), Some(vec![0x42]));
    }

    #[test]
    fn ignores_a_frame_addressed_to_the_radio() {
        let mut decoder = Decoder::new();

        // 0x3C is the app-to-radio direction; we must not read our own echo.
        decoder.push(&encode(&[0x99]).unwrap());

        assert_eq!(decoder.next_frame(), None);
    }

    #[test]
    fn resynchronises_after_an_implausible_length() {
        let mut decoder = Decoder::new();
        let mut stream = vec![
            MARKER_RADIO_TO_APP,
            0xFF,
            0xFF, // announces 65535 bytes — desynchronised
        ];
        stream.extend_from_slice(&radio_frame(&[0x77]));

        decoder.push(&stream);

        assert_eq!(
            decoder.next_frame(),
            Some(vec![0x77]),
            "the following frame must still be found"
        );
    }

    #[test]
    fn returns_nothing_for_a_stream_without_a_marker() {
        let mut decoder = Decoder::new();

        decoder.push(b"no frame in here at all");

        assert_eq!(decoder.next_frame(), None);
    }

    #[test]
    fn decodes_a_payload_larger_than_the_node_accepts() {
        // Reception must not clamp to MAX_FRAME_SIZE, see MAX_PLAUSIBLE_FRAME_SIZE.
        let payload = vec![0x5A; MAX_FRAME_SIZE + 20];
        let mut decoder = Decoder::new();

        decoder.push(&radio_frame(&payload));

        assert_eq!(decoder.next_frame(), Some(payload));
    }

    #[test]
    fn round_trips_payloads_of_every_interesting_length() {
        // Around the node's limit and up to the decoder's plausibility bound.
        for len in [
            0,
            1,
            2,
            MAX_FRAME_SIZE - 1,
            MAX_FRAME_SIZE,
            MAX_FRAME_SIZE + 1,
            MAX_PLAUSIBLE_FRAME_SIZE,
        ] {
            let payload: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let mut decoder = Decoder::new();

            decoder.push(&radio_frame(&payload));

            assert_eq!(
                decoder.next_frame(),
                Some(payload),
                "payload of length {len} did not survive the round trip"
            );
            assert_eq!(
                decoder.next_frame(),
                None,
                "leftover bytes for length {len}"
            );
        }
    }

    #[test]
    fn encoded_frames_carry_the_length_the_decoder_would_read() {
        for len in [0, 1, MAX_FRAME_SIZE] {
            let payload = vec![0xC3; len];

            let frame = encode(&payload).unwrap();

            let announced = u16::from_le_bytes([frame[1], frame[2]]);
            assert_eq!(usize::from(announced), len);
            assert_eq!(frame.len(), HEADER_LEN + len);
            assert_eq!(&frame[HEADER_LEN..], &payload[..]);
        }
    }

    #[test]
    fn drops_a_frame_one_byte_beyond_the_plausible_length() {
        let mut decoder = Decoder::new();
        let announced = u16::try_from(MAX_PLAUSIBLE_FRAME_SIZE + 1).unwrap();
        let mut stream = vec![MARKER_RADIO_TO_APP];
        stream.extend_from_slice(&announced.to_le_bytes());
        stream.extend_from_slice(&vec![0x00; MAX_PLAUSIBLE_FRAME_SIZE + 1]);
        stream.extend_from_slice(&radio_frame(&[0x55]));

        decoder.push(&stream);

        // The oversized frame is skipped; the well-formed one behind it survives.
        assert_eq!(decoder.next_frame(), Some(vec![0x55]));
    }

    #[test]
    fn does_not_grow_the_buffer_on_endless_noise() {
        let mut decoder = Decoder::new();

        for _ in 0..100 {
            decoder.push(b"chatty console output without any frame marker\r\n");
            assert_eq!(decoder.next_frame(), None);
        }

        assert!(
            decoder.buf.is_empty(),
            "noise must not accumulate, buffer holds {} bytes",
            decoder.buf.len()
        );
    }

    #[test]
    fn decodes_a_frame_fed_one_byte_at_a_time() {
        let mut decoder = Decoder::new();
        let frame = radio_frame(&[0xDE, 0xAD, 0xBE, 0xEF]);

        let mut decoded = None;
        for byte in &frame {
            decoder.push(&[*byte]);
            if let Some(payload) = decoder.next_frame() {
                decoded = Some(payload);
            }
        }

        assert_eq!(decoded, Some(vec![0xDE, 0xAD, 0xBE, 0xEF]));
    }
}
