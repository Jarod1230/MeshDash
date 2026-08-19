//! Sending text, and what the node answers.
//!
//! This is the first thing MeshDash asks a node to *do* rather than report.
//!
//! # Layout
//!
//! Source: the `CMD_SEND_TXT_MSG` and `CMD_SEND_CHANNEL_TXT_MSG` branches of
//! `handleCmdFrame()` in `examples/companion_radio/MyMesh.cpp`, MeshCore commit
//! `d929643`.
//!
//! ```text
//! CMD_SEND_TXT_MSG
//! offset  size  field
//!      0     1  opcode
//!      1     1  text type
//!      2     1  attempt number
//!      3     4  message timestamp (u32 little-endian)
//!      7     6  recipient key prefix  ← six bytes, as everywhere else
//!     13     …  text, to the end of the frame, not terminated
//!
//! CMD_SEND_CHANNEL_TXT_MSG
//!      0     1  opcode
//!      1     1  text type, must be plain
//!      2     1  channel index
//!      3     4  message timestamp (u32 little-endian)
//!      7     …  text
//!
//! RESP_CODE_SENT — the answer to a direct message, ten bytes
//!      0     1  opcode
//!      1     1  1 if it went out as a flood, 0 if it took a known route
//!      2     4  expected acknowledgement (u32 little-endian), 0 for none
//!      6     4  estimated timeout in milliseconds (u32 little-endian)
//! ```
//!
//! # A channel message is not acknowledged
//!
//! The firmware answers `CMD_SEND_CHANNEL_TXT_MSG` with a plain
//! `RESP_CODE_OK`, not with `RESP_CODE_SENT`. Nobody acknowledges a broadcast,
//! so there is no delivery to wait for and no round-trip to measure. Expecting
//! a receipt here would wait forever.
//!
//! # An empty text is refused rather than sent
//!
//! The firmware takes the `CMD_SEND_TXT_MSG` branch only for frames of at least
//! 14 bytes — thirteen of header plus at least one byte of text. An empty
//! message would fall through the whole command chain and be answered as an
//! unsupported command, which reads like a protocol error rather than an empty
//! message. Refusing it here says what actually happened.

use crate::{frame::MAX_FRAME_SIZE, message::TextType, opcode::Command, opcode::Response};

/// Offsets of the two send commands.
mod layout {
    /// Header of `CMD_SEND_TXT_MSG`, up to where the text starts.
    pub const DIRECT_HEADER: usize = 13;
    /// Header of `CMD_SEND_CHANNEL_TXT_MSG`.
    pub const CHANNEL_HEADER: usize = 7;
    /// Length of `RESP_CODE_SENT`.
    pub const RECEIPT_LEN: usize = 10;
}

/// What the node reports after taking a direct message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendReceipt {
    /// Whether the packet went out as a flood rather than along a known route.
    pub flooded: bool,
    /// The acknowledgement to expect back, or `None` when none is expected.
    ///
    /// Matches the value in `PUSH_CODE_SEND_CONFIRMED`.
    pub expected_ack: Option<u32>,
    /// How long the node thinks the round trip will take, in milliseconds.
    pub estimated_timeout_ms: u32,
}

/// Why a message could not be turned into a frame.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SendError {
    /// The firmware would not recognise the frame as a send command.
    #[error("cannot send an empty text")]
    EmptyText,

    /// The text does not fit into one frame.
    #[error("text of {len} bytes exceeds the {allowed} that fit into a frame")]
    TooLong {
        /// How many bytes the text takes.
        len: usize,
        /// How many would fit.
        allowed: usize,
    },
}

/// Why a receipt could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReceiptError {
    /// The payload is not a send receipt.
    #[error("expected a send receipt, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload is shorter than a receipt.
    #[error("receipt is {len} bytes, need {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

/// Builds `CMD_SEND_TXT_MSG` for one recipient.
///
/// `attempt` counts retries of the same message; the firmware passes it on so
/// a repeater can tell a resend from a new message.
pub fn encode_direct(
    recipient_prefix: [u8; 6],
    text_type: TextType,
    attempt: u8,
    timestamp: u32,
    text: &str,
) -> Result<Vec<u8>, SendError> {
    check_text(text, layout::DIRECT_HEADER)?;

    let mut frame = Vec::with_capacity(layout::DIRECT_HEADER + text.len());
    frame.push(u8::from(Command::SendTxtMsg));
    frame.push(text_type.as_byte());
    frame.push(attempt);
    frame.extend_from_slice(&timestamp.to_le_bytes());
    frame.extend_from_slice(&recipient_prefix);
    frame.extend_from_slice(text.as_bytes());

    Ok(frame)
}

/// Builds `CMD_SEND_CHANNEL_TXT_MSG` for one channel.
pub fn encode_channel(channel_index: u8, timestamp: u32, text: &str) -> Result<Vec<u8>, SendError> {
    check_text(text, layout::CHANNEL_HEADER)?;

    let mut frame = Vec::with_capacity(layout::CHANNEL_HEADER + text.len());
    frame.push(u8::from(Command::SendChannelTxtMsg));
    // The firmware answers anything else with an unsupported-command error.
    frame.push(TextType::Plain.as_byte());
    frame.push(channel_index);
    frame.extend_from_slice(&timestamp.to_le_bytes());
    frame.extend_from_slice(text.as_bytes());

    Ok(frame)
}

/// Rejects texts the node could not take, rather than sending a bad frame.
fn check_text(text: &str, header_len: usize) -> Result<(), SendError> {
    if text.is_empty() {
        return Err(SendError::EmptyText);
    }

    // Bytes, not characters: one umlaut takes two of them, and the node
    // counts what is on the wire.
    let allowed = MAX_FRAME_SIZE - header_len;
    if text.len() > allowed {
        return Err(SendError::TooLong {
            len: text.len(),
            allowed,
        });
    }

    Ok(())
}

