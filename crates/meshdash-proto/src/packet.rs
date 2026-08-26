//! A packet as it comes off the air.
//!
//! The node reports every packet its radio hears, undecoded, in
//! `PUSH_CODE_LOG_RX_DATA` — including packets meant for someone else and
//! packets it goes on to discard. This module reads the part of such a packet
//! that is not encrypted: what kind it is, how it travelled, and which
//! stations passed it on.
//!
//! # What is readable and what is not
//!
//! The payload is encrypted. Readable is what sits in front of it: route type,
//! payload type, and the path. That is enough to say "a text message came in
//! over two stations, this well received" — and not enough to say what it
//! said, which is as it should be for a stranger's traffic.
//!
//! Source: `Packet::readFrom()` and `Packet::writeTo()` in `src/Packet.cpp`,
//! the constants in `src/Packet.h`, MeshCore commit `d929643`. Details and
//! quotes in `docs/research/meshcore-companion-protocol.md`.

use crate::path::{self, PathShape};

/// Why a packet could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    /// The frame ends inside a field.
    ///
    /// Carries what was being read, so the message says where it stopped
    /// rather than only that it did.
    Truncated {
        /// Which field ran out of bytes.
        what: &'static str,
    },
    /// The path-length byte does not describe a route.
    BadPathLength {
        /// The byte as it arrived.
        byte: u8,
    },
    /// The header says "do not retransmit", which is a marker, not a packet.
    ///
    /// `Packet::markDoNotRetransmit()` writes `0xFF` over the whole header.
    /// It never travels over the air; seeing it means the bytes came from
    /// somewhere else.
    NotAPacket,
}

/// How a packet is routed. `PH_ROUTE_MASK` in `src/Packet.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteType {
    /// Flooded, with transport codes. `ROUTE_TYPE_TRANSPORT_FLOOD` (0).
    TransportFlood,
    /// Flooded; every station appends itself to the path.
    /// `ROUTE_TYPE_FLOOD` (1).
    Flood,
    /// Sent along a known path. `ROUTE_TYPE_DIRECT` (2).
    Direct,
    /// Along a known path, with transport codes.
    /// `ROUTE_TYPE_TRANSPORT_DIRECT` (3).
    TransportDirect,
}

impl RouteType {
    /// Reads the two route bits of a header byte.
    fn from_header(header: u8) -> Self {
        match header & 0b0000_0011 {
            0 => Self::TransportFlood,
            1 => Self::Flood,
            2 => Self::Direct,
            // Two bits, four arms — the compiler cannot see that, but the
            // mask can only produce 0..=3.
            _ => Self::TransportDirect,
        }
    }

    /// Whether four bytes of transport codes follow the header.
    ///
    /// `Packet::hasTransportCodes()`.
    pub fn has_transport_codes(self) -> bool {
        matches!(self, Self::TransportFlood | Self::TransportDirect)
    }

    /// Whether the packet is flooded rather than routed.
    ///
    /// Worth knowing when reading a path: a flooded packet collects its path
    /// on the way, a direct one carries the path it was given.
    pub fn is_flood(self) -> bool {
        matches!(self, Self::TransportFlood | Self::Flood)
    }
}

/// What a packet carries. `PAYLOAD_TYPE_*` in `src/Packet.h`.
///
/// `Unknown` is not a defect: a later firmware may use one of the values this
/// table leaves out, and a packet nobody can name is still a packet that was
/// heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadType {
    /// A request. `PAYLOAD_TYPE_REQ` (0x00).
    Request,
    /// An answer to a request. `PAYLOAD_TYPE_RESPONSE` (0x01).
    Response,
    /// A text message. `PAYLOAD_TYPE_TXT_MSG` (0x02).
    TextMessage,
    /// An acknowledgement. `PAYLOAD_TYPE_ACK` (0x03).
    Ack,
    /// A node introducing itself. `PAYLOAD_TYPE_ADVERT` (0x04).
    Advert,
    /// A channel message. `PAYLOAD_TYPE_GRP_TXT` (0x05).
    GroupText,
    /// A channel datagram. `PAYLOAD_TYPE_GRP_DATA` (0x06).
    GroupData,
    /// A request without a sender identity. `PAYLOAD_TYPE_ANON_REQ` (0x07).
    AnonymousRequest,
    /// A returned route. `PAYLOAD_TYPE_PATH` (0x08).
    Path,
    /// A route being traced, collecting SNR per leg.
    /// `PAYLOAD_TYPE_TRACE` (0x09).
    Trace,
    /// One of a set. `PAYLOAD_TYPE_MULTIPART` (0x0A).
    Multipart,
    /// Control and discovery. `PAYLOAD_TYPE_CONTROL` (0x0B).
    Control,
    /// An application's own format. `PAYLOAD_TYPE_RAW_CUSTOM` (0x0F).
    RawCustom,
    /// A value this table does not know.
    Unknown(u8),
}

