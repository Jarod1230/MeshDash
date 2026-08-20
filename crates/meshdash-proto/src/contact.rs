//! Contacts, as the node reports them.
//!
//! `RESP_CODE_CONTACT` arrives once per contact after `CMD_GET_CONTACTS`,
//! framed by `RESP_CODE_CONTACTS_START` and `RESP_CODE_END_OF_CONTACTS`.
//!
//! # Layout
//!
//! Source: `writeContactRespFrame()` in `examples/companion_radio/MyMesh.cpp`,
//! MeshCore commit `d929643`. `PUB_KEY_SIZE` (32) and `MAX_PATH_SIZE` (64) from
//! `src/MeshCore.h` of the same commit; the route encoding lives in
//! [`crate::path`].
//!
//! ```text
//! offset  size  field
//!      0     1  opcode
//!      1    32  public key
//!     33     1  contact type
//!     34     1  flags
//!     35     1  used length of the path
//!     36    64  path, fixed width regardless of the length above
//!    100    32  name, NUL-padded
//!    132     4  last advert timestamp (u32 little-endian)
//!    136     4  latitude  (i32 little-endian, micro-degrees)
//!    140     4  longitude (i32 little-endian, micro-degrees)
//!    144     4  last modified (u32 little-endian)
//! ```
//!
//! # The path field is wider than the path
//!
//! `out_path` is always 64 bytes on the wire; only the first `out_path_len` of
//! them mean anything. Reading all 64 would append whatever the previous, longer
//! path left behind — a route that looks plausible and never existed.
//!
//! # The length byte is not a length
//!
//! `out_path_len` packs two fields — how many stations, and how many bytes
//! each takes. See [`crate::path`]; `0xFF` is `OUT_PATH_UNKNOWN` and `64`
//! means *zero* stations, not sixty-four. Reading the byte as a count turned a
//! contact list full of unreachable nodes into a mesh where everything sat 64
//! hops away, which is how real hardware exposed it.
//!
//! # Coordinates are micro-degrees
//!
//! The firmware multiplies by 1e6 when storing and divides when reading, and
//! rejects anything beyond ±90e6 / ±180e6 (`CMD_SET_ADVERT_LATLON`). Treating
//! the raw number as degrees would put every node somewhere past the poles.

use crate::opcode::Response;

/// Offsets and widths from the layout above.
mod layout {
    pub const PUB_KEY: usize = 1;
    pub const PUB_KEY_SIZE: usize = 32;
    pub const TYPE: usize = 33;
    pub const FLAGS: usize = 34;
    pub const PATH_LEN: usize = 35;
    pub const PATH: usize = 36;
    pub const NAME: usize = 100;
    pub const NAME_SIZE: usize = 32;
    pub const LAST_ADVERT: usize = 132;
    pub const LATITUDE: usize = 136;
    pub const LONGITUDE: usize = 140;
    pub const LAST_MODIFIED: usize = 144;
    /// Total length of a contact frame.
    pub const LEN: usize = 148;
}

/// A route to a contact, as the node knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// How many stations the packet passes through. Zero means direct.
    pub stations: u8,
    /// The raw hop bytes, `stations × bytes_per_station` of them.
    pub hops: Vec<u8>,
}

/// How the node describes one contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Ed25519 public key; identifies the contact.
    pub public_key: [u8; 32],
    /// What kind of node this is, as the firmware classifies it.
    pub contact_type: u8,
    /// Firmware flags, passed through unread.
    pub flags: u8,
    /// The known route, or `None` when the node has none to this contact.
    ///
    /// A route with zero stations is not the same as no route: it means
    /// reachable directly.
    pub path: Option<Route>,
    /// Display name.
    pub name: String,
    /// When the contact last advertised itself, in seconds.
    pub last_advert: u32,
    /// Latitude in micro-degrees, or `None` if unset.
    pub latitude: Option<i32>,
    /// Longitude in micro-degrees, or `None` if unset.
    pub longitude: Option<i32>,
    /// When the node last changed this entry, in seconds.
    pub last_modified: u32,
}

/// Why a contact payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContactError {
    /// The payload is not a contact.
    #[error("expected a contact, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload is shorter than a contact frame.
    #[error("contact payload is {len} bytes, need {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

impl Contact {
    /// Reads a contact payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, ContactError> {
        match payload.first() {
            Some(&opcode) if Response::from(opcode) == Response::Contact => {}
            Some(&opcode) => return Err(ContactError::WrongOpcode { opcode }),
            None => {
                return Err(ContactError::TooShort {
                    len: 0,
                    needed: layout::LEN,
                });
            }
        }

