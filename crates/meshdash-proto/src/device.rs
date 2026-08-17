//! The node's own description of itself.
//!
//! Answers `CMD_DEVICE_QUERY` with `RESP_CODE_DEVICE_INFO`. This is the first
//! thing an app asks after connecting, and the only payload whose layout is
//! verified so far — see `docs/research/meshcore-companion-protocol.md`.
//!
//! # Layout
//!
//! Source: `handleCmdFrame()` in `examples/companion_radio/MyMesh.cpp`,
//! MeshCore commit `d929643`, firmware `v1.17.1`.
//!
//! ```text
//! offset  size  field
//!      0     1  opcode (RESP_CODE_DEVICE_INFO)
//!      1     1  firmware version code
//!      2     1  half the contact capacity  ← see below
//!      3     1  number of group channels
//!      4     4  BLE pairing pin (u32 little-endian)
//!      8    12  build date, NUL-padded
//!     20    40  manufacturer name, NUL-padded
//!     60    20  firmware version string, NUL-padded
//!     80     1  repeater enabled (protocol version 9 and above)
//!     81     1  path hash mode (protocol version 10 and above)
//! ```
//!
//! # The contact capacity is halved on the wire
//!
//! The firmware writes `MAX_CONTACTS / 2` into a single byte, because the real
//! capacity does not fit in one. Reading that byte as "the capacity" gives half
//! the truth — an operator would be told 50 contacts fit where 100 do. The
//! doubling happens here, once.
//!
//! # Fields the node always sends
//!
//! The `v3+`, `v9+` and `v10+` markers in the firmware say from which protocol
//! version an **app** understands a field, not whether the node writes it — it
//! always does. Older firmware may still send a shorter frame, so everything
//! past the strings is optional here.

use crate::opcode::Response;

/// Offsets and widths from the layout above.
mod layout {
    /// Where the fixed-width part ends.
    pub const MINIMUM_LEN: usize = 60;
    pub const FIRMWARE_VER_CODE: usize = 1;
    pub const HALF_CONTACT_CAPACITY: usize = 2;
    pub const GROUP_CHANNELS: usize = 3;
    pub const BLE_PIN: usize = 4;
    pub const BUILD_DATE: usize = 8;
    pub const BUILD_DATE_LEN: usize = 12;
    pub const MANUFACTURER: usize = 20;
    pub const MANUFACTURER_LEN: usize = 40;
    pub const FIRMWARE_VERSION: usize = 60;
    pub const FIRMWARE_VERSION_LEN: usize = 20;
    pub const REPEATER_ENABLED: usize = 80;
    pub const PATH_HASH_MODE: usize = 81;
}

/// What a node reports about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Firmware's own version number, as a single byte.
    pub firmware_version_code: u8,
    /// How many contacts the node can store.
    ///
    /// Already doubled — the wire carries half of it, see the module docs.
    pub contact_capacity: u16,
    /// How many group channels the node supports.
    pub group_channels: u8,
    /// Pairing pin used for BLE.
    pub ble_pin: u32,
    /// When the firmware was built, as the firmware spells it.
    pub build_date: String,
    /// Hardware manufacturer, as reported by the board.
    pub manufacturer: String,
    /// Firmware version string, for instance `v1.17.1`.
    pub firmware_version: String,
    /// Whether the node also repeats. `None` on firmware that omits the field.
    pub repeater_enabled: Option<bool>,
    /// How path hashes are handled. `None` on firmware that omits the field.
    pub path_hash_mode: Option<u8>,
}

/// Why a device info payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceInfoError {
    /// The payload is not a device info response.
    #[error("expected a device info response, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte actually was.
        opcode: u8,
    },

    /// The payload ends before the fixed-width fields do.
    #[error("device info payload is {len} bytes, need at least {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl DeviceInfo {
    /// Reads a device info payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, DeviceInfoError> {
        match payload.first() {
            Some(&opcode) if Response::from(opcode) == Response::DeviceInfo => {}
            Some(&opcode) => return Err(DeviceInfoError::WrongOpcode { opcode }),
            None => {
                return Err(DeviceInfoError::TooShort {
                    len: 0,
                    needed: layout::MINIMUM_LEN,
                });
            }
        }

        let needed = layout::FIRMWARE_VERSION + layout::FIRMWARE_VERSION_LEN;
        if payload.len() < needed {
            return Err(DeviceInfoError::TooShort {
                len: payload.len(),
                needed,
            });
        }

        let ble_pin = u32::from_le_bytes([
            payload[layout::BLE_PIN],
            payload[layout::BLE_PIN + 1],
            payload[layout::BLE_PIN + 2],
            payload[layout::BLE_PIN + 3],
        ]);

        Ok(Self {
            firmware_version_code: payload[layout::FIRMWARE_VER_CODE],
            // Doubled here, once, so no caller has to remember.
            contact_capacity: u16::from(payload[layout::HALF_CONTACT_CAPACITY]) * 2,
            group_channels: payload[layout::GROUP_CHANNELS],
            ble_pin,
            build_date: read_text(payload, layout::BUILD_DATE, layout::BUILD_DATE_LEN),
            manufacturer: read_text(payload, layout::MANUFACTURER, layout::MANUFACTURER_LEN),
            firmware_version: read_text(
                payload,
                layout::FIRMWARE_VERSION,
                layout::FIRMWARE_VERSION_LEN,
            ),
            repeater_enabled: payload.get(layout::REPEATER_ENABLED).map(|&byte| byte != 0),
            path_hash_mode: payload.get(layout::PATH_HASH_MODE).copied(),
        })
    }
}

