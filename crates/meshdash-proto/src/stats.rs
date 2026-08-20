//! What the node counts about itself.
//!
//! # Layout
//!
//! Source: the `CMD_GET_STATS` branch of `handleCmdFrame()` in
//! `examples/companion_radio/MyMesh.cpp`, MeshCore commit `d929643`. Confirmed
//! against a Xiao S3 WIO on firmware v1.17.0 for the core kind.
//!
//! Every answer starts with the opcode and the kind, then the counters of that
//! kind:
//!
//! ```text
//! core (0), 11 bytes
//!   2   2  battery in millivolts (u16 little-endian)
//!   4   4  uptime in seconds (u32)
//!   8   2  error flags, passed through unread (u16)
//!  10   1  packets waiting to go out
//!
//! radio (1), 14 bytes
//!   2   2  noise floor (i16)
//!   4   1  RSSI of the last packet (i8, dBm)
//!   5   1  SNR of the last packet (i8, multiplied by four)
//!   6   4  seconds spent transmitting (u32)
//!  10   4  seconds spent receiving (u32)
//!
//! packets (2), 30 bytes
//!   2   4  packets received      6   4  packets sent
//!  10   4  sent as flood        14   4  sent direct
//!  18   4  received as flood    22   4  received direct
//!  26   4  receive errors
//! ```
//!
//! # The error flags are not decoded
//!
//! `_err_flags` is a bit field whose meaning is nowhere in the companion
//! source. It is passed through as a number rather than given invented names —
//! a flag called something wrong is worse than a flag called `0x0004`.

use crate::opcode::{Response, StatsType};

/// A statistics answer, by kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stats {
    /// Battery, uptime and the outbound queue.
    Core {
        /// Battery voltage in millivolts. Zero on a board running off USB.
        battery_millivolts: u16,
        /// How long the node has been up, in seconds.
        uptime_seconds: u32,
        /// Firmware error flags, passed through unread.
        error_flags: u16,
        /// How many packets are waiting to be sent.
        queued_packets: u8,
    },

    /// What the radio hears and how long it talks.
    Radio {
        /// Noise floor, as the radio reports it.
        noise_floor: i16,
        /// RSSI of the last received packet, in dBm.
        last_rssi: i8,
        /// SNR of the last received packet, in dB.
        last_snr: f32,
        /// Seconds spent transmitting since boot.
        transmit_seconds: u32,
        /// Seconds spent receiving since boot.
        receive_seconds: u32,
    },

    /// Packet counters since boot.
    Packets {
        /// Packets received.
        received: u32,
        /// Packets sent.
        sent: u32,
        /// Of those sent, how many went out as a flood.
        sent_flood: u32,
        /// Of those sent, how many took a known route.
        sent_direct: u32,
        /// Of those received, how many arrived as a flood.
        received_flood: u32,
        /// Of those received, how many arrived directly.
        received_direct: u32,
        /// Packets the radio could not decode.
        receive_errors: u32,
    },
}

/// Why a statistics answer could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatsError {
    /// The payload is not a statistics answer.
    #[error("expected statistics, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// A kind this build does not know. Its layout is unknown, so nothing of
    /// it can be read.
    #[error("unknown statistics kind {kind}")]
    UnknownKind {
        /// The kind byte.
        kind: u8,
    },

    /// The payload is shorter than the kind requires.
    #[error("statistics payload is {len} bytes, need {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl Stats {
    /// Reads `RESP_CODE_STATS`, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, StatsError> {
        match payload.first().map(|&byte| Response::from(byte)) {
            Some(Response::Stats) => {}
            Some(_) => {
                return Err(StatsError::WrongOpcode { opcode: payload[0] });
            }
            None => return Err(StatsError::TooShort { len: 0, needed: 2 }),
        }

        let Some(&kind) = payload.get(1) else {
            return Err(StatsError::TooShort {
                len: payload.len(),
                needed: 2,
            });
        };

        match StatsType::from(kind) {
            StatsType::Core => Self::core(payload),
            StatsType::Radio => Self::radio(payload),
            StatsType::Packets => Self::packets(payload),
            StatsType::Unknown(kind) => Err(StatsError::UnknownKind { kind }),
        }
    }

    fn core(payload: &[u8]) -> Result<Self, StatsError> {
        need(payload, 11)?;

        Ok(Self::Core {
            battery_millivolts: u16::from_le_bytes([payload[2], payload[3]]),
            uptime_seconds: read_u32(payload, 4),
            error_flags: u16::from_le_bytes([payload[8], payload[9]]),
            queued_packets: payload[10],
        })
    }

    fn radio(payload: &[u8]) -> Result<Self, StatsError> {
        need(payload, 14)?;

        Ok(Self::Radio {
            noise_floor: i16::from_le_bytes([payload[2], payload[3]]),
            last_rssi: payload[4] as i8,
            // Stored multiplied by four, as everywhere SNR appears.
            last_snr: f32::from(payload[5] as i8) / 4.0,
            transmit_seconds: read_u32(payload, 6),
            receive_seconds: read_u32(payload, 10),
        })
    }

    fn packets(payload: &[u8]) -> Result<Self, StatsError> {
        need(payload, 30)?;

        Ok(Self::Packets {
            received: read_u32(payload, 2),
            sent: read_u32(payload, 6),
            sent_flood: read_u32(payload, 10),
            sent_direct: read_u32(payload, 14),
            received_flood: read_u32(payload, 18),
            received_direct: read_u32(payload, 22),
            receive_errors: read_u32(payload, 26),
        })
    }
}

