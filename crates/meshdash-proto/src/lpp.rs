//! CayenneLPP: how other nodes report their sensors.
//!
//! # Layout
//!
//! Source: `src/helpers/sensors/LPPDataHelpers.h`, MeshCore commit `d929643`,
//! which carries both the type table and `LPPReader` as a reference for
//! reading it. The firmware itself builds these payloads with
//! `electroniccats/CayenneLPP @ 1.6.1` (`platformio.ini`).
//!
//! A payload is a chain of entries:
//!
//! ```text
//! [ channel: u8 ][ type: u8 ][ data: width depends on the type ]
//! ```
//!
//! # Three things that bite
//!
//! **Numbers are big-endian here.** The rest of the MeshCore protocol is
//! little-endian throughout; `getFloat()` in the firmware shifts the other way
//! (`value = (value << 8) + buffer[i]`). Reading them the usual way raises no
//! error — it produces plausible wrong numbers, which is worse.
//!
//! **Channel 0 ends the payload.** It is not a channel. `readHeader()` returns
//! `channel != 0`, so anything after it is padding, not data.
//!
//! **An unknown type stops the reading.** Every type has its own width, so
//! without knowing the type there is no way to tell where the next entry
//! starts. Guessing would silently shift every following value. What was read
//! up to that point is returned, and the stop is reported.

/// Type codes and their widths, from `LPPDataHelpers.h`.
///
/// The multiplier turns the stored integer into the real value: the firmware
/// divides by it, so a temperature of 21.5 °C travels as 215.
mod types {
    /// `(type, width in bytes, multiplier, signed)`
    pub const TABLE: &[(u8, usize, f64, bool)] = &[
        (0, 1, 1.0, false),      // LPP_DIGITAL_INPUT
        (1, 1, 1.0, false),      // LPP_DIGITAL_OUTPUT
        (2, 2, 100.0, true),     // LPP_ANALOG_INPUT
        (3, 2, 100.0, true),     // LPP_ANALOG_OUTPUT
        (100, 4, 1.0, false),    // LPP_GENERIC_SENSOR
        (101, 2, 1.0, false),    // LPP_LUMINOSITY
        (102, 1, 1.0, false),    // LPP_PRESENCE
        (103, 2, 10.0, true),    // LPP_TEMPERATURE
        (104, 1, 2.0, false),    // LPP_RELATIVE_HUMIDITY
        (113, 6, 1000.0, true),  // LPP_ACCELEROMETER, 2 bytes per axis
        (115, 2, 10.0, false),   // LPP_BAROMETRIC_PRESSURE
        (116, 2, 100.0, false),  // LPP_VOLTAGE
        (117, 2, 1000.0, false), // LPP_CURRENT
        (118, 4, 1.0, false),    // LPP_FREQUENCY
        (120, 1, 1.0, false),    // LPP_PERCENTAGE
        (121, 2, 1.0, true),     // LPP_ALTITUDE
        (125, 2, 1.0, false),    // LPP_CONCENTRATION
        (128, 2, 1.0, false),    // LPP_POWER
        (130, 4, 1000.0, false), // LPP_DISTANCE
        (131, 4, 1000.0, false), // LPP_ENERGY
        (132, 2, 1.0, false),    // LPP_DIRECTION
        (133, 4, 1.0, false),    // LPP_UNIXTIME
        (134, 6, 100.0, true),   // LPP_GYROMETER, 2 bytes per axis
        (135, 3, 1.0, false),    // LPP_COLOUR, one byte per channel
        (136, 9, 1.0, true),     // LPP_GPS, handled apart
        (142, 1, 1.0, false),    // LPP_SWITCH
    ];

    /// Position, which needs three separate scales rather than one.
    pub const GPS: u8 = 136;
    /// Channel zero marks the end of the data.
    pub const END_OF_DATA: u8 = 0;
}

/// One measurement out of a payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Which sensor of the reporting node. 1 is the device itself.
    pub channel: u8,
    /// The LPP type code, passed through so an unknown one stays identifiable.
    pub type_code: u8,
    /// The value, already scaled.
    pub value: Value,
}

/// What a reading carries.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A single scaled number.
    Number(f64),
    /// Three axes, for accelerometer and gyrometer.
    Axes {
        /// First axis.
        x: f64,
        /// Second axis.
        y: f64,
        /// Third axis.
        z: f64,
    },
    /// A position. Altitude is in metres.
    Position {
        /// Degrees.
        latitude: f64,
        /// Degrees.
        longitude: f64,
        /// Metres.
        altitude: f64,
    },
}

