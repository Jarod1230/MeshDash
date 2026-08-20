//! The smaller answers a node gives about itself.
//!
//! # Layout
//!
//! Source: `handleCmdFrame()` in `examples/companion_radio/MyMesh.cpp`,
//! MeshCore commit `d929643`. The clock and the tuning parameters were
//! additionally confirmed against a Xiao S3 WIO on firmware v1.17.0.
//!
//! ```text
//! RESP_CODE_CURR_TIME (9), 5 bytes
//!   1   4  seconds since the epoch (u32 little-endian)
//!
//! RESP_CODE_TUNING_PARAMS (23), 9 bytes
//!   1   4  receive delay in milliseconds (u32)
//!   5   4  advert flood delay in milliseconds (u32)
//!
//! RESP_CODE_ADVERT_PATH (22)
//!   1   4  when the advert was heard (u32)
//!   5   1  route length byte — see crate::path
//!   6   …  the route itself
//!
//! RESP_CODE_CUSTOM_VARS (21)
//!   1   …  "name:value,name:value", to the end of the frame
//!
//! RESP_CODE_AUTOADD_CONFIG (25), 3 bytes
//!   1   1  configuration flags, passed through unread
//!   2   1  how many hops away a contact may be to be added
//! ```
//!
//! # Two answers are deliberately not read here
//!
//! `RESP_CODE_PRIVATE_KEY` carries the node's **private** key — the thing that
//! *is* its identity. MeshDash has no reason to ever hold it, so there is no
//! parser for it: what cannot be read cannot be logged, stored or backed up by
//! accident.
//!
//! `RESP_CODE_DEFAULT_FLOOD_SCOPE` carries a scope name followed by a 16-byte
//! key. Only the name is read, for the same reason the channel key is skipped
//! in [`crate::channel`].

use crate::{opcode::Response, path};

/// Reads a length-checked answer that carries one number.
macro_rules! simple {
    ($name:ident, $opcode:ident, $len:expr) => {
        fn $name(payload: &[u8]) -> Result<(), ResponseError> {
            check(payload, Response::$opcode, $len)
        }
    };
}

/// Why an answer could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseError {
    /// The payload is not the expected answer.
    #[error("expected {expected:?}, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What was expected.
        expected: Response,
        /// What the first byte was.
        opcode: u8,
    },

    /// The payload ends before its fields do.
    #[error("answer is {len} bytes, need at least {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },
}

fn check(payload: &[u8], expected: Response, needed: usize) -> Result<(), ResponseError> {
    match payload.first().map(|&byte| Response::from(byte)) {
        Some(found) if found == expected => {}
        Some(_) => {
            return Err(ResponseError::WrongOpcode {
                expected,
                opcode: payload[0],
            });
        }
        None => return Err(ResponseError::TooShort { len: 0, needed }),
    }

    if payload.len() < needed {
        return Err(ResponseError::TooShort {
            len: payload.len(),
            needed,
        });
    }

    Ok(())
}

simple!(check_time, CurrTime, 5);
simple!(check_tuning, TuningParams, 9);
simple!(check_autoadd, AutoAddConfig, 3);

fn read_u32(payload: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([
        payload[at],
        payload[at + 1],
        payload[at + 2],
        payload[at + 3],
    ])
}

/// Reads `RESP_CODE_CURR_TIME`: the node's clock, in seconds since the epoch.
pub fn current_time(payload: &[u8]) -> Result<u32, ResponseError> {
    check_time(payload)?;
    Ok(read_u32(payload, 1))
}

/// How long the node waits, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuningParams {
    /// Delay before receiving, in milliseconds.
    pub receive_delay_ms: u32,
    /// Delay before flooding an advert onward, in milliseconds.
    pub advert_flood_delay_ms: u32,
}

impl TuningParams {
    /// Reads `RESP_CODE_TUNING_PARAMS`.
    pub fn parse(payload: &[u8]) -> Result<Self, ResponseError> {
        check_tuning(payload)?;

        Ok(Self {
            receive_delay_ms: read_u32(payload, 1),
            advert_flood_delay_ms: read_u32(payload, 5),
        })
    }
}

/// When a contact was last heard, and over which route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertPath {
    /// When the node heard the advert, in seconds since the epoch.
    pub heard_at: u32,
    /// How many stations the route passes through, or `None` if unknown.
    pub stations: Option<u8>,
    /// The route's hop bytes.
    pub hops: Vec<u8>,
}

impl AdvertPath {
    /// Reads `RESP_CODE_ADVERT_PATH`.
    pub fn parse(payload: &[u8]) -> Result<Self, ResponseError> {
        check(payload, Response::AdvertPath, 6)?;

        // Same encoding as everywhere else: the byte is not a byte count.
        let shape = path::decode(payload[5]);
        let byte_len = shape.map_or(0, |shape| shape.byte_len());
        let end = (6 + byte_len).min(payload.len());

        Ok(Self {
            heard_at: read_u32(payload, 1),
            stations: shape.map(|shape| shape.stations),
            hops: payload[6..end].to_vec(),
        })
    }
}

/// How the node decides which contacts to add on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoAddConfig {
    /// Firmware flags, passed through unread: their meaning is not verified.
    pub flags: u8,
    /// How many stations away a contact may be and still be added.
    pub max_stations: u8,
}

impl AutoAddConfig {
    /// Reads `RESP_CODE_AUTOADD_CONFIG`.
    pub fn parse(payload: &[u8]) -> Result<Self, ResponseError> {
        check_autoadd(payload)?;

        Ok(Self {
            flags: payload[1],
            max_stations: payload[2],
        })
    }
}