fn need(payload: &[u8], length: usize) -> Result<(), StatsError> {
    if payload.len() < length {
        return Err(StatsError::TooShort {
            len: payload.len(),
            needed: length,
        });
    }

    Ok(())
}

fn read_u32(payload: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([
        payload[at],
        payload[at + 1],
        payload[at + 2],
        payload[at + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes a real node answered with — a Xiao S3 WIO on firmware v1.17.0,
    /// asked over USB on 2026-08-20. Battery reads zero because the board was
    /// running off the cable with no cell attached.
    const CORE_FROM_HARDWARE: [u8; 11] = [
        0x18, 0x00, 0x00, 0x00, 0x6d, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn reads_what_the_real_node_answered() {
        let Ok(Stats::Core {
            battery_millivolts,
            uptime_seconds,
            error_flags,
            queued_packets,
        }) = Stats::parse(&CORE_FROM_HARDWARE)
        else {
            panic!("expected core statistics");
        };

        assert_eq!(battery_millivolts, 0, "running off USB");
        assert_eq!(uptime_seconds, 2669);
        assert_eq!(error_flags, 0);
        assert_eq!(queued_packets, 0);
    }

    #[test]
    fn reads_the_radio_counters() {
        let mut payload = vec![u8::from(Response::Stats), 1];
        payload.extend_from_slice(&(-98_i16).to_le_bytes());
        payload.push(-77_i8 as u8);
        payload.push((5.5_f32 * 4.0) as i8 as u8);
        payload.extend_from_slice(&120_u32.to_le_bytes());
        payload.extend_from_slice(&3_600_u32.to_le_bytes());

        let Ok(Stats::Radio {
            noise_floor,
            last_rssi,
            last_snr,
            transmit_seconds,
            receive_seconds,
        }) = Stats::parse(&payload)
        else {
            panic!("expected radio statistics");
        };

        // All three of these are ordinarily negative; a parser that forgets
        // the sign reports a radio that hears better than physics allows.
        assert_eq!(noise_floor, -98);
        assert_eq!(last_rssi, -77);
        assert_eq!(last_snr, 5.5);
        assert_eq!(transmit_seconds, 120);
        assert_eq!(receive_seconds, 3_600);
    }

    #[test]
    fn reads_a_negative_snr() {
        // LoRa decodes below the noise floor, so this is the common case.
        let mut payload = vec![u8::from(Response::Stats), 1];
        payload.extend_from_slice(&(-98_i16).to_le_bytes());
        payload.push(0);
        payload.push((-7.25_f32 * 4.0) as i8 as u8);
        payload.extend_from_slice(&[0; 8]);

        let Ok(Stats::Radio { last_snr, .. }) = Stats::parse(&payload) else {
            panic!("expected radio statistics");
        };

        assert_eq!(last_snr, -7.25);
    }

    #[test]
    fn reads_the_packet_counters() {
        let mut payload = vec![u8::from(Response::Stats), 2];
        for value in [1_000_u32, 800, 300, 500, 600, 400, 12] {
            payload.extend_from_slice(&value.to_le_bytes());
        }

        let Ok(Stats::Packets {
            received,
            sent,
            sent_flood,
            receive_errors,
            ..
        }) = Stats::parse(&payload)
        else {
            panic!("expected packet statistics");
        };

        assert_eq!(received, 1_000);
        assert_eq!(sent, 800);
        assert_eq!(sent_flood, 300);
        assert_eq!(receive_errors, 12);
    }

    #[test]
    fn refuses_a_kind_it_does_not_know() {
        // Each kind has its own length and layout, so an unknown one cannot be
        // read at all — not even partially.
        assert_eq!(
            Stats::parse(&[u8::from(Response::Stats), 9, 0, 0]),
            Err(StatsError::UnknownKind { kind: 9 })
        );
    }

    #[test]
    fn refuses_a_payload_that_ends_early() {
        assert_eq!(
            Stats::parse(&CORE_FROM_HARDWARE[..6]),
            Err(StatsError::TooShort { len: 6, needed: 11 })
        );
    }

    #[test]
    fn refuses_something_that_is_not_statistics() {
        assert_eq!(
            Stats::parse(&[u8::from(Response::Ok), 0]),
            Err(StatsError::WrongOpcode { opcode: 0 })
        );
    }
}