/// What came out of a payload, and whether all of it could be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Everything that was read, in order.
    pub readings: Vec<Reading>,
    /// Why reading stopped early, or `None` if the payload was read whole.
    pub stopped: Option<StoppedBecause>,
}

/// Why a payload could not be read to its end.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoppedBecause {
    /// A type this table does not know. Its width is unknown, so the rest of
    /// the payload cannot be located.
    #[error("unknown measurement type {type_code} at byte {at}")]
    UnknownType {
        /// The code that was not recognised.
        type_code: u8,
        /// Where it sat.
        at: usize,
    },

    /// The payload ends inside an entry.
    #[error("payload ends inside a measurement at byte {at}")]
    Truncated {
        /// Where the incomplete entry began.
        at: usize,
    },
}

/// Reads a CayenneLPP payload.
pub fn decode(payload: &[u8]) -> Decoded {
    let mut readings = Vec::new();
    let mut at = 0;

    loop {
        // Header is channel and type; anything shorter is the end.
        if at + 2 > payload.len() {
            return Decoded {
                readings,
                stopped: (at != payload.len()).then_some(StoppedBecause::Truncated { at }),
            };
        }

        let channel = payload[at];
        if channel == types::END_OF_DATA {
            // Not a channel — the marker for "nothing follows". Whatever comes
            // after is padding.
            return Decoded {
                readings,
                stopped: None,
            };
        }

        let type_code = payload[at + 1];
        let Some(&(_, width, multiplier, signed)) =
            types::TABLE.iter().find(|(code, ..)| *code == type_code)
        else {
            // Without the width there is no way to find the next entry.
            return Decoded {
                readings,
                stopped: Some(StoppedBecause::UnknownType { type_code, at }),
            };
        };

        let body = at + 2;
        if body + width > payload.len() {
            return Decoded {
                readings,
                stopped: Some(StoppedBecause::Truncated { at }),
            };
        }

        let bytes = &payload[body..body + width];
        let value = match type_code {
            types::GPS => Value::Position {
                // Three scales, not one: the firmware reads latitude and
                // longitude at 1/10000 and altitude at 1/100.
                latitude: scaled(&bytes[0..3], 10_000.0, true),
                longitude: scaled(&bytes[3..6], 10_000.0, true),
                altitude: scaled(&bytes[6..9], 100.0, true),
            },
            _ if width == 6 => Value::Axes {
                x: scaled(&bytes[0..2], multiplier, signed),
                y: scaled(&bytes[2..4], multiplier, signed),
                z: scaled(&bytes[4..6], multiplier, signed),
            },
            _ => Value::Number(scaled(bytes, multiplier, signed)),
        };

        readings.push(Reading {
            channel,
            type_code,
            value,
        });
        at = body + width;
    }
}

