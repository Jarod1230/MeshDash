//! Everything the node says without being asked.
//!
//! # One place for all of them
//!
//! Until now each module checked opcodes itself and read the payloads it cared
//! about. That works while two kinds matter and stops working at fifteen: the
//! knowledge of what a push *is* ends up spread across modules that have no
//! business holding it. [`PushEvent::parse`] answers that question once.
//!
//! # Layout
//!
//! Source: the `on…Recv()` methods and `processAck()` in
//! `examples/companion_radio/MyMesh.cpp`, MeshCore commit `d929643`.
//!
//! ```text
//! 0x81 path updated        32  public key
//! 0x82 send confirmed       4  acknowledgement, 4  round trip in ms
//! 0x84 raw data             1  SNR×4, 1 RSSI, 1 reserved (0xFF), …  payload
//! 0x85 login succeeded      1  permissions, 6 sender prefix, [4 tag]
//! 0x86 login failed         1  reserved, 6 sender prefix
//! 0x87 status response      1  reserved, 6 sender prefix, …  payload
//! 0x88 received packet log  1  SNR×4, 1 RSSI, …  the raw packet
//! 0x89 trace                1  reserved, 1 path length, 1 flags,
//!                           4  tag, 4 authentication code,
//!                           …  hop hashes, … one SNR per group, 1 final SNR
//! 0x8B telemetry response   1  reserved, 6 sender prefix, …  CayenneLPP
//! 0x8D path discovery       1  reserved, 6 sender prefix,
//!                           1  outbound length, …  outbound route,
//!                           1  inbound length,  …  inbound route
//! 0x8E control data         1  SNR×4, 1 RSSI, 1 path length, …  payload
//! 0x8F contact deleted     32  public key
//! 0x90 contacts full        —
//! ```
//!
//! # An unknown push is kept, not dropped
//!
//! Newer firmware will send kinds this build has never heard of. They arrive
//! as [`PushEvent::Unknown`] with their bytes intact, so a module can log them
//! and a later version can read them.

use crate::{
    advert::Advert,
    binary_request::BinaryResponse,
    lpp,
    opcode::{self, Push},
    path,
};

/// A six-byte prefix of someone's public key.
///
/// Six bytes can collide. Whatever matches this to a contact has to treat the
/// result as probable, not certain.
pub type SenderPrefix = [u8; 6];

