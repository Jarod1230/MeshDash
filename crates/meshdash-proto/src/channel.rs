//! Channels: messages everyone on a shared key can read.
//!
//! # Layout
//!
//! Source: `onChannelMessageRecv()` and the `CMD_GET_CHANNEL` branch of
//! `handleCmdFrame()` in `examples/companion_radio/MyMesh.cpp`, MeshCore commit
//! `d929643`.
//!
//! A channel message arrives the same way a direct one does: the node queues it
//! and rings `PUSH_CODE_MSG_WAITING`, and it is handed over in response to
//! `CMD_SYNC_NEXT_MESSAGE`. Only the layout differs.
//!
//! ```text
//! offset  size  field                          V3 only
//!      0     1  opcode
//!      1     1  SNR, multiplied by four            yes
//!      2     2  reserved                           yes
//!      +     1  channel index
//!      +     1  path length, 0xFF when not flooded
//!      +     1  text type
//!      +     4  sender timestamp (u32 little-endian)
//!      +     …  text, to the end of the frame
//! ```
//!
//! # There is no sender
//!
//! A direct message names who sent it. A channel message does not: the firmware
//! puts the sender's node name into the text itself before broadcasting. What
//! arrives is the channel and the text, and nothing that identifies the author
//! in a way code could check.
//!
//! # The channel key is a secret and is not read here
//!
//! `RESP_CODE_CHANNEL_INFO` carries the shared 128-bit key right after the
//! name. Whoever holds it can read and write the channel, so it is deliberately
//! not parsed into [`ChannelInfo`] — a field that does not exist cannot be
//! logged, serialised into an API response or written to the database by
//! accident.

use crate::{message::TextType, opcode::Response};

/// Offsets that do not depend on the variant.
mod layout {
    /// Where the fields start in the V3 form.
    pub const HEADER_V3: usize = 4;
    /// Where they start in the older form.
    pub const HEADER_PLAIN: usize = 1;
    /// Index, path length, text type, timestamp.
    pub const FIXED: usize = 1 + 1 + 1 + 4;

    /// Channel info: index, name, key.
    pub const INFO_INDEX: usize = 1;
    pub const INFO_NAME: usize = 2;
    pub const INFO_NAME_SIZE: usize = 32;
    /// The key sits behind the name and is not read.
    pub const INFO_LEN: usize = 50;
}

/// A message that arrived on a channel.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelMessage {
    /// Which of the node's channels it came in on.
    pub channel_index: u8,
    /// Signal-to-noise ratio in dB, if the frame carried it.
    pub snr: Option<f32>,
    /// How many hops the packet flooded over, or `None` if it was not flooded.
    pub path_len: Option<u8>,
    /// What kind of text this is.
    pub text_type: TextType,
    /// When the sender stamped it, in seconds since the epoch.
    pub sent_at: u32,
    /// The message itself, sender name included by the sending firmware.
    pub text: String,
}

/// One of the node's channels, without its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInfo {
    /// Position in the node's channel table; how channels are addressed.
    pub index: u8,
    /// Display name.
    pub name: String,
}

/// Why a channel payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    /// The payload is not what was expected.
    #[error("expected a channel frame, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload ends before its fields do.
    #[error("channel payload is {len} bytes, need at least {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl ChannelMessage {
    /// Reads a channel message payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, ChannelError> {
        let (header_len, carries_snr) = match payload.first().map(|&byte| Response::from(byte)) {
            Some(Response::ChannelMsgRecvV3) => (layout::HEADER_V3, true),
            Some(Response::ChannelMsgRecv) => (layout::HEADER_PLAIN, false),
            Some(_) => return Err(ChannelError::WrongOpcode { opcode: payload[0] }),
            None => {
                return Err(ChannelError::TooShort {
                    len: 0,
                    needed: layout::HEADER_PLAIN,
                });
            }
        };

        let text_at = header_len + layout::FIXED;
        if payload.len() < text_at {
            return Err(ChannelError::TooShort {
                len: payload.len(),
                needed: text_at,
            });
        }

        let raw_path_len = payload[header_len + 1];
        let timestamp_at = header_len + 3;
        let sent_at = u32::from_le_bytes([
            payload[timestamp_at],
            payload[timestamp_at + 1],
            payload[timestamp_at + 2],
            payload[timestamp_at + 3],
        ]);

        Ok(Self {
            channel_index: payload[header_len],
            // Stored multiplied by four, and signed: LoRa decodes below the
            // noise floor, so negative values are ordinary.
            snr: carries_snr.then(|| f32::from(payload[1] as i8) / 4.0),
            // The byte encodes stations and their width; 0xFF is the
            // firmware's marker for "did not travel as a flood". See
            // crate::path — read as a plain count it is quietly wrong.
            path_len: crate::path::decode(raw_path_len).map(|shape| shape.stations),
            text_type: TextType::from(payload[header_len + 2]),
            sent_at,
            // Runs to the end of the frame, unterminated and possibly cut
            // mid-character; a lossy read keeps the rest readable.
            text: String::from_utf8_lossy(&payload[text_at..]).into_owned(),
        })
    }
}