impl PayloadType {
    /// Reads the four type bits of a header byte.
    fn from_header(header: u8) -> Self {
        match (header >> 2) & 0b0000_1111 {
            0x00 => Self::Request,
            0x01 => Self::Response,
            0x02 => Self::TextMessage,
            0x03 => Self::Ack,
            0x04 => Self::Advert,
            0x05 => Self::GroupText,
            0x06 => Self::GroupData,
            0x07 => Self::AnonymousRequest,
            0x08 => Self::Path,
            0x09 => Self::Trace,
            0x0A => Self::Multipart,
            0x0B => Self::Control,
            0x0F => Self::RawCustom,
            other => Self::Unknown(other),
        }
    }
}

/// A packet heard over the air, as far as it can be read without keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet<'a> {
    /// How it travelled.
    pub route: RouteType,
    /// What it carries.
    pub payload_type: PayloadType,
    /// Payload version, `PH_VER_SHIFT`. Zero is the only one in use.
    pub version: u8,
    /// The two transport codes, where the route type carries them.
    pub transport_codes: Option<[u16; 2]>,
    /// The shape of the path field: how many stations, how wide each is.
    pub shape: PathShape,
    /// The path itself, one entry per station, in travel order.
    ///
    /// Each entry is the **start of a public key**, not a hash of one —
    /// see [`Station`].
    pub path: Vec<Station<'a>>,
    /// The encrypted payload, untouched.
    pub payload: &'a [u8],
}

/// One station on a packet's path.
///
/// # Not a hash
///
/// MeshCore calls this a hash, and it is not one. `Identity::copyHashTo()`
/// copies the first bytes of the public key verbatim — its own comment says
/// "hash is just prefix of pub_key". A station can therefore be matched
/// against known contacts by comparing the start of their keys.
///
/// # How wide, decides the sender
///
/// One to three bytes, and the packet says which: the width sits in the top
/// two bits of the path-length byte. The sender picks it when flooding —
/// `sendFlood(pkt, delay, _prefs.path_hash_mode + 1)` in `MyMesh.cpp`, set
/// with `CMD_SET_PATH_HASH_MODE` — and every station that forwards the packet
/// keeps that width (`packet->getPathHashSize()` in `Mesh::routeRecvPacket`).
///
/// `PATH_HASH_SIZE` (1) is only the default of the one-argument
/// `Identity::copyHashTo`. It is **not** what arrives: a mesh observed on
/// 2026-08-26 sent two bytes per station throughout.
///
/// # A prefix is not an identity
///
/// How weak the match is depends on that width. At one byte, 256 values, two
/// of a few dozen nodes sharing a first byte is likelier than not; at three,
/// a collision is remote. Either way matching must handle "several fit" — the
/// same problem as the six-byte sender prefix on a message, and the same
/// answer: name nobody rather than name the wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Station<'a> {
    /// The leading bytes of that node's public key.
    pub key_prefix: &'a [u8],
}

impl Station<'_> {
    /// Whether this station could be the node with that public key.
    ///
    /// "Could be": with a one-byte prefix this is a weak statement, and the
    /// caller has to say how many candidates it found.
    pub fn matches(&self, public_key: &[u8]) -> bool {
        public_key.starts_with(self.key_prefix)
    }
}