impl SendReceipt {
    /// Reads `RESP_CODE_SENT`, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, ReceiptError> {
        match payload.first().map(|&byte| Response::from(byte)) {
            Some(Response::Sent) => {}
            Some(_) => return Err(ReceiptError::WrongOpcode { opcode: payload[0] }),
            None => {
                return Err(ReceiptError::TooShort {
                    len: 0,
                    needed: layout::RECEIPT_LEN,
                });
            }
        }

        if payload.len() < layout::RECEIPT_LEN {
            return Err(ReceiptError::TooShort {
                len: payload.len(),
                needed: layout::RECEIPT_LEN,
            });
        }

        let expected_ack = u32::from_le_bytes([payload[2], payload[3], payload[4], payload[5]]);

        Ok(Self {
            flooded: payload[1] == 1,
            // The firmware writes zero when it expects nothing back.
            expected_ack: (expected_ack != 0).then_some(expected_ack),
            estimated_timeout_ms: u32::from_le_bytes([
                payload[6], payload[7], payload[8], payload[9],
            ]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_a_direct_message_as_the_firmware_reads_it() {
        let frame = encode_direct([0xAA; 6], TextType::Plain, 0, 1_700_000_000, "Moin").unwrap();

        assert_eq!(frame[0], u8::from(Command::SendTxtMsg));
        assert_eq!(frame[1], 0, "TXT_TYPE_PLAIN");
        assert_eq!(frame[2], 0, "attempt");
        assert_eq!(&frame[3..7], &1_700_000_000_u32.to_le_bytes());
        assert_eq!(&frame[7..13], &[0xAA; 6]);
        assert_eq!(&frame[13..], b"Moin");
    }

    #[test]
    fn carries_the_attempt_and_text_type_through() {
        let frame =
            encode_direct([0x01; 6], TextType::CliData, 3, 1_700_000_000, "reboot").unwrap();

        assert_eq!(frame[1], 1, "TXT_TYPE_CLI_DATA");
        assert_eq!(frame[2], 3);
    }

    #[test]
    fn lays_out_a_channel_message() {
        let frame = encode_channel(2, 1_700_000_000, "Hallo Kanal").unwrap();

        assert_eq!(frame[0], u8::from(Command::SendChannelTxtMsg));
        assert_eq!(frame[1], 0, "the firmware only accepts plain here");
        assert_eq!(frame[2], 2, "channel index");
        assert_eq!(&frame[3..7], &1_700_000_000_u32.to_le_bytes());
        assert_eq!(&frame[7..], "Hallo Kanal".as_bytes());
    }

    #[test]
    fn refuses_an_empty_text() {
        // The firmware would not recognise the frame as a send command at all.
        assert_eq!(
            encode_direct([0xAA; 6], TextType::Plain, 0, 0, ""),
            Err(SendError::EmptyText)
        );
        assert_eq!(encode_channel(0, 0, ""), Err(SendError::EmptyText));
    }

    #[test]
    fn refuses_a_text_that_would_not_fit_into_a_frame() {
        let allowed = MAX_FRAME_SIZE - layout::DIRECT_HEADER;
        let text = "x".repeat(allowed + 1);

        assert_eq!(
            encode_direct([0xAA; 6], TextType::Plain, 0, 0, &text),
            Err(SendError::TooLong {
                len: allowed + 1,
                allowed
            })
        );
    }

    #[test]
    fn accepts_a_text_that_fills_the_frame_exactly() {
        let text = "x".repeat(MAX_FRAME_SIZE - layout::DIRECT_HEADER);

        assert!(encode_direct([0xAA; 6], TextType::Plain, 0, 0, &text).is_ok());
    }

    #[test]
    fn measures_the_text_in_bytes_not_characters() {
        // Two bytes per umlaut: counting characters would build a frame the
        // node rejects.
        let text = "ä".repeat(MAX_FRAME_SIZE - layout::DIRECT_HEADER);

        assert!(matches!(
            encode_direct([0xAA; 6], TextType::Plain, 0, 0, &text),
            Err(SendError::TooLong { .. })
        ));
    }

    #[test]
    fn reads_a_receipt() {
        let mut payload = vec![u8::from(Response::Sent), 1];
        payload.extend_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
        payload.extend_from_slice(&4_500_u32.to_le_bytes());

        assert_eq!(
            SendReceipt::parse(&payload),
            Ok(SendReceipt {
                flooded: true,
                expected_ack: Some(0xDEAD_BEEF),
                estimated_timeout_ms: 4_500,
            })
        );
    }

    #[test]
    fn reads_a_receipt_without_an_acknowledgement() {
        // Zero means "none expected", not "the acknowledgement is zero".
        let mut payload = vec![u8::from(Response::Sent), 0];
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());

        let receipt = SendReceipt::parse(&payload).unwrap();

        assert_eq!(receipt.expected_ack, None);
        assert!(!receipt.flooded);
    }

    #[test]
    fn refuses_a_receipt_that_is_something_else() {
        assert_eq!(
            SendReceipt::parse(&[u8::from(Response::Ok); 10]),
            Err(ReceiptError::WrongOpcode { opcode: 0 })
        );
    }

    #[test]
    fn refuses_a_receipt_that_ends_early() {
        assert_eq!(
            SendReceipt::parse(&[u8::from(Response::Sent), 1, 0]),
            Err(ReceiptError::TooShort {
                len: 3,
                needed: layout::RECEIPT_LEN
            })
        );
    }
}