/// Something the node reported on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum PushEvent {
    /// A node was heard over the air.
    Advert(Advert),

    /// The route to a contact changed.
    PathUpdated {
        /// Whose route.
        public_key: [u8; 32],
    },

    /// A message this node sent was acknowledged.
    SendConfirmed {
        /// Matches the acknowledgement from the send receipt.
        acknowledgement: u32,
        /// How long the round trip took, in milliseconds.
        round_trip_ms: u32,
        /// The same acknowledgement may arrive more than once — the firmware
        /// says so in its own comment. Counting these counts too high.
        repeat_possible: bool,
    },

    /// Messages are waiting to be fetched.
    MessageWaiting,

    /// A raw datagram arrived.
    RawData {
        /// Signal-to-noise ratio in dB.
        snr: f32,
        /// RSSI in dBm.
        rssi: i8,
        /// The payload, undecoded.
        payload: Vec<u8>,
    },

    /// A login to a repeater or room server succeeded.
    LoginSucceeded {
        /// Permission bits the server granted; `0` for a legacy repeater.
        permissions: u8,
        /// Six bytes of the server's key.
        sender_prefix: SenderPrefix,
        /// Present only on the newer login response.
        tag: Option<u32>,
    },

    /// A login was refused.
    LoginFailed {
        /// Six bytes of the server's key.
        sender_prefix: SenderPrefix,
    },

    /// A contact answered a status request.
    StatusResponse {
        /// Six bytes of the answering node's key.
        sender_prefix: SenderPrefix,
        /// The answer, undecoded — its shape belongs to the responding node.
        payload: Vec<u8>,
    },

    /// A packet the radio heard, logged in full.
    ReceivedPacketLog {
        /// Signal-to-noise ratio in dB.
        snr: f32,
        /// RSSI in dBm.
        rssi: i8,
        /// The packet as it came off the air.
        packet: Vec<u8>,
    },

    /// A trace came back — the mesh equivalent of a traceroute.
    Trace {
        /// Matches the tag of the trace that was sent.
        tag: u32,
        /// Authentication code the trace carried.
        authentication_code: u32,
        /// One hash per station on the way.
        hop_hashes: Vec<u8>,
        /// Signal-to-noise ratio reported per group of stations, in dB.
        hop_snrs: Vec<f32>,
        /// Signal-to-noise ratio of the last leg, to this node.
        final_snr: f32,
    },

    /// A contact answered a telemetry request.
    TelemetryResponse {
        /// Six bytes of the answering node's key.
        sender_prefix: SenderPrefix,
        /// What it reported.
        readings: lpp::Decoded,
    },

    /// A contact answered a binary request.
    BinaryResponse(BinaryResponse),

    /// A path discovery came back with both directions.
    PathDiscovered {
        /// Six bytes of the answering node's key.
        sender_prefix: SenderPrefix,
        /// The route out to them, or `None` if the byte describes no route.
        outbound_stations: Option<u8>,
        /// The route back, or `None`.
        inbound_stations: Option<u8>,
    },

    /// Control data arrived.
    ControlData {
        /// Signal-to-noise ratio in dB.
        snr: f32,
        /// RSSI in dBm.
        rssi: i8,
        /// The payload, undecoded.
        payload: Vec<u8>,
    },

    /// A contact was pushed out of the node's table.
    ContactDeleted {
        /// Which one.
        public_key: [u8; 32],
    },

    /// The node's contact store is full.
    ContactsFull,

    /// A kind this build does not know, kept whole.
    Unknown {
        /// The opcode.
        opcode: u8,
        /// Everything, opcode byte included.
        payload: Vec<u8>,
    },
}