impl<'a> Packet<'a> {
    /// Reads a packet as it came off the air.
    ///
    /// Mirrors `Packet::readFrom()`, including its refusals: an invalid path
    /// length and a frame that ends inside a field are both errors there too.
    pub fn parse(raw: &'a [u8]) -> Result<Self, PacketError> {
        let (&header, rest) = raw
            .split_first()
            .ok_or(PacketError::Truncated { what: "the header" })?;

        // `markDoNotRetransmit()` writes 0xFF over the whole header. It is an
        // internal marker and never travels; treating it as a packet would
        // report a route type and a payload type that were never sent.
        if header == 0xFF {
            return Err(PacketError::NotAPacket);
        }

        let route = RouteType::from_header(header);

        let (transport_codes, rest) = if route.has_transport_codes() {
            let bytes: &[u8; 4] = rest
                .get(..4)
                .and_then(|slice| slice.try_into().ok())
                .ok_or(PacketError::Truncated {
                    what: "the transport codes",
                })?;
            let first = u16::from_le_bytes([bytes[0], bytes[1]]);
            let second = u16::from_le_bytes([bytes[2], bytes[3]]);
            (Some([first, second]), &rest[4..])
        } else {
            (None, rest)
        };

        let (&path_len, rest) = rest.split_first().ok_or(PacketError::Truncated {
            what: "the path length",
        })?;
        let shape = path::decode(path_len).ok_or(PacketError::BadPathLength { byte: path_len })?;

        let path_bytes = rest
            .get(..shape.byte_len())
            .ok_or(PacketError::Truncated { what: "the path" })?;
        let payload = &rest[shape.byte_len()..];

        let path = path_bytes
            .chunks_exact(usize::from(shape.bytes_per_station))
            .map(|key_prefix| Station { key_prefix })
            .collect();

        Ok(Self {
            route,
            payload_type: PayloadType::from_header(header),
            version: header >> 6,
            transport_codes,
            shape,
            path,
            payload,
        })
    }

    /// How many stations passed this packet on.
    pub fn stations(&self) -> usize {
        self.path.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flooded text message over two stations, as the radio would hand it up.
    fn text_over_two_stations() -> Vec<u8> {
        vec![
            // route 1 (flood), type 2 (text), version 0
            0b0000_1001,
            // one byte per station, two stations
            0b0000_0010,
            0xAA,
            0xBB,
            // encrypted remainder
            0x01,
            0x02,
            0x03,
        ]
    }

    #[test]
    fn reads_the_three_fields_of_the_header() {
        let raw = text_over_two_stations();
        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.route, RouteType::Flood);
        assert_eq!(packet.payload_type, PayloadType::TextMessage);
        assert_eq!(packet.version, 0);
    }

    #[test]
    fn reads_the_stations_in_travel_order() {
        let raw = text_over_two_stations();
        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.stations(), 2);
        assert_eq!(packet.path[0].key_prefix, &[0xAA]);
        assert_eq!(packet.path[1].key_prefix, &[0xBB]);
        assert_eq!(packet.payload, &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn a_station_matches_a_key_that_starts_with_it() {
        let raw = text_over_two_stations();
        let packet = Packet::parse(&raw).unwrap();
        let station = packet.path[0];

        let mut key = [0u8; 32];
        key[0] = 0xAA;
        assert!(station.matches(&key));

        let mut other = [0u8; 32];
        other[0] = 0xAB;
        assert!(!station.matches(&other));
    }

    #[test]
    fn reads_two_byte_stations() {
        // Top bits 01: two bytes per station. Two stations, four bytes.
        let raw = [0b0000_1001, 0b0100_0010, 0xAA, 0x11, 0xBB, 0x22, 0x99];

        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.shape.bytes_per_station, 2);
        assert_eq!(packet.path[0].key_prefix, &[0xAA, 0x11]);
        assert_eq!(packet.path[1].key_prefix, &[0xBB, 0x22]);
        assert_eq!(packet.payload, &[0x99]);
    }

