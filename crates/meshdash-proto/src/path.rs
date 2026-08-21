//! How MeshCore packs a route into a single byte.
//!
//! # Two fields in one byte
//!
//! Source: `Packet::isValidPathLen()` and `Packet::writePath()` in
//! `src/Packet.cpp`, MeshCore commit `d929643`.
//!
//! ```c
//! uint8_t hash_count = path_len & 63;
//! uint8_t hash_size  = (path_len >> 6) + 1;
//! if (hash_size == 4) return false;            // reserved
//! return hash_count * hash_size <= MAX_PATH_SIZE;
//! ```
//!
//! ```text
//! bit  7 6 5 4 3 2 1 0
//!      \_/ \_________/
//!       |       └── how many stations the packet passed
//!       └────────── bytes per station, minus one (0..2; 3 is reserved)
//! ```
//!
//! # Why this matters more than it looks
//!
//! Read as a plain byte count — which is the obvious reading, and the one this
//! project used until it met real hardware — the numbers are quietly wrong:
//!
//! - `64` means **zero** stations with two-byte hashes, not sixty-four
//!   stations. A contact list full of unreachable nodes came out as a mesh
//!   where everything was 64 hops away.
//! - `255` has `hash_size == 4`, which is reserved, so the value is not a
//!   route at all. The firmware uses exactly this as `OUT_PATH_UNKNOWN`.
//!
//! Neither produces an error. Both produce a plausible-looking journey that
//! never happened, which is what rule 1 in `CLAUDE.md` is about.

/// Largest path in bytes, `MAX_PATH_SIZE` from `src/MeshCore.h`.
pub const MAX_PATH_BYTES: usize = 64;

/// A route, as the length byte describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathShape {
    /// How many stations the packet passed through.
    pub stations: u8,
    /// How many bytes each station takes in the path field.
    pub bytes_per_station: u8,
}

impl PathShape {
    /// How many bytes of the path field this route occupies.
    pub fn byte_len(self) -> usize {
        usize::from(self.stations) * usize::from(self.bytes_per_station)
    }
}

/// Reads a path-length byte.
///
/// `None` means the byte does not describe a route: either the reserved
/// hash size, or a count that would run past the field. `OUT_PATH_UNKNOWN`
/// (`0xFF`) lands here, which is why "no known route" and "unreadable" are
/// the same answer — the firmware makes no distinction either.
pub fn decode(path_len: u8) -> Option<PathShape> {
    let stations = path_len & 0b0011_1111;
    let bytes_per_station = (path_len >> 6) + 1;

    // Four is reserved for a future encoding; the firmware refuses it, and
    // 0xFF — its marker for "no route" — is exactly this case.
    if bytes_per_station == 4 {
        return None;
    }

    let shape = PathShape {
        stations,
        bytes_per_station,
    };

    // A route that would not fit the field cannot be read: the bytes for the
    // later stations are not there.
    (shape.byte_len() <= MAX_PATH_BYTES).then_some(shape)
}

/// Turns a route shape back into its length byte.
///
/// `None` when the shape cannot be expressed: more than 63 stations, or hashes
/// wider than three bytes, or a route that would not fit the field.
pub fn encode(shape: PathShape) -> Option<u8> {
    if shape.stations > 63 || shape.bytes_per_station == 0 || shape.bytes_per_station > 3 {
        return None;
    }

    if shape.byte_len() > MAX_PATH_BYTES {
        return None;
    }

    Some(((shape.bytes_per_station - 1) << 6) | shape.stations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_and_decoding_are_inverse() {
        // Every byte that decodes must encode back to itself, or writing a
        // contact back to the node would change its route.
        for byte in 0..=u8::MAX {
            if let Some(shape) = decode(byte) {
                assert_eq!(encode(shape), Some(byte), "round trip failed for {byte}");
            }
        }
    }

    #[test]
    fn refuses_to_encode_a_shape_that_has_no_byte() {
        assert_eq!(
            encode(PathShape {
                stations: 64,
                bytes_per_station: 1
            }),
            None
        );
        assert_eq!(
            encode(PathShape {
                stations: 1,
                bytes_per_station: 4
            }),
            None
        );
    }

    #[test]
    fn a_plain_count_is_that_many_one_byte_stations() {
        assert_eq!(
            decode(3),
            Some(PathShape {
                stations: 3,
                bytes_per_station: 1
            })
        );
        assert_eq!(decode(3).unwrap().byte_len(), 3);
    }

    #[test]
    fn zero_is_a_direct_route_not_a_missing_one() {
        // Reachable with nothing in between. Different from "no route known",
        // which decode() reports as None.
        assert_eq!(decode(0).unwrap().stations, 0);
        assert_eq!(decode(0).unwrap().byte_len(), 0);
    }

    #[test]
    fn the_top_bits_widen_each_station() {
        // 0x41 = 0b01_000001: one station, two bytes for it.
        let shape = decode(0x41).unwrap();

        assert_eq!(shape.stations, 1);
        assert_eq!(shape.bytes_per_station, 2);
        assert_eq!(shape.byte_len(), 2);
    }

    #[test]
    fn sixty_four_means_no_stations_at_all() {
        // This is the value that made a contact list look like a mesh where
        // everything sat 64 hops away: 0b01_000000 is zero stations with
        // two-byte hashes, not sixty-four stations.
        let shape = decode(64).unwrap();

        assert_eq!(shape.stations, 0);
        assert_eq!(shape.byte_len(), 0);
    }

    #[test]
    fn the_unknown_marker_is_not_a_route() {
        // 0xFF has hash_size 4, which the firmware reserves — and uses as
        // OUT_PATH_UNKNOWN.
        assert_eq!(decode(0xFF), None);
    }

    #[test]
    fn refuses_the_reserved_hash_size() {
        // Any value with both top bits set, not just 0xFF.
        assert_eq!(decode(0xC0), None);
        assert_eq!(decode(0xC1), None);
    }

    #[test]
    fn refuses_a_route_that_would_run_past_the_field() {
        // 63 stations of two bytes each is 126, well past the 64 available.
        assert_eq!(decode(0x7F), None);
    }

    #[test]
    fn accepts_a_route_that_exactly_fills_the_field() {
        // 0b00_111111 is 63 single-byte stations; 0x80 | 21 is 21 three-byte
        // ones, both inside 64 bytes.
        assert_eq!(decode(63).unwrap().byte_len(), 63);
        assert_eq!(decode(0x80 | 21).unwrap().byte_len(), 63);
    }
}
