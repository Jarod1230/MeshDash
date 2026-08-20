//! Asking another node something, and matching up its answer.
//!
//! # Layout
//!
//! Source: the `CMD_SEND_BINARY_REQ` branch of `handleCmdFrame()` and
//! `onContactResponse()` in `examples/companion_radio/MyMesh.cpp`, MeshCore
//! commit `d929643`; the request types from `simple_repeater/MyMesh.cpp` and
//! `simple_sensor/SensorMesh.cpp` of the same commit, the permission bits from
//! `src/helpers/SensorManager.h`.
//!
//! ```text
//! CMD_SEND_BINARY_REQ
//! offset  size  field
//!      0     1  opcode
//!      1    32  the recipient's full public key — not the six-byte prefix
//!     33     n  request body, first byte is the request type
//!
//! PUSH_CODE_BINARY_RESPONSE — arrives later, when the other node replies
//!      0     1  opcode
//!      1     1  reserved
//!      2     4  tag (u32 little-endian), matching the RESP_CODE_SENT receipt
//!      6     n  payload
//! ```
//!
//! # The answer does not say who sent it
//!
//! Only the tag comes back. Whoever asks has to remember which contact a tag
//! belongs to; the deprecated `CMD_SEND_TELEMETRY_REQ` carried a key prefix
//! instead, and that convenience is gone.
//!
//! # A repeated question needs to look different
//!
//! The firmware appends four random bytes to a telemetry request, with the
//! comment "random blob to help make packet-hash unique". Without them two
//! identical requests hash the same and the second is dropped as a duplicate —
//! so polling a node every few minutes would work exactly once.

use crate::opcode::{Command, Push};

/// What kind of question is being asked.
///
/// Source: `REQ_TYPE_*` in `simple_repeater/MyMesh.cpp`, commit `d929643`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// Sensor readings, answered as CayenneLPP — see [`crate::lpp`].
    Telemetry,
}

impl RequestType {
    /// The value as it travels.
    pub fn as_byte(self) -> u8 {
        match self {
            // REQ_TYPE_GET_TELEMETRY_DATA
            Self::Telemetry => 0x03,
        }
    }
}

/// Which groups of readings are being asked for.
///
/// Source: `TELEM_PERM_*` in `src/helpers/SensorManager.h`, commit `d929643`.
/// The answering node may still refuse: it applies its own configuration on
/// top, so asking for everything is not the same as getting everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Telemetry {
    /// Battery and the basics.
    pub base: bool,
    /// Position.
    pub location: bool,
    /// Attached environment sensors.
    pub environment: bool,
}

impl Telemetry {
    /// Everything the other node is willing to share.
    pub const ALL: Self = Self {
        base: true,
        location: true,
        environment: true,
    };

    /// Only battery and the basics.
    pub const BASE: Self = Self {
        base: true,
        location: false,
        environment: false,
    };

    /// The permission mask as the firmware spells it.
    fn mask(self) -> u8 {
        let mut mask = 0;
        if self.base {
            mask |= 0x01; // TELEM_PERM_BASE
        }
        if self.location {
            mask |= 0x02; // TELEM_PERM_LOCATION
        }
        if self.environment {
            mask |= 0x04; // TELEM_PERM_ENVIRONMENT
        }
        mask
    }
}

/// The LPP channel a node uses for its own readings, as opposed to a sensor's.
///
/// Source: `TELEM_CHANNEL_SELF` in `src/helpers/SensorManager.h`.
pub const CHANNEL_SELF: u8 = 1;

/// Builds a telemetry request for one node.
///
/// `nonce` must differ between requests to the same node; see the note about
/// duplicate packet hashes above. It is taken as an argument rather than
/// generated here so this crate stays free of randomness and stays testable.
pub fn encode_telemetry_request(
    recipient_key: &[u8; 32],
    wanted: Telemetry,
    nonce: [u8; 4],
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(42);
    frame.push(u8::from(Command::SendBinaryReq));
    frame.extend_from_slice(recipient_key);
    frame.push(RequestType::Telemetry.as_byte());
    // Inverted: the firmware applies `~mask` to its own permissions.
    frame.push(!wanted.mask());
    frame.extend_from_slice(&[0, 0, 0]);
    frame.extend_from_slice(&nonce);

    frame
}