    #[test]
    fn reads_transport_codes_only_where_the_route_carries_them() {
        // Route 0 (transport flood): four bytes of codes sit between header
        // and path length. Reading them as path would shift everything.
        let raw = [0b0000_1000, 0x34, 0x12, 0x78, 0x56, 0x01, 0xAA, 0x42];

        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.route, RouteType::TransportFlood);
        assert_eq!(packet.transport_codes, Some([0x1234, 0x5678]));
        assert_eq!(packet.path[0].key_prefix, &[0xAA]);
        assert_eq!(packet.payload, &[0x42]);
    }

    #[test]
    fn a_packet_without_stations_still_has_a_payload() {
        // Direct route, no path recorded: the common shape for a packet
        // received straight from its sender.
        let raw = [0b0000_1010, 0x00, 0xDE, 0xAD];

        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.route, RouteType::Direct);
        assert_eq!(packet.stations(), 0);
        assert_eq!(packet.payload, &[0xDE, 0xAD]);
    }

    #[test]
    fn an_unknown_payload_type_keeps_its_number() {
        // 0x0C is unused today. A later firmware may use it, and a packet
        // nobody can name is still one that was heard.
        let raw = [0b0011_0001, 0x00];

        let packet = Packet::parse(&raw).unwrap();

        assert_eq!(packet.payload_type, PayloadType::Unknown(0x0C));
    }

    #[test]
    fn refuses_the_do_not_retransmit_marker() {
        // 0xFF as a header is `markDoNotRetransmit()`, not a packet. Read as
        // one it would claim route 3 and payload type 0x0F.
        assert_eq!(Packet::parse(&[0xFF, 0x00]), Err(PacketError::NotAPacket));
    }

    #[test]
    fn refuses_a_path_length_that_is_not_a_route() {
        // 0xFE: hash size 4, which the firmware reserves and refuses.
        assert_eq!(
            Packet::parse(&[0b0000_1001, 0xFE, 0x00]),
            Err(PacketError::BadPathLength { byte: 0xFE })
        );
    }

    #[test]
    fn says_which_field_ran_out_of_bytes() {
        assert_eq!(
            Packet::parse(&[]),
            Err(PacketError::Truncated { what: "the header" })
        );
        assert_eq!(
            Packet::parse(&[0b0000_1001]),
            Err(PacketError::Truncated {
                what: "the path length"
            })
        );
        // Two stations announced, one delivered.
        assert_eq!(
            Packet::parse(&[0b0000_1001, 0x02, 0xAA]),
            Err(PacketError::Truncated { what: "the path" })
        );
        assert_eq!(
            Packet::parse(&[0b0000_1000, 0x34, 0x12]),
            Err(PacketError::Truncated {
                what: "the transport codes"
            })
        );
    }

    #[test]
    fn reads_what_the_packet_log_push_carries() {
        // The two halves have to line up: the push hands over everything from
        // byte 3 onwards, and that is exactly one packet. Tested together,
        // because an off-by-one between them would show as nonsense route
        // types on real traffic and as nothing at all in either unit test.
        use crate::opcode::Push;
        use crate::push::PushEvent;

        let mut frame = vec![u8::from(Push::LogRxData)];
        frame.push((-3.5_f32 * 4.0) as i8 as u8);
        frame.push(-92_i8 as u8);
        frame.extend_from_slice(&text_over_two_stations());

        let PushEvent::ReceivedPacketLog { snr, rssi, packet } = PushEvent::parse(&frame).unwrap()
        else {
            panic!("expected a packet log");
        };

        let read = Packet::parse(&packet).unwrap();

        assert_eq!(snr, -3.5);
        assert_eq!(rssi, -92);
        assert_eq!(read.payload_type, PayloadType::TextMessage);
        assert_eq!(read.stations(), 2);
    }

    #[test]
    fn an_empty_payload_is_not_an_error() {
        // The firmware refuses this (`if (i >= len) return false`), but it
        // refuses it when *building* a packet to act on. A listener that
        // throws away what it heard because the sender was odd learns less
        // than one that records "an empty advert arrived".
        let packet = Packet::parse(&[0b0001_0001, 0x00]).unwrap();

        assert_eq!(packet.payload_type, PayloadType::Advert);
        assert!(packet.payload.is_empty());
    }
}