/// Reads a fixed-width, NUL-padded text field.
///
/// Invalid UTF-8 is replaced rather than rejected: a garbled manufacturer name
/// is no reason to discard a node's whole description.
fn read_text(payload: &[u8], offset: usize, width: usize) -> String {
    let field = &payload[offset..offset + width];
    let end = field.iter().position(|&byte| byte == 0).unwrap_or(width);

    String::from_utf8_lossy(&field[..end]).trim().to_owned()
}

/// Builds the command asking a node to describe itself.
///
/// The version tells the node which protocol variants this app understands;
/// see [`PROTOCOL_VERSION`].
pub fn device_query(protocol_version: u8) -> Vec<u8> {
    vec![
        u8::from(crate::opcode::Command::DeviceQuery),
        protocol_version,
    ]
}

/// The protocol version MeshDash announces.
///
/// Chosen as 3 because that is what makes the node send the message variants
/// carrying SNR, which the telemetry module needs. Higher versions unlock
/// statistics (8) and further fields, but they also change response formats we
/// have not verified — raising this is a decision, not a detail.
///
/// Source for the meaning of the versions: the `app_target_ver` branches in
/// `MyMesh.cpp`, MeshCore commit `d929643`.
pub const PROTOCOL_VERSION: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a payload exactly as the firmware lays it out.
    ///
    /// Not captured from hardware — assembled from the source-verified layout,
    /// so it proves the parser matches our reading of the firmware, not that
    /// the reading itself is right. Real captures belong under `fixtures/`.
    fn device_info_payload() -> Vec<u8> {
        let mut payload = vec![0u8; 82];
        payload[0] = u8::from(Response::DeviceInfo);
        payload[layout::FIRMWARE_VER_CODE] = 13;
        // The firmware halves the capacity to fit one byte: 100 contacts.
        payload[layout::HALF_CONTACT_CAPACITY] = 50;
        payload[layout::GROUP_CHANNELS] = 8;
        payload[layout::BLE_PIN..layout::BLE_PIN + 4].copy_from_slice(&123_456_u32.to_le_bytes());
        payload[layout::BUILD_DATE..layout::BUILD_DATE + 11].copy_from_slice(b"14 Aug 2026");
        payload[layout::MANUFACTURER..layout::MANUFACTURER + 8].copy_from_slice(b"Heltec  ");
        payload[layout::FIRMWARE_VERSION..layout::FIRMWARE_VERSION + 7].copy_from_slice(b"v1.17.1");
        payload[layout::REPEATER_ENABLED] = 1;
        payload[layout::PATH_HASH_MODE] = 2;
        payload
    }

    #[test]
    fn reads_every_field() {
        let info = DeviceInfo::parse(&device_info_payload()).unwrap();

        assert_eq!(info.firmware_version_code, 13);
        assert_eq!(info.group_channels, 8);
        assert_eq!(info.ble_pin, 123_456);
        assert_eq!(info.build_date, "14 Aug 2026");
        assert_eq!(info.firmware_version, "v1.17.1");
        assert_eq!(info.repeater_enabled, Some(true));
        assert_eq!(info.path_hash_mode, Some(2));
    }

    #[test]
    fn doubles_the_contact_capacity() {
        // The wire carries half. Reporting 50 where 100 fit would be a quiet
        // lie an operator has no way to notice.
        let info = DeviceInfo::parse(&device_info_payload()).unwrap();

        assert_eq!(info.contact_capacity, 100);
    }

    #[test]
    fn trims_the_padding_from_strings() {
        let info = DeviceInfo::parse(&device_info_payload()).unwrap();

        // Fixed-width fields are NUL-padded, and this one also had spaces.
        assert_eq!(info.manufacturer, "Heltec");
    }

    #[test]
    fn reads_a_frame_without_the_newer_fields() {
        // Older firmware stops after the strings.
        let short = device_info_payload()[..layout::MINIMUM_LEN + 20].to_vec();

        let info = DeviceInfo::parse(&short).unwrap();

        assert_eq!(info.firmware_version, "v1.17.1");
        assert_eq!(info.repeater_enabled, None, "absent, not guessed");
        assert_eq!(info.path_hash_mode, None);
    }

    #[test]
    fn reads_a_frame_that_has_only_the_repeater_flag() {
        let mut short = device_info_payload();
        short.truncate(layout::PATH_HASH_MODE);

        let info = DeviceInfo::parse(&short).unwrap();

        assert_eq!(info.repeater_enabled, Some(true));
        assert_eq!(info.path_hash_mode, None);
    }

    #[test]
    fn rejects_a_different_response() {
        let payload = vec![u8::from(Response::Ok); 82];

        let error = DeviceInfo::parse(&payload).unwrap_err();

        assert!(matches!(error, DeviceInfoError::WrongOpcode { .. }));
    }

    #[test]
    fn rejects_a_truncated_payload() {
        // Must not panic on a short frame — it comes from the wire.
        let error = DeviceInfo::parse(&[u8::from(Response::DeviceInfo), 13]).unwrap_err();

        assert!(matches!(error, DeviceInfoError::TooShort { .. }));
    }

    #[test]
    fn rejects_an_empty_payload() {
        assert!(DeviceInfo::parse(&[]).is_err());
    }

    #[test]
    fn builds_the_query_with_the_announced_version() {
        let frame = device_query(PROTOCOL_VERSION);

        assert_eq!(
            frame,
            vec![u8::from(crate::opcode::Command::DeviceQuery), 3]
        );
    }
}