/// Why a push could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PushError {
    /// The frame is empty.
    #[error("a push frame cannot be empty")]
    Empty,

    /// The frame does not carry a push opcode at all.
    #[error("opcode {opcode:#04x} is not a push")]
    NotAPush {
        /// What the first byte was.
        opcode: u8,
    },

    /// The frame is shorter than its kind requires.
    #[error("{kind} push is {len} bytes, need at least {needed}")]
    TooShort {
        /// Which kind.
        kind: &'static str,
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl PushEvent {
    /// Reads any push frame, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, PushError> {
        let Some(&opcode) = payload.first() else {
            return Err(PushError::Empty);
        };

        if !opcode::is_push(opcode) {
            return Err(PushError::NotAPush { opcode });
        }

        match Push::from(opcode) {
            // These have their own parsers; this is the one place that knows
            // which opcode leads where.
            Push::Advert | Push::NewAdvert => {
                Advert::parse(payload)
                    .map(Self::Advert)
                    .map_err(|_| PushError::TooShort {
                        kind: "advert",
                        len: payload.len(),
                        needed: 33,
                    })
            }

            Push::BinaryResponse => BinaryResponse::parse(payload)
                .map(Self::BinaryResponse)
                .map_err(|_| PushError::TooShort {
                    kind: "binary response",
                    len: payload.len(),
                    needed: 6,
                }),

            Push::MsgWaiting => Ok(Self::MessageWaiting),
            Push::ContactsFull => Ok(Self::ContactsFull),

            Push::PathUpdated => {
                key_only(payload, "path updated").map(|public_key| Self::PathUpdated { public_key })
            }

            Push::ContactDeleted => key_only(payload, "contact deleted")
                .map(|public_key| Self::ContactDeleted { public_key }),

            Push::SendConfirmed => {
                need(payload, 9, "send confirmed")?;
                Ok(Self::SendConfirmed {
                    acknowledgement: read_u32(payload, 1),
                    round_trip_ms: read_u32(payload, 5),
                    repeat_possible: true,
                })
            }

            Push::RawData => {
                need(payload, 4, "raw data")?;
                Ok(Self::RawData {
                    snr: snr(payload[1]),
                    rssi: payload[2] as i8,
                    // Byte 3 is reserved and always 0xFF today.
                    payload: payload[4..].to_vec(),
                })
            }

            Push::ControlData => {
                need(payload, 4, "control data")?;
                Ok(Self::ControlData {
                    snr: snr(payload[1]),
                    rssi: payload[2] as i8,
                    payload: payload[4..].to_vec(),
                })
            }

            Push::LogRxData => {
                need(payload, 3, "packet log")?;
                Ok(Self::ReceivedPacketLog {
                    snr: snr(payload[1]),
                    rssi: payload[2] as i8,
                    packet: payload[3..].to_vec(),
                })
            }

            Push::LoginSuccess => {
                need(payload, 8, "login success")?;
                Ok(Self::LoginSucceeded {
                    permissions: payload[1],
                    sender_prefix: prefix(payload, 2),
                    // The newer response appends a tag; the legacy one stops
                    // after the prefix.
                    tag: (payload.len() >= 12).then(|| read_u32(payload, 8)),
                })
            }

            Push::LoginFail => {
                need(payload, 8, "login failure")?;
                Ok(Self::LoginFailed {
                    sender_prefix: prefix(payload, 2),
                })
            }

            Push::StatusResponse => {
                need(payload, 8, "status response")?;
                Ok(Self::StatusResponse {
                    sender_prefix: prefix(payload, 2),
                    payload: payload[8..].to_vec(),
                })
            }

            Push::TelemetryResponse => {
                need(payload, 8, "telemetry response")?;
                Ok(Self::TelemetryResponse {
                    sender_prefix: prefix(payload, 2),
                    readings: lpp::decode(&payload[8..]),
                })
            }

            Push::PathDiscoveryResponse => {
                need(payload, 9, "path discovery")?;
                let outbound = path::decode(payload[8]);
                let outbound_len = outbound.map_or(0, |shape| shape.byte_len());

                // The inbound length byte sits behind the outbound route, so
                // its position depends on how long that route was.
                let inbound_at = 9 + outbound_len;
                let inbound = payload.get(inbound_at).copied().and_then(path::decode);

                Ok(Self::PathDiscovered {
                    sender_prefix: prefix(payload, 2),
                    outbound_stations: outbound.map(|shape| shape.stations),
                    inbound_stations: inbound.map(|shape| shape.stations),
                })
            }

            Push::TraceData => {
                need(payload, 12, "trace")?;
                let path_len = usize::from(payload[2]);
                let flags = payload[3];
                // The low two bits say how many hops share one SNR reading.
                let group_shift = flags & 0b0000_0011;
                let snr_count = path_len >> group_shift;

                let hashes_end = 12 + path_len;
                let snrs_end = hashes_end + snr_count;
                need(payload, snrs_end + 1, "trace")?;

                Ok(Self::Trace {
                    tag: read_u32(payload, 4),
                    authentication_code: read_u32(payload, 8),
                    hop_hashes: payload[12..hashes_end].to_vec(),
                    hop_snrs: payload[hashes_end..snrs_end]
                        .iter()
                        .map(|&byte| snr(byte))
                        .collect(),
                    final_snr: snr(payload[snrs_end]),
                })
            }

            Push::Unknown(opcode) => Ok(Self::Unknown {
                opcode,
                payload: payload.to_vec(),
            }),
        }
    }
}

/// SNR travels multiplied by four and signed, everywhere in this protocol.
fn snr(byte: u8) -> f32 {
    f32::from(byte as i8) / 4.0
}

fn read_u32(payload: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([
        payload[at],
        payload[at + 1],
        payload[at + 2],
        payload[at + 3],
    ])
}

fn prefix(payload: &[u8], at: usize) -> SenderPrefix {
    let mut prefix = [0u8; 6];
    prefix.copy_from_slice(&payload[at..at + 6]);
    prefix
}

