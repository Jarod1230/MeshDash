//! Incoming direct messages.
//!
//! Delivered one at a time in response to `CMD_SYNC_NEXT_MESSAGE`, after the
//! node announced them with `PUSH_CODE_MSG_WAITING`.
//!
//! # Layout
//!
//! Source: `queueMessage()` in `examples/companion_radio/MyMesh.cpp`, MeshCore
//! commit `d929643`. The V3 form carries SNR; which one arrives depends on the
//! protocol version *we* announce, not on the firmware — see
//! `meshdash_proto::device::PROTOCOL_VERSION`.
//!
//! ```text
//! offset  size  field                          V3 only
//!      0     1  opcode
//!      1     1  SNR, multiplied by four            yes
//!      2     2  reserved                           yes
//!      +     6  sender key prefix  ← only six bytes
//!      +     1  path length, 0xFF when not flooded
//!      +     1  text type
//!      +     4  sender timestamp (u32 little-endian)
//!      +     4  signature prefix, only for signed messages
//!      +     …  text, to the end of the frame
//! ```
//!
//! # Three things that bite
//!
//! **The sender is identified by six bytes, not a full key.** Matching a
//! contact means comparing prefixes, and prefixes can collide. Anything built
//! on this must treat the match as "probably this contact", not as certain.
//!
//! **A path length of `0xFF` means "no flood path"**, not 255 hops. The
//! firmware writes it whenever the packet did not travel as a flood.
//!
//! **The text runs to the end of the frame and is not terminated.** The
//! firmware truncates it to the frame size without regard for character
//! boundaries — its own source says `TODO: UTF-8 ??` — so the last character
//! can arrive cut in half.

use crate::opcode::Response;

/// Text kinds a message can have.
///
/// Source: `src/helpers/TxtDataHelpers.h`, MeshCore commit `d929643`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextType {
    /// An ordinary message.
    Plain,
    /// A command for a repeater's console.
    CliData,
    /// Plain text with a sender signature; carries four extra bytes.
    SignedPlain,
    /// A value this firmware version does not define.
    Unknown(u8),
}

impl From<u8> for TextType {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Plain,
            1 => Self::CliData,
            2 => Self::SignedPlain,
            other => Self::Unknown(other),
        }
    }
}

impl TextType {
    /// The value as it travels on the wire.
    ///
    /// Round-trips an unknown type rather than flattening it to a known one:
    /// what came in as 7 goes out as 7.
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Plain => 0,
            Self::CliData => 1,
            Self::SignedPlain => 2,
            Self::Unknown(value) => value,
        }
    }

    /// How many bytes sit between the timestamp and the text.
    ///
    /// Only signed messages carry a signature prefix. Assuming a fixed size
    /// would either swallow four characters or prepend four bytes of rubbish.
    fn extra_len(self) -> usize {
        match self {
            Self::SignedPlain => 4,
            // Unknown types are assumed to carry no extra, which is what every
            // known type but one does.
            _ => 0,
        }
    }
}

/// A message the node handed over.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// First six bytes of the sender's public key — a prefix, not a key.
    pub sender_prefix: [u8; 6],
    /// Signal-to-noise ratio in dB, if the frame carried it.
    pub snr: Option<f32>,
    /// How many hops the packet flooded over, or `None` if it was not flooded.
    pub path_len: Option<u8>,
    /// What kind of text this is.
    pub text_type: TextType,
    /// When the sender stamped it, in seconds since the epoch.
    pub sent_at: u32,
    /// Signature prefix, present only on signed messages.
    pub signature_prefix: Option<[u8; 4]>,
    /// The message itself.
    pub text: String,
}

/// Why a message payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MessageError {
    /// The payload is not a contact message.
    #[error("expected a contact message, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload ends before its fields do.
    #[error("message payload is {len} bytes, need at least {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

/// Where the sender prefix starts, per variant.
const HEADER_V3: usize = 4;
const HEADER_PLAIN: usize = 1;
/// Marks a packet that did not travel as a flood.
const NO_FLOOD_PATH: u8 = 0xFF;

impl Message {
    /// Reads a message payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, MessageError> {
        let (header_len, carries_snr) = match payload.first().map(|&byte| Response::from(byte)) {
            Some(Response::ContactMsgRecvV3) => (HEADER_V3, true),
            Some(Response::ContactMsgRecv) => (HEADER_PLAIN, false),
            Some(_) => {
                return Err(MessageError::WrongOpcode { opcode: payload[0] });
            }
            None => {
                return Err(MessageError::TooShort {
                    len: 0,
                    needed: HEADER_PLAIN,
                });
            }
        };

        // Prefix, path length, text type and timestamp all have to be there
        // before anything can be read.
        let fixed_end = header_len + 6 + 1 + 1 + 4;
        if payload.len() < fixed_end {
            return Err(MessageError::TooShort {
                len: payload.len(),
                needed: fixed_end,
            });
        }

        let mut sender_prefix = [0u8; 6];
        sender_prefix.copy_from_slice(&payload[header_len..header_len + 6]);

        let raw_path_len = payload[header_len + 6];
        let text_type = TextType::from(payload[header_len + 7]);

        let timestamp_at = header_len + 8;
        let sent_at = u32::from_le_bytes([
            payload[timestamp_at],
            payload[timestamp_at + 1],
            payload[timestamp_at + 2],
            payload[timestamp_at + 3],
        ]);

        let extra_len = text_type.extra_len();
        let text_at = fixed_end + extra_len;
        if payload.len() < text_at {
            return Err(MessageError::TooShort {
                len: payload.len(),
                needed: text_at,
            });
        }