/// Reads `RESP_CODE_CUSTOM_VARS` into name and value pairs.
///
/// The firmware writes them as one string, `name:value` separated by commas,
/// and cuts the whole thing at 140 bytes. A pair without a colon is skipped
/// rather than guessed at — a truncated last entry is the ordinary case.
pub fn custom_vars(payload: &[u8]) -> Result<Vec<(String, String)>, ResponseError> {
    check(payload, Response::CustomVars, 1)?;

    let text = String::from_utf8_lossy(&payload[1..]);
    Ok(text
        .split(',')
        .filter_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect())
}

/// Reads the name of the default flood scope, if one is set.
///
/// The key that follows it is deliberately not read; see the note at the top
/// of this module.
pub fn default_flood_scope_name(payload: &[u8]) -> Result<Option<String>, ResponseError> {
    check(payload, Response::DefaultFloodScope, 1)?;

    // A frame of just the opcode means no scope is set at all.
    if payload.len() < 1 + 31 {
        return Ok(None);
    }

    let raw = &payload[1..32];
    let end = raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len());
    let name = String::from_utf8_lossy(&raw[..end]).into_owned();

    Ok((!name.is_empty()).then_some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_clock_a_real_node_reported() {
        // Answered by a Xiao S3 WIO over USB on 2026-08-20.
        let payload = [0x09, 0x19, 0x12, 0x87, 0x6a];

        assert_eq!(current_time(&payload).unwrap(), 0x6a87_1219);
    }

    #[test]
    fn reads_the_tuning_parameters_a_real_node_reported() {
        // Also from the same node: no receive delay, one second before an
        // advert is flooded onward.
        let payload = [0x17, 0, 0, 0, 0, 0xe8, 0x03, 0, 0];

        assert_eq!(
            TuningParams::parse(&payload).unwrap(),
            TuningParams {
                receive_delay_ms: 0,
                advert_flood_delay_ms: 1_000,
            }
        );
    }

    #[test]
    fn reads_a_route_to_a_contact() {
        let mut payload = vec![u8::from(Response::AdvertPath)];
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload.push(2); // two stations, one byte each
        payload.extend_from_slice(&[0x11, 0x22]);

        assert_eq!(
            AdvertPath::parse(&payload).unwrap(),
            AdvertPath {
                heard_at: 1_700_000_000,
                stations: Some(2),
                hops: vec![0x11, 0x22],
            }
        );
    }

    #[test]
    fn reports_an_unusable_route_as_unknown() {
        // 0xFF is the marker for "no route", not a length — the same trap
        // that produced 64-hop journeys in the contact list.
        let mut payload = vec![u8::from(Response::AdvertPath)];
        payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload.push(0xFF);

        let path = AdvertPath::parse(&payload).unwrap();

        assert_eq!(path.stations, None);
        assert!(path.hops.is_empty());
    }

    #[test]
    fn reads_custom_variables_as_pairs() {
        let mut payload = vec![u8::from(Response::CustomVars)];
        payload.extend_from_slice(b"temp:21.5,name:Bake Sued");

        assert_eq!(
            custom_vars(&payload).unwrap(),
            vec![
                ("temp".to_owned(), "21.5".to_owned()),
                ("name".to_owned(), "Bake Sued".to_owned()),
            ]
        );
    }

    #[test]
    fn skips_a_pair_the_firmware_cut_in_half() {
        // The firmware stops writing at 140 bytes, wherever that lands.
        let mut payload = vec![u8::from(Response::CustomVars)];
        payload.extend_from_slice(b"temp:21.5,halb");

        assert_eq!(custom_vars(&payload).unwrap().len(), 1);
    }

    #[test]
    fn an_empty_custom_vars_answer_is_no_variables() {
        // A real node with no sensors answers with the bare opcode.
        assert_eq!(
            custom_vars(&[u8::from(Response::CustomVars)]).unwrap(),
            vec![]
        );
    }

    #[test]
    fn reads_the_auto_add_configuration() {
        let payload = [u8::from(Response::AutoAddConfig), 0x03, 2];

        assert_eq!(
            AutoAddConfig::parse(&payload).unwrap(),
            AutoAddConfig {
                flags: 0x03,
                max_stations: 2
            }
        );
    }

    #[test]
    fn a_bare_flood_scope_answer_means_none_is_set() {
        assert_eq!(
            default_flood_scope_name(&[u8::from(Response::DefaultFloodScope)]).unwrap(),
            None
        );
    }

    #[test]
    fn reads_a_flood_scope_name_and_leaves_its_key_alone() {
        let mut payload = vec![u8::from(Response::DefaultFloodScope)];
        let mut name = [0u8; 31];
        name[..7].copy_from_slice(b"Notfunk");
        payload.extend_from_slice(&name);
        // The key follows here and is never read.
        payload.extend_from_slice(&[0x99; 16]);

        assert_eq!(
            default_flood_scope_name(&payload).unwrap(),
            Some("Notfunk".to_owned())
        );
    }

    #[test]
    fn names_what_it_expected_when_the_opcode_is_wrong() {
        assert_eq!(
            current_time(&[u8::from(Response::Ok), 0, 0, 0, 0]),
            Err(ResponseError::WrongOpcode {
                expected: Response::CurrTime,
                opcode: 0
            })
        );
    }

    #[test]
    fn refuses_an_answer_that_ends_early() {
        assert_eq!(
            TuningParams::parse(&[u8::from(Response::TuningParams), 0, 0]),
            Err(ResponseError::TooShort { len: 3, needed: 9 })
        );
    }
}
