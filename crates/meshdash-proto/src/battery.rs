//! Battery and storage of the attached node.
//!
//! Answers `CMD_GET_BATT_AND_STORAGE`. This is the node MeshDash is plugged
//! into, not a node out in the mesh — for those, telemetry travels in
//! `PUSH_CODE_TELEMETRY_RESPONSE`, whose contents are CayenneLPP and not read
//! here.
//!
//! # Layout
//!
//! Source: `handleCmdFrame()` in `examples/companion_radio/MyMesh.cpp`,
//! MeshCore commit `d929643`.
//!
//! ```text
//! offset  size  field
//!      0     1  opcode
//!      1     2  battery voltage in millivolts (u16 little-endian)
//!      3     4  storage used, in kibibytes (u32 little-endian)
//!      7     4  storage total, in kibibytes (u32 little-endian)
//! ```
//!
//! # Millivolts, not percent
//!
//! The firmware reports what it measures. Turning that into a percentage needs
//! the chemistry and cell count of the pack, which the node does not send — so
//! this layer passes the voltage on rather than inventing a charge level.

use crate::opcode::{Command, Response};

/// Total length of the frame.
const FRAME_LEN: usize = 11;

/// What the node reports about its own power and storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryAndStorage {
    /// Battery voltage in millivolts.
    pub battery_millivolts: u16,
    /// Storage in use, in kibibytes.
    pub storage_used_kib: u32,
    /// Storage available in total, in kibibytes.
    pub storage_total_kib: u32,
}

/// Why the payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BatteryError {
    /// The payload is not a battery response.
    #[error("expected a battery response, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload is shorter than the frame.
    #[error("battery payload is {len} bytes, need {FRAME_LEN}")]
    TooShort {
        /// What arrived.
        len: usize,
    },
}

impl BatteryAndStorage {
    /// Reads a battery payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, BatteryError> {
        match payload.first() {
            Some(&opcode) if Response::from(opcode) == Response::BattAndStorage => {}
            Some(&opcode) => return Err(BatteryError::WrongOpcode { opcode }),
            None => return Err(BatteryError::TooShort { len: 0 }),
        }

        if payload.len() < FRAME_LEN {
            return Err(BatteryError::TooShort { len: payload.len() });
        }

        Ok(Self {
            battery_millivolts: u16::from_le_bytes([payload[1], payload[2]]),
            storage_used_kib: u32::from_le_bytes([payload[3], payload[4], payload[5], payload[6]]),
            storage_total_kib: u32::from_le_bytes([
                payload[7],
                payload[8],
                payload[9],
                payload[10],
            ]),
        })
    }

    /// How full the storage is, between 0 and 1.
    ///
    /// `None` when the node reports no capacity — dividing by that would be a
    /// crash, and reporting 0 % free would be a lie.
    pub fn storage_fraction(&self) -> Option<f64> {
        (self.storage_total_kib > 0)
            .then(|| f64::from(self.storage_used_kib) / f64::from(self.storage_total_kib))
    }
}

/// Builds the command asking for battery and storage.
pub fn battery_query() -> Vec<u8> {
    vec![u8::from(Command::GetBattAndStorage)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload as the firmware lays it out.
    fn payload() -> Vec<u8> {
        let mut frame = vec![u8::from(Response::BattAndStorage)];
        frame.extend_from_slice(&4_100_u16.to_le_bytes());
        frame.extend_from_slice(&512_u32.to_le_bytes());
        frame.extend_from_slice(&2048_u32.to_le_bytes());
        frame
    }

    #[test]
    fn reads_every_field() {
        let reading = BatteryAndStorage::parse(&payload()).unwrap();

        assert_eq!(reading.battery_millivolts, 4_100);
        assert_eq!(reading.storage_used_kib, 512);
        assert_eq!(reading.storage_total_kib, 2048);
    }

    #[test]
    fn computes_how_full_the_storage_is() {
        let reading = BatteryAndStorage::parse(&payload()).unwrap();

        assert_eq!(reading.storage_fraction(), Some(0.25));
    }

    #[test]
    fn reports_no_fraction_when_capacity_is_zero() {
        // Dividing by zero would crash; claiming 0 % would be untrue.
        let mut frame = payload();
        frame[7..11].copy_from_slice(&0_u32.to_le_bytes());

        let reading = BatteryAndStorage::parse(&frame).unwrap();

        assert_eq!(reading.storage_fraction(), None);
    }

    #[test]
    fn rejects_a_different_response() {
        assert!(matches!(
            BatteryAndStorage::parse(&[u8::from(Response::Ok); FRAME_LEN]),
            Err(BatteryError::WrongOpcode { .. })
        ));
    }

    #[test]
    fn rejects_a_truncated_payload() {
        assert!(matches!(
            BatteryAndStorage::parse(&payload()[..6]),
            Err(BatteryError::TooShort { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_payload() {
        assert!(BatteryAndStorage::parse(&[]).is_err());
    }

    #[test]
    fn builds_the_query() {
        assert_eq!(battery_query(), vec![u8::from(Command::GetBattAndStorage)]);
    }
}