        Self::parse_body(payload)
    }

    /// Reads the fields of a contact frame without checking the opcode.
    ///
    /// `PUSH_CODE_NEW_ADVERT` carries the same bytes under a different first
    /// byte — the firmware builds both with `writeContactRespFrame()`. See
    /// [`crate::advert`].
    pub(crate) fn parse_body(payload: &[u8]) -> Result<Self, ContactError> {
        if payload.len() < layout::LEN {
            return Err(ContactError::TooShort {
                len: payload.len(),
                needed: layout::LEN,
            });
        }

        let mut public_key = [0u8; layout::PUB_KEY_SIZE];
        public_key.copy_from_slice(&payload[layout::PUB_KEY..layout::PUB_KEY + 32]);

        // The byte says both how many stations and how wide each is; a byte
        // that describes no valid route means the node has none.
        let path = crate::path::decode(payload[layout::PATH_LEN]).map(|shape| Route {
            stations: shape.stations,
            hops: payload[layout::PATH..layout::PATH + shape.byte_len()].to_vec(),
        });

        Ok(Self {
            public_key,
            contact_type: payload[layout::TYPE],
            flags: payload[layout::FLAGS],
            path,
            name: read_name(payload),
            last_advert: read_u32(payload, layout::LAST_ADVERT),
            latitude: read_coordinate(payload, layout::LATITUDE),
            longitude: read_coordinate(payload, layout::LONGITUDE),
            last_modified: read_u32(payload, layout::LAST_MODIFIED),
        })
    }

    /// Latitude in degrees, if the contact has a position.
    pub fn latitude_degrees(&self) -> Option<f64> {
        self.latitude.map(|value| f64::from(value) / 1e6)
    }

    /// Longitude in degrees, if the contact has a position.
    pub fn longitude_degrees(&self) -> Option<f64> {
        self.longitude.map(|value| f64::from(value) / 1e6)
    }
}

/// Reads the NUL-padded name field.
fn read_name(payload: &[u8]) -> String {
    let field = &payload[layout::NAME..layout::NAME + layout::NAME_SIZE];
    let end = field
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(layout::NAME_SIZE);

    String::from_utf8_lossy(&field[..end]).trim().to_owned()
}

/// Reads a little-endian `u32` at `offset`.
fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