        let signature_prefix = (extra_len == 4).then(|| {
            let mut prefix = [0u8; 4];
            prefix.copy_from_slice(&payload[fixed_end..fixed_end + 4]);
            prefix
        });

        Ok(Self {
            sender_prefix,
            // Stored multiplied by four, and signed: LoRa decodes below the
            // noise floor, so negative values are ordinary.
            snr: carries_snr.then(|| f32::from(payload[1] as i8) / 4.0),
            // 0xFF marks a packet that did not travel as a flood.
            path_len: (raw_path_len != NO_FLOOD_PATH).then_some(raw_path_len),
            text_type,
            sent_at,
            signature_prefix,
            // Runs to the end of the frame and is not terminated. The firmware
            // truncates without regard for character boundaries, so a cut
            // character is replaced rather than making the message unreadable.
            text: String::from_utf8_lossy(&payload[text_at..]).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a V3 message frame as the firmware lays it out.
    fn v3_message(text_type: u8, text: &str) -> Vec<u8> {
        let mut payload = vec![u8::from(Response::ContactMsgRecvV3)];
        payload.push((5.0_f32 * 4.0) as u8); // SNR of 5 dB
        payload.extend_from_slice(&[0, 0]); // reserved
        payload.extend_from_slice(&[0xAA; 6]); // sender prefix
        payload.push(3); // path length
        payload.push(text_type);
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        if text_type == 2 {
            payload.extend_from_slice(&[0xBB; 4]); // signature prefix
        }
        payload.extend_from_slice(text.as_bytes());
        payload
    }

    #[test]
    fn reads_a_plain_message() {
        let message = Message::parse(&v3_message(0, "Hallo Mesh")).unwrap();

        assert_eq!(message.text, "Hallo Mesh");
        assert_eq!(message.text_type, TextType::Plain);
        assert_eq!(message.sender_prefix, [0xAA; 6]);
        assert_eq!(message.sent_at, 1_700_000_000);
        assert_eq!(message.signature_prefix, None);
    }

    #[test]
    fn divides_the_snr_by_four() {
        // The firmware multiplies by four; reporting the raw byte would show
        // 20 dB where 5 dB were measured.
        let message = Message::parse(&v3_message(0, "x")).unwrap();

        assert_eq!(message.snr, Some(5.0));
    }

    #[test]
    fn reads_a_negative_snr() {
        // LoRa decodes below the noise floor, so negative values are normal.
        let mut payload = v3_message(0, "x");
        payload[1] = (-6.0_f32 * 4.0) as i8 as u8;

        let message = Message::parse(&payload).unwrap();

        assert_eq!(message.snr, Some(-6.0));
    }

    #[test]
    fn keeps_the_signature_prefix_out_of_the_text() {
        // Only signed messages carry it; counting it as text would prepend
        // four bytes of rubbish.
        let message = Message::parse(&v3_message(2, "Signiert")).unwrap();

        assert_eq!(message.text, "Signiert");
        assert_eq!(message.signature_prefix, Some([0xBB; 4]));
        assert_eq!(message.text_type, TextType::SignedPlain);
    }

    #[test]
    fn treats_the_no_flood_marker_as_absent() {
        // 0xFF means "did not travel as a flood", not 255 hops.
        let mut payload = v3_message(0, "x");
        payload[10] = 0xFF;

        let message = Message::parse(&payload).unwrap();

        assert_eq!(message.path_len, None);
    }

    #[test]
    fn reads_the_older_variant_without_snr() {
        let mut payload = vec![u8::from(Response::ContactMsgRecv)];
        payload.extend_from_slice(&[0xAA; 6]);
        payload.push(0);
        payload.push(0);
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload.extend_from_slice(b"Alt");

        let message = Message::parse(&payload).unwrap();

        assert_eq!(message.text, "Alt");
        assert_eq!(message.snr, None, "the older form carries none");
    }

    #[test]
    fn survives_text_cut_mid_character() {
        // The firmware truncates to the frame size without regard for
        // character boundaries, so half a character can arrive.
        let mut payload = v3_message(0, "");
        payload.extend_from_slice(&"ä".as_bytes()[..1]);

        let message = Message::parse(&payload).unwrap();

        assert!(!message.text.is_empty(), "replaced, not rejected");
    }

    #[test]
    fn accepts_an_empty_text() {
        let message = Message::parse(&v3_message(0, "")).unwrap();

        assert_eq!(message.text, "");
    }

    #[test]
    fn rejects_a_different_response() {
        assert!(matches!(
            Message::parse(&[u8::from(Response::Ok); 40]),
            Err(MessageError::WrongOpcode { .. })
        ));
    }

    #[test]
    fn rejects_a_truncated_payload() {
        let payload = v3_message(0, "x")[..8].to_vec();

        assert!(matches!(
            Message::parse(&payload),
            Err(MessageError::TooShort { .. })
        ));
    }

    #[test]
    fn rejects_a_signed_message_without_its_prefix() {
        // Claiming to be signed but ending early must not read past the frame.
        let mut payload = vec![u8::from(Response::ContactMsgRecvV3)];
        payload.extend_from_slice(&[0, 0, 0]);
        payload.extend_from_slice(&[0xAA; 6]);
        payload.push(0);
        payload.push(2); // signed
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        // no signature prefix follows

        assert!(matches!(
            Message::parse(&payload),
            Err(MessageError::TooShort { .. })
        ));
    }

    #[test]
    fn a_text_type_survives_the_round_trip() {
        for byte in [0u8, 1, 2, 7, 255] {
            assert_eq!(TextType::from(byte).as_byte(), byte);
        }
    }
}