fn need(payload: &[u8], length: usize, kind: &'static str) -> Result<(), PushError> {
    if payload.len() < length {
        return Err(PushError::TooShort {
            kind,
            len: payload.len(),
            needed: length,
        });
    }

    Ok(())
}

fn key_only(payload: &[u8], kind: &'static str) -> Result<[u8; 32], PushError> {
    need(payload, 33, kind)?;

    let mut key = [0u8; 32];
    key.copy_from_slice(&payload[1..33]);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_kinds_that_carry_nothing() {
        assert_eq!(
            PushEvent::parse(&[u8::from(Push::MsgWaiting)]),
            Ok(PushEvent::MessageWaiting)
        );
        assert_eq!(
            PushEvent::parse(&[u8::from(Push::ContactsFull)]),
            Ok(PushEvent::ContactsFull)
        );
    }

    #[test]
    fn reads_the_kinds_that_carry_a_key() {
        let mut payload = vec![u8::from(Push::PathUpdated)];
        payload.extend_from_slice(&[0xAB; 32]);

        assert_eq!(
            PushEvent::parse(&payload),
            Ok(PushEvent::PathUpdated {
                public_key: [0xAB; 32]
            })
        );
    }

    #[test]
    fn reads_a_send_confirmation() {
        let mut payload = vec![u8::from(Push::SendConfirmed)];
        payload.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        payload.extend_from_slice(&450_u32.to_le_bytes());

        let Ok(PushEvent::SendConfirmed {
            acknowledgement,
            round_trip_ms,
            repeat_possible,
        }) = PushEvent::parse(&payload)
        else {
            panic!("expected a send confirmation");
        };

        assert_eq!(acknowledgement, 0x1234_5678);
        assert_eq!(round_trip_ms, 450);
        // The firmware's own comment warns that the same acknowledgement can
        // arrive more than once. Counting these would count too high.
        assert!(repeat_possible);
    }

    #[test]
    fn reads_a_successful_login_with_and_without_a_tag() {
        let mut legacy = vec![u8::from(Push::LoginSuccess), 0];
        legacy.extend_from_slice(&[0xAA; 6]);

        let Ok(PushEvent::LoginSucceeded {
            permissions, tag, ..
        }) = PushEvent::parse(&legacy)
        else {
            panic!("expected a login success");
        };
        assert_eq!(permissions, 0, "a legacy repeater grants nothing");
        assert_eq!(tag, None);

        let mut modern = legacy.clone();
        modern[1] = 0x01; // is_admin
        modern.extend_from_slice(&7_u32.to_le_bytes());

        let Ok(PushEvent::LoginSucceeded {
            permissions, tag, ..
        }) = PushEvent::parse(&modern)
        else {
            panic!("expected a login success");
        };
        assert_eq!(permissions, 0x01);
        assert_eq!(tag, Some(7));
    }

    #[test]
    fn reads_a_telemetry_answer_through_the_lpp_decoder() {
        let mut payload = vec![u8::from(Push::TelemetryResponse), 0];
        payload.extend_from_slice(&[0xCD; 6]);
        // Channel 1, voltage, 4.02 V — big-endian, as CayenneLPP is.
        payload.extend_from_slice(&[1, 116, 0x01, 0x92]);

        let Ok(PushEvent::TelemetryResponse {
            sender_prefix,
            readings,
        }) = PushEvent::parse(&payload)
        else {
            panic!("expected telemetry");
        };

        assert_eq!(sender_prefix, [0xCD; 6]);
        assert_eq!(readings.readings.len(), 1);
        assert_eq!(readings.stopped, None);
    }

    #[test]
    fn reads_a_trace_with_one_reading_per_hop() {
        let mut payload = vec![u8::from(Push::TraceData), 0, 3, 0];
        payload.extend_from_slice(&99_u32.to_le_bytes());
        payload.extend_from_slice(&0xABCD_u32.to_le_bytes());
        payload.extend_from_slice(&[0x11, 0x22, 0x33]); // three hop hashes
        payload.extend_from_slice(&[20, 12, (-8_i8 * 4) as u8]); // one SNR each
        payload.push((6.0_f32 * 4.0) as u8); // the final leg

        let Ok(PushEvent::Trace {
            tag,
            authentication_code,
            hop_hashes,
            hop_snrs,
            final_snr,
        }) = PushEvent::parse(&payload)
        else {
            panic!("expected a trace");
        };

        assert_eq!(tag, 99);
        assert_eq!(authentication_code, 0xABCD);
        assert_eq!(hop_hashes, vec![0x11, 0x22, 0x33]);
        assert_eq!(hop_snrs, vec![5.0, 3.0, -8.0]);
        assert_eq!(final_snr, 6.0);
    }

    #[test]
    fn a_trace_can_group_several_hops_under_one_reading() {
        // The low two bits of the flags say how many hops share one SNR: a
        // shift of one means half as many readings as hashes.
        let mut payload = vec![u8::from(Push::TraceData), 0, 4, 0b01];
        payload.extend_from_slice(&[0; 8]); // tag and authentication code
        payload.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        payload.extend_from_slice(&[20, 12]); // 4 >> 1 = two readings
        payload.push(0);

        let Ok(PushEvent::Trace {
            hop_snrs,
            hop_hashes,
            ..
        }) = PushEvent::parse(&payload)
        else {
            panic!("expected a trace");
        };

        assert_eq!(hop_hashes.len(), 4);
        assert_eq!(hop_snrs.len(), 2);
    }

    #[test]
    fn reads_both_directions_of_a_path_discovery() {
        let mut payload = vec![u8::from(Push::PathDiscoveryResponse), 0];
        payload.extend_from_slice(&[0xEE; 6]);
        payload.push(2); // outbound: two stations, one byte each
        payload.extend_from_slice(&[0x01, 0x02]);
        payload.push(1); // inbound: one station
        payload.push(0x03);

        let Ok(PushEvent::PathDiscovered {
            outbound_stations,
            inbound_stations,
            ..
        }) = PushEvent::parse(&payload)
        else {
            panic!("expected a path discovery");
        };

        assert_eq!(outbound_stations, Some(2));
        assert_eq!(inbound_stations, Some(1), "found behind the outbound route");
    }

    #[test]
    fn reads_the_signal_values_of_a_logged_packet() {
        let mut payload = vec![u8::from(Push::LogRxData)];
        payload.push((-3.5_f32 * 4.0) as i8 as u8);
        payload.push(-92_i8 as u8);
        payload.extend_from_slice(&[0xDE, 0xAD]);

        let Ok(PushEvent::ReceivedPacketLog { snr, rssi, packet }) = PushEvent::parse(&payload)
        else {
            panic!("expected a packet log");
        };

        // Both are ordinarily negative; unsigned reading reports a radio that
        // hears better than physics allows.
        assert_eq!(snr, -3.5);
        assert_eq!(rssi, -92);
        assert_eq!(packet, vec![0xDE, 0xAD]);
    }

    #[test]
    fn keeps_a_kind_it_does_not_know() {
        // Newer firmware will send kinds this build has never heard of.
        // Dropping them would lose evidence a later version could read.
        let payload = vec![0x9F, 1, 2, 3];

        assert_eq!(
            PushEvent::parse(&payload),
            Ok(PushEvent::Unknown {
                opcode: 0x9F,
                payload: vec![0x9F, 1, 2, 3]
            })
        );
    }

    #[test]
    fn refuses_a_frame_that_is_not_a_push_at_all() {
        assert_eq!(
            PushEvent::parse(&[0x03, 0, 0]),
            Err(PushError::NotAPush { opcode: 0x03 })
        );
        assert_eq!(PushEvent::parse(&[]), Err(PushError::Empty));
    }

    #[test]
    fn refuses_a_push_that_ends_before_its_fields() {
        assert_eq!(
            PushEvent::parse(&[u8::from(Push::SendConfirmed), 1, 2]),
            Err(PushError::TooShort {
                kind: "send confirmed",
                len: 3,
                needed: 9
            })
        );
    }
}
