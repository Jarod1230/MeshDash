//! Adverts: a node announcing itself over the air.
//!
//! # Layout
//!
//! Source: `onDiscoveredContact()` in `examples/companion_radio/MyMesh.cpp`,
//! MeshCore commit `d929643`.
//!
//! Two pushes report a sighting, and they carry very different amounts of it:
//!
//! ```text
//! 0x8A  PUSH_CODE_NEW_ADVERT   a contact the node did not have yet
//! 0x80  PUSH_CODE_ADVERT       a contact already in the node's list
//! ```
//!
//! For `PUSH_CODE_NEW_ADVERT` the firmware calls `writeContactRespFrame()` —
//! the very function that produces `RESP_CODE_CONTACT`. The payload is a full
//! contact, byte for byte, and [`crate::contact`] already knows how to read it.
//!
//! `PUSH_CODE_ADVERT` carries the public key and nothing else:
//!
//! ```text
//! offset  size  field
//!      0     1  opcode
//!      1    32  public key
//! ```
//!
//! # The short form is not a lesser contact
//!
//! A `PUSH_CODE_ADVERT` says "this key was heard just now" and no more. Name,
//! type, path and position are absent because the node assumes the client
//! already has them from the contact listing — not because they changed to
//! empty. Writing empty fields on this push would erase what the listing
//! delivered.

use crate::{contact::Contact, contact::ContactError, opcode::Push};

/// Offsets of the short form.
mod layout {
    pub const PUB_KEY: usize = 1;
    pub const PUB_KEY_SIZE: usize = 32;
    /// Total length of a short advert frame.
    pub const LEN: usize = 33;
}

/// A node was heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advert {
    /// A contact the node already knew. Only its public key travels.
    Known {
        /// Ed25519 public key of whoever advertised.
        public_key: [u8; 32],
    },

    /// A contact the node did not have yet, delivered in full.
    New(Box<Contact>),
}

/// Why an advert payload could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdvertError {
    /// The payload is not an advert.
    #[error("expected an advert, got opcode {opcode:#04x}")]
    WrongOpcode {
        /// What the first byte was.
        opcode: u8,
    },

    /// The short form is shorter than a public key.
    #[error("advert payload is {len} bytes, need {needed}")]
    TooShort {
        /// What arrived.
        len: usize,
        /// What is required.
        needed: usize,
    },

    /// The long form did not hold a readable contact.
    #[error("advert carries a contact that could not be read: {0}")]
    Contact(#[from] ContactError),
}

impl Advert {
    /// Reads an advert payload, opcode byte included.
    pub fn parse(payload: &[u8]) -> Result<Self, AdvertError> {
        let Some(&opcode) = payload.first() else {
            return Err(AdvertError::TooShort {
                len: 0,
                needed: layout::LEN,
            });
        };

        match Push::from(opcode) {
            // Same bytes as a contact frame, only the opcode differs.
            Push::NewAdvert => Ok(Self::New(Box::new(Contact::parse_body(payload)?))),
            Push::Advert => {
                if payload.len() < layout::LEN {
                    return Err(AdvertError::TooShort {
                        len: payload.len(),
                        needed: layout::LEN,
                    });
                }

                let mut public_key = [0u8; layout::PUB_KEY_SIZE];
                public_key.copy_from_slice(
                    &payload[layout::PUB_KEY..layout::PUB_KEY + layout::PUB_KEY_SIZE],
                );

                Ok(Self::Known { public_key })
            }
            _ => Err(AdvertError::WrongOpcode { opcode }),
        }
    }

    /// The public key of whoever advertised, whichever form arrived.
    pub fn public_key(&self) -> &[u8; 32] {
        match self {
            Self::Known { public_key } => public_key,
            Self::New(contact) => &contact.public_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a long-form advert: the contact layout under a push opcode.
    ///
    /// Assembled from the source-verified layout, not captured from hardware.
    fn new_advert_payload() -> Vec<u8> {
        // 148 bytes, the contact frame length.
        let mut payload = vec![0u8; 148];
        payload[0] = u8::from(Push::NewAdvert);
        payload[1..33].copy_from_slice(&[0xCD; 32]);
        payload[33] = 2; // type
        payload[34] = 1; // flags
        payload[35] = 2; // path length
        payload[36..40].copy_from_slice(&[7, 8, 9, 9]);
        payload[100..108].copy_from_slice(b"Nachbar1");
        payload[132..136].copy_from_slice(&1_700_000_000_u32.to_le_bytes());
        payload
    }

    /// Builds a short-form advert: opcode and public key, nothing else.
    fn known_advert_payload() -> Vec<u8> {
        let mut payload = vec![0u8; layout::LEN];
        payload[0] = u8::from(Push::Advert);
        payload[layout::PUB_KEY..layout::PUB_KEY + layout::PUB_KEY_SIZE]
            .copy_from_slice(&[0xEF; 32]);
        payload
    }

    #[test]
    fn reads_a_new_advert_as_a_full_contact() {
        let Ok(Advert::New(contact)) = Advert::parse(&new_advert_payload()) else {
            panic!("expected a new advert");
        };

        assert_eq!(contact.public_key, [0xCD; 32]);
        assert_eq!(contact.name, "Nachbar1");
        assert_eq!(contact.contact_type, 2);
        assert_eq!(contact.last_advert, 1_700_000_000);
    }

    #[test]
    fn cuts_the_path_of_a_new_advert_to_its_used_length() {
        let Ok(Advert::New(contact)) = Advert::parse(&new_advert_payload()) else {
            panic!("expected a new advert");
        };

        assert_eq!(
            contact.path,
            Some(crate::contact::Route {
                stations: 2,
                hops: vec![7, 8]
            })
        );
    }

    #[test]
    fn reads_a_known_advert_as_a_bare_key() {
        assert_eq!(
            Advert::parse(&known_advert_payload()),
            Ok(Advert::Known {
                public_key: [0xEF; 32]
            })
        );
    }

    #[test]
    fn reports_the_key_of_either_form() {
        assert_eq!(
            Advert::parse(&known_advert_payload()).unwrap().public_key(),
            &[0xEF; 32]
        );
        assert_eq!(
            Advert::parse(&new_advert_payload()).unwrap().public_key(),
            &[0xCD; 32]
        );
    }

    #[test]
    fn ignores_bytes_past_a_known_advert() {
        // Should the firmware ever append to this push, the key still reads.
        let mut payload = known_advert_payload();
        payload.extend_from_slice(&[0xFF; 4]);

        assert_eq!(
            Advert::parse(&payload),
            Ok(Advert::Known {
                public_key: [0xEF; 32]
            })
        );
    }

    #[test]
    fn refuses_a_known_advert_without_a_full_key() {
        let payload = &known_advert_payload()[..20];

        assert_eq!(
            Advert::parse(payload),
            Err(AdvertError::TooShort {
                len: 20,
                needed: layout::LEN
            })
        );
    }

    #[test]
    fn passes_on_why_a_new_advert_could_not_be_read() {
        let payload = &new_advert_payload()[..100];

        assert!(matches!(
            Advert::parse(payload),
            Err(AdvertError::Contact(ContactError::TooShort { .. }))
        ));
    }

    #[test]
    fn refuses_a_frame_that_is_not_an_advert() {
        assert_eq!(
            Advert::parse(&[0x83, 0x00]),
            Err(AdvertError::WrongOpcode { opcode: 0x83 })
        );
    }

    #[test]
    fn refuses_an_empty_frame() {
        assert_eq!(
            Advert::parse(&[]),
            Err(AdvertError::TooShort {
                len: 0,
                needed: layout::LEN
            })
        );
    }
}