/// A late answer to a request, matched by its tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryResponse {
    /// Matches the tag from the `RESP_CODE_SENT` receipt.
    pub tag: u32,
    /// Whatever the other node sent back.
    pub payload: Vec<u8>,
}

/// Why a response could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseError {
    /// The frame is not a binary response.
    #[error("expected a binary response, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The frame ends before the tag does.
    #[error("binary response is {len} bytes, need at least {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl BinaryResponse {
    /// Reads `PUSH_CODE_BINARY_RESPONSE`, opcode byte included.
    pub fn parse(frame: &[u8]) -> Result<Self, ResponseError> {
        /// Opcode, reserved byte and the tag.
        const HEADER: usize = 6;

        match frame.first().map(|&byte| Push::from(byte)) {
            Some(Push::BinaryResponse) => {}
            Some(_) => return Err(ResponseError::WrongOpcode { opcode: frame[0] }),
            None => {
                return Err(ResponseError::TooShort {
                    len: 0,
                    needed: HEADER,
                });
            }
        }

        if frame.len() < HEADER {
            return Err(ResponseError::TooShort {
                len: frame.len(),
                needed: HEADER,
            });
        }

        Ok(Self {
            tag: u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]),
            payload: frame[HEADER..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_a_telemetry_request_as_the_firmware_reads_it() {
        let frame = encode_telemetry_request(&[0xAB; 32], Telemetry::ALL, [1, 2, 3, 4]);

        assert_eq!(frame[0], u8::from(Command::SendBinaryReq));
        assert_eq!(&frame[1..33], &[0xAB; 32], "the full key, not a prefix");
        assert_eq!(frame[33], 0x03, "REQ_TYPE_GET_TELEMETRY_DATA");
        assert_eq!(frame[35..38], [0, 0, 0], "three reserved bytes");
        assert_eq!(&frame[38..42], &[1, 2, 3, 4], "the nonce");
        assert_eq!(frame.len(), 42);
    }

    #[test]
    fn asks_with_an_inverted_permission_mask() {
        // The firmware sends ~(wanted), not the wanted bits themselves.
        let all = encode_telemetry_request(&[0; 32], Telemetry::ALL, [0; 4]);
        let base = encode_telemetry_request(&[0; 32], Telemetry::BASE, [0; 4]);

        assert_eq!(all[34], !0x07u8);
        assert_eq!(base[34], !0x01u8);
    }

    #[test]
    fn two_requests_with_different_nonces_differ() {
        // Identical packets hash the same and the second is dropped as a
        // duplicate, so polling would work exactly once.
        let first = encode_telemetry_request(&[0xAB; 32], Telemetry::ALL, [1, 1, 1, 1]);
        let second = encode_telemetry_request(&[0xAB; 32], Telemetry::ALL, [2, 2, 2, 2]);

        assert_ne!(first, second);
    }

    #[test]
    fn reads_a_response_and_its_tag() {
        let mut frame = vec![u8::from(Push::BinaryResponse), 0];
        frame.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        frame.extend_from_slice(&[1, 116, 0x01, 0x92]);

        assert_eq!(
            BinaryResponse::parse(&frame),
            Ok(BinaryResponse {
                tag: 0x1234_5678,
                payload: vec![1, 116, 0x01, 0x92],
            })
        );
    }

    #[test]
    fn accepts_a_response_with_an_empty_payload() {
        let mut frame = vec![u8::from(Push::BinaryResponse), 0];
        frame.extend_from_slice(&7_u32.to_le_bytes());

        let response = BinaryResponse::parse(&frame).unwrap();

        assert_eq!(response.tag, 7);
        assert!(response.payload.is_empty());
    }

    #[test]
    fn refuses_a_frame_that_is_not_a_response() {
        assert_eq!(
            BinaryResponse::parse(&[u8::from(Push::Advert), 0, 0, 0, 0, 0]),
            Err(ResponseError::WrongOpcode { opcode: 0x80 })
        );
    }

    #[test]
    fn refuses_a_frame_that_ends_inside_the_tag() {
        assert_eq!(
            BinaryResponse::parse(&[u8::from(Push::BinaryResponse), 0, 1, 2]),
            Err(ResponseError::TooShort { len: 4, needed: 6 })
        );
    }
}