impl ChannelInfo {
    /// Reads a channel description, opcode byte included.
    ///
    /// The shared key travels in this frame and is skipped on purpose.
    pub fn parse(payload: &[u8]) -> Result<Self, ChannelError> {
        match payload.first().map(|&byte| Response::from(byte)) {
            Some(Response::ChannelInfo) => {}
            Some(_) => return Err(ChannelError::WrongOpcode { opcode: payload[0] }),
            None => {
                return Err(ChannelError::TooShort {
                    len: 0,
                    needed: layout::INFO_LEN,
                });
            }
        }

        if payload.len() < layout::INFO_LEN {
            return Err(ChannelError::TooShort {
                len: payload.len(),
                needed: layout::INFO_LEN,
            });
        }

        let raw = &payload[layout::INFO_NAME..layout::INFO_NAME + layout::INFO_NAME_SIZE];
        let name = raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len());

        Ok(Self {
            index: payload[layout::INFO_INDEX],
            name: String::from_utf8_lossy(&raw[..name]).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a V3 channel message as the firmware lays it out.
    fn v3_message(text: &str) -> Vec<u8> {
        let mut payload = vec![u8::from(Response::ChannelMsgRecvV3)];
        payload.push((-2.5_f32 * 4.0) as i8 as u8);
        payload.extend_from_slice(&[0, 0]);
        payload.push(3); // channel index
        payload.push(2); // path length
        payload.push(0); // TXT_TYPE_PLAIN
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload.extend_from_slice(text.as_bytes());
        payload
    }

    /// The same in the form older protocol versions get.
    fn plain_message(text: &str) -> Vec<u8> {
        let mut payload = vec![u8::from(Response::ChannelMsgRecv)];
        payload.push(3);
        payload.push(0xFF); // did not travel as a flood
        payload.push(0);
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload.extend_from_slice(text.as_bytes());
        payload
    }

    fn channel_info() -> Vec<u8> {
        let mut payload = vec![0u8; layout::INFO_LEN];
        payload[0] = u8::from(Response::ChannelInfo);
        payload[layout::INFO_INDEX] = 3;
        payload[layout::INFO_NAME..layout::INFO_NAME + 9].copy_from_slice(b"Notfunk\0\0");
        // The key. Present on the wire, deliberately unread.
        payload[34..50].copy_from_slice(&[0x99; 16]);
        payload
    }

    #[test]
    fn reads_a_v3_channel_message() {
        let message = ChannelMessage::parse(&v3_message("Moin zusammen")).unwrap();

        assert_eq!(message.channel_index, 3);
        assert_eq!(message.text, "Moin zusammen");
        assert_eq!(message.sent_at, 1_700_000_000);
        assert_eq!(message.text_type, TextType::Plain);
    }

    #[test]
    fn reads_the_signal_quality_only_where_it_travels() {
        // LoRa decodes below the noise floor, so a negative SNR is ordinary.
        assert_eq!(
            ChannelMessage::parse(&v3_message("A")).unwrap().snr,
            Some(-2.5)
        );
        assert_eq!(
            ChannelMessage::parse(&plain_message("A")).unwrap().snr,
            None
        );
    }

    #[test]
    fn treats_the_no_flood_marker_as_absent() {
        // 0xFF is "did not travel as a flood", not 255 hops.
        assert_eq!(
            ChannelMessage::parse(&v3_message("A")).unwrap().path_len,
            Some(2)
        );
        assert_eq!(
            ChannelMessage::parse(&plain_message("A")).unwrap().path_len,
            None
        );
    }

    #[test]
    fn reads_the_older_form_from_the_same_offsets_shifted() {
        let message = ChannelMessage::parse(&plain_message("Kurz")).unwrap();

        assert_eq!(message.channel_index, 3);
        assert_eq!(message.text, "Kurz");
        assert_eq!(message.sent_at, 1_700_000_000);
    }

    #[test]
    fn survives_a_message_cut_mid_character() {
        // The firmware truncates to the frame size without minding character
        // boundaries — its own source says so.
        let mut payload = v3_message("Grüße");
        payload.truncate(payload.len() - 1);

        assert!(ChannelMessage::parse(&payload).is_ok());
    }

    #[test]
    fn accepts_an_empty_channel_message() {
        assert_eq!(ChannelMessage::parse(&v3_message("")).unwrap().text, "");
    }

    #[test]
    fn refuses_a_channel_message_that_ends_early() {
        let payload = &v3_message("A")[..8];

        assert_eq!(
            ChannelMessage::parse(payload),
            Err(ChannelError::TooShort {
                len: 8,
                needed: layout::HEADER_V3 + layout::FIXED
            })
        );
    }

    #[test]
    fn refuses_a_frame_that_is_not_a_channel_message() {
        assert_eq!(
            ChannelMessage::parse(&[u8::from(Response::Contact), 0]),
            Err(ChannelError::WrongOpcode { opcode: 3 })
        );
    }

    #[test]
    fn reads_a_channel_description() {
        let info = ChannelInfo::parse(&channel_info()).unwrap();

        assert_eq!(info.index, 3);
        assert_eq!(info.name, "Notfunk");
    }

    #[test]
    fn refuses_a_channel_description_that_ends_early() {
        let payload = &channel_info()[..20];

        assert_eq!(
            ChannelInfo::parse(payload),
            Err(ChannelError::TooShort {
                len: 20,
                needed: layout::INFO_LEN
            })
        );
    }

    #[test]
    fn refuses_a_description_that_is_something_else() {
        assert_eq!(
            ChannelInfo::parse(&[u8::from(Response::Ok)]),
            Err(ChannelError::WrongOpcode { opcode: 0 })
        );
    }
}