/// Reads one big-endian integer and scales it.
///
/// Mirrors `getFloat()` in `LPPDataHelpers.h`: bytes go most significant
/// first — the opposite of everywhere else in this protocol.
fn scaled(bytes: &[u8], multiplier: f64, signed: bool) -> f64 {
    let mut value: u64 = 0;
    for &byte in bytes {
        value = (value << 8) + u64::from(byte);
    }

    if signed {
        // Two's complement over the width actually present, which is three
        // bytes for a coordinate and two for most everything else.
        let sign_bit = 1u64 << (bytes.len() * 8 - 1);
        if value & sign_bit == sign_bit {
            let magnitude = (sign_bit << 1) - value;
            return -(magnitude as f64) / multiplier;
        }
    }

    value as f64 / multiplier
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Voltage on the device's own channel: 4.02 V travels as 402.
    fn voltage() -> Vec<u8> {
        vec![1, 116, 0x01, 0x92]
    }

    #[test]
    fn reads_a_single_measurement() {
        let decoded = decode(&voltage());

        assert_eq!(decoded.stopped, None);
        assert_eq!(
            decoded.readings,
            vec![Reading {
                channel: 1,
                type_code: 116,
                value: Value::Number(4.02),
            }]
        );
    }

    #[test]
    fn reads_numbers_big_endian() {
        // The rest of this protocol is little-endian. Reading these two bytes
        // the usual way would give 37377 instead of 402 — no error, just a
        // wrong number, which is why this test exists.
        let Value::Number(volts) = decode(&voltage()).readings[0].value.clone() else {
            panic!("expected a number");
        };

        assert!((volts - 4.02).abs() < 1e-9, "got {volts}");
    }

    #[test]
    fn scales_by_the_multiplier_of_each_type() {
        // Temperature is tenths, humidity is halves.
        let payload = vec![1, 103, 0x00, 0xD7, 1, 104, 0x5A];

        let decoded = decode(&payload);

        assert_eq!(decoded.readings[0].value, Value::Number(21.5));
        assert_eq!(decoded.readings[1].value, Value::Number(45.0));
    }

    #[test]
    fn reads_negative_values_where_the_type_is_signed() {
        // −3.5 °C travels as −35, two's complement.
        let payload = vec![1, 103, 0xFF, 0xDD];

        assert_eq!(decode(&payload).readings[0].value, Value::Number(-3.5));
    }

    #[test]
    fn keeps_an_unsigned_type_unsigned() {
        // The same bytes under an unsigned type are a large positive number,
        // not a negative one: 0xFFDD is 65501, divided by 100.
        let payload = vec![1, 116, 0xFF, 0xDD];

        assert_eq!(decode(&payload).readings[0].value, Value::Number(655.01));
    }

    #[test]
    fn reads_several_channels_in_one_payload() {
        let mut payload = voltage();
        payload.extend_from_slice(&[2, 103, 0x00, 0xD7]);

        let decoded = decode(&payload);

        assert_eq!(decoded.readings.len(), 2);
        assert_eq!(decoded.readings[1].channel, 2);
    }

    #[test]
    fn stops_at_channel_zero() {
        // Channel 0 is the end marker, not a channel. What follows is padding.
        let mut payload = voltage();
        payload.extend_from_slice(&[0, 103, 0xFF, 0xFF, 0xFF]);

        let decoded = decode(&payload);

        assert_eq!(decoded.readings.len(), 1);
        assert_eq!(
            decoded.stopped, None,
            "reaching the end marker is not a failure"
        );
    }

    #[test]
    fn reads_a_position_with_its_three_scales() {
        // Latitude and longitude are 3 bytes each divided by 10000, altitude
        // 3 bytes divided by 100.
        let mut payload = vec![1, 136];
        payload.extend_from_slice(&[0x08, 0x05, 0x28]); // 525_608 → 52.5608
        payload.extend_from_slice(&[0x02, 0x07, 0x0E]); // 132_878 → 13.2878
        payload.extend_from_slice(&[0x00, 0x0B, 0xB8]); // 3_000 → 30.00 m

        let Value::Position {
            latitude,
            longitude,
            altitude,
        } = decode(&payload).readings[0].value.clone()
        else {
            panic!("expected a position");
        };

        assert!((latitude - 52.5608).abs() < 1e-6, "got {latitude}");
        assert!((longitude - 13.2878).abs() < 1e-6, "got {longitude}");
        assert!((altitude - 30.0).abs() < 1e-6, "got {altitude}");
    }

    #[test]
    fn reads_three_axes_where_the_type_has_them() {
        let payload = vec![1, 113, 0x00, 0x64, 0xFF, 0x9C, 0x03, 0xE8];

        let Value::Axes { x, y, z } = decode(&payload).readings[0].value.clone() else {
            panic!("expected axes");
        };

        assert!((x - 0.1).abs() < 1e-9);
        assert!((y + 0.1).abs() < 1e-9, "the middle axis is negative");
        assert!((z - 1.0).abs() < 1e-9);
    }

    #[test]
    fn stops_at_a_type_it_does_not_know() {
        // Widths differ per type, so an unknown one makes everything after it
        // unlocatable. Guessing would shift every following value silently.
        let mut payload = voltage();
        payload.extend_from_slice(&[2, 250, 0x01, 0x02]);
        payload.extend_from_slice(&[3, 103, 0x00, 0xD7]);

        let decoded = decode(&payload);

        assert_eq!(decoded.readings.len(), 1, "keeps what was read before");
        assert_eq!(
            decoded.stopped,
            Some(StoppedBecause::UnknownType {
                type_code: 250,
                at: 4
            })
        );
    }

    #[test]
    fn reports_a_payload_that_ends_mid_measurement() {
        let payload = vec![1, 116, 0x01];

        assert_eq!(
            decode(&payload).stopped,
            Some(StoppedBecause::Truncated { at: 0 })
        );
    }

    #[test]
    fn an_empty_payload_yields_nothing_and_no_complaint() {
        assert_eq!(
            decode(&[]),
            Decoded {
                readings: Vec::new(),
                stopped: None
            }
        );
    }
}