/// Reads a coordinate, treating exactly zero as "not set".
///
/// The firmware leaves both fields at zero when no position is known. Zero is a
/// real place off the coast of Africa, so passing it on as a position would put
/// every node without GPS into the Gulf of Guinea.
fn read_coordinate(payload: &[u8], offset: usize) -> Option<i32> {
    let value = read_u32(payload, offset) as i32;
    (value != 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a contact payload as the firmware lays it out.
    ///
    /// Assembled from the source-verified layout, not captured from hardware —
    /// it proves the parser matches our reading, not that the reading is right.
    fn contact_payload() -> Vec<u8> {
        let mut payload = vec![0u8; layout::LEN];
        payload[0] = u8::from(Response::Contact);
        payload[layout::PUB_KEY..layout::PUB_KEY + 32].copy_from_slice(&[0xAB; 32]);
        payload[layout::TYPE] = 2;
        payload[layout::FLAGS] = 1;
        payload[layout::PATH_LEN] = 3;
        // A longer route left behind in the fixed-width field.
        payload[layout::PATH..layout::PATH + 6].copy_from_slice(&[1, 2, 3, 9, 9, 9]);
        payload[layout::NAME..layout::NAME + 8].copy_from_slice(b"Repeater");
        payload[layout::LAST_ADVERT..layout::LAST_ADVERT + 4]
            .copy_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload[layout::LATITUDE..layout::LATITUDE + 4]
            .copy_from_slice(&52_520_008_i32.to_le_bytes());
        payload[layout::LONGITUDE..layout::LONGITUDE + 4]
            .copy_from_slice(&13_404_954_i32.to_le_bytes());
        payload[layout::LAST_MODIFIED..layout::LAST_MODIFIED + 4]
            .copy_from_slice(&1_700_000_100_u32.to_le_bytes());
        payload
    }

    #[test]
    fn reads_the_identifying_fields() {
        let contact = Contact::parse(&contact_payload()).unwrap();

        assert_eq!(contact.public_key, [0xAB; 32]);
        assert_eq!(contact.contact_type, 2);
        assert_eq!(contact.flags, 1);
        assert_eq!(contact.name, "Repeater");
    }

    #[test]
    fn treats_the_unknown_marker_as_no_path_at_all() {
        // 0xFF is OUT_PATH_UNKNOWN, not a length. Clamping it to the field
        // width yields 64 bytes of padding that read as a 64-hop route —
        // a plausible-looking journey that never happened. Found against
        // real hardware, where nearly every contact carries this marker.
        let mut payload = contact_payload();
        payload[layout::PATH_LEN] = 0xFF;

        assert_eq!(Contact::parse(&payload).unwrap().path, None);
    }

    #[test]
    fn a_known_direct_route_is_an_empty_path_not_an_unknown_one() {
        // Zero hops means "reachable directly", which is knowledge. It must
        // not collapse into the same value as "we have no idea".
        let mut payload = contact_payload();
        payload[layout::PATH_LEN] = 0;

        assert_eq!(
            Contact::parse(&payload).unwrap().path,
            Some(Route {
                stations: 0,
                hops: Vec::new()
            })
        );
    }

    #[test]
    fn cuts_the_path_to_its_used_length() {
        // The field is 64 bytes wide whatever the route is. Taking all of it
        // would invent hops from whatever a previous, longer path left there.
        let contact = Contact::parse(&contact_payload()).unwrap();

        assert_eq!(
            contact.path,
            Some(Route {
                stations: 3,
                hops: vec![1, 2, 3]
            })
        );
    }

    #[test]
    fn reads_a_route_whose_stations_are_wider_than_one_byte() {
        // 0x42 = 0b01_000010: two stations, two bytes each.
        let mut payload = contact_payload();
        payload[layout::PATH_LEN] = 0x42;
        payload[layout::PATH..layout::PATH + 4].copy_from_slice(&[1, 2, 3, 4]);

        assert_eq!(
            Contact::parse(&payload).unwrap().path,
            Some(Route {
                stations: 2,
                hops: vec![1, 2, 3, 4]
            })
        );
    }

    #[test]
    fn reads_the_timestamps() {
        let contact = Contact::parse(&contact_payload()).unwrap();

        assert_eq!(contact.last_advert, 1_700_000_000);
        assert_eq!(contact.last_modified, 1_700_000_100);
    }

    #[test]
    fn converts_coordinates_to_degrees() {
        // Micro-degrees on the wire; raw numbers would be far past the poles.
        let contact = Contact::parse(&contact_payload()).unwrap();

        assert_eq!(contact.latitude, Some(52_520_008));
        let latitude = contact.latitude_degrees().unwrap();
        assert!((latitude - 52.520_008).abs() < 1e-6, "got {latitude}");
        let longitude = contact.longitude_degrees().unwrap();
        assert!((longitude - 13.404_954).abs() < 1e-6, "got {longitude}");
    }

    #[test]
    fn treats_a_zero_position_as_unset() {
        // The firmware leaves both at zero when no position is known. Null
        // island is off Africa; showing a node there would be wrong, not empty.
        let mut payload = contact_payload();
        payload[layout::LATITUDE..layout::LATITUDE + 4].copy_from_slice(&0_i32.to_le_bytes());
        payload[layout::LONGITUDE..layout::LONGITUDE + 4].copy_from_slice(&0_i32.to_le_bytes());

        let contact = Contact::parse(&payload).unwrap();

        assert_eq!(contact.latitude, None);
        assert_eq!(contact.longitude, None);
    }

    #[test]
    fn refuses_a_length_byte_that_describes_no_route() {
        // Must not read past the field, whatever the node claims.
        let mut payload = contact_payload();
        payload[layout::PATH_LEN] = 200;

        let contact = Contact::parse(&payload).unwrap();

        // 200 is 0b11_001000 — the reserved hash size, so not a route at all.
        assert_eq!(contact.path, None);
    }

    #[test]
    fn rejects_a_different_response() {
        let payload = vec![u8::from(Response::Ok); layout::LEN];

        assert!(matches!(
            Contact::parse(&payload),
            Err(ContactError::WrongOpcode { .. })
        ));
    }

    #[test]
    fn rejects_a_truncated_payload() {
        let payload = contact_payload()[..100].to_vec();

        assert!(matches!(
            Contact::parse(&payload),
            Err(ContactError::TooShort { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_payload() {
        assert!(Contact::parse(&[]).is_err());
    }
}
