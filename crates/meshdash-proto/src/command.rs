//! Building the commands a client sends to its own node.
//!
//! # Layout
//!
//! Source: the branches of `handleCmdFrame()` in
//! `examples/companion_radio/MyMesh.cpp`, MeshCore commit `d929643`. Each
//! function below cites the length check the firmware applies, because that
//! check *is* the contract: a frame the branch does not accept falls through
//! the whole chain and comes back as "unsupported command", which reads like a
//! protocol error rather than a bad argument.
//!
//! # Not every command lives here
//!
//! Two of them sit with the answer they belong to, because the request and
//! the reply are one subject: [`crate::battery::battery_query`] and
//! [`crate::device::device_query`], the latter of which carries the protocol
//! version MeshDash announces. Splitting a request from its reply to satisfy
//! a filename would help nobody.
//!
//! # These build frames, they do not send them
//!
//! Everything here is pure. What a module may actually do with a node — and
//! what it must ask the operator first — is decided where the link lives, not
//! here.
//!
//! # Two of these destroy things
//!
//! [`reboot`] interrupts the node, and [`factory_reset`] erases its
//! filesystem: contacts, channels and identity. The firmware guards both with
//! a magic word, which is a hint about how careless a caller can be, not a
//! safety net. Nothing in MeshDash calls them today.

use crate::opcode::Command;

/// Longest node name the firmware keeps.
///
/// `sizeof(_prefs.node_name) - 1` in the `CMD_SET_ADVERT_NAME` branch; the
/// firmware truncates silently past this, so a caller that means the whole
/// name has to know.
pub const MAX_NODE_NAME: usize = 31;

/// Why a command could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// A text argument is longer than the node accepts.
    #[error("{what} is {len} bytes, the node keeps at most {allowed}")]
    TooLong {
        /// Which argument.
        what: &'static str,
        /// How long it was.
        len: usize,
        /// How long it may be.
        allowed: usize,
    },

    /// A text argument is empty where the firmware needs at least one byte.
    #[error("{what} must not be empty")]
    Empty {
        /// Which argument.
        what: &'static str,
    },

    /// A coordinate lies outside what the firmware accepts.
    #[error("{what} of {value} micro-degrees is outside ±{limit}")]
    OutOfRange {
        /// Which argument.
        what: &'static str,
        /// What was given.
        value: i32,
        /// The bound the firmware checks.
        limit: i32,
    },
}

/// Asks the node to announce itself.
///
/// `flood` chooses how far: flooded through the mesh, or one hop only. The
/// firmware reads the flag only when it is there — `len >= 2 && cmd_frame[1]
/// == 1` — so an omitted byte means zero-hop.
pub fn send_self_advert(flood: bool) -> Vec<u8> {
    vec![u8::from(Command::SendSelfAdvert), u8::from(flood)]
}

/// Renames the node. `CMD_SET_ADVERT_NAME`, `len >= 2`.
pub fn set_advert_name(name: &str) -> Result<Vec<u8>, CommandError> {
    check_text(name, "the node name", MAX_NODE_NAME)?;

    let mut frame = Vec::with_capacity(1 + name.len());
    frame.push(u8::from(Command::SetAdvertName));
    frame.extend_from_slice(name.as_bytes());

    Ok(frame)
}

/// Sets the position the node advertises. `CMD_SET_ADVERT_LATLON`, `len >= 9`.
///
/// Coordinates are micro-degrees, as everywhere in this protocol. The firmware
/// refuses anything past ±90e6 / ±180e6 with `ERR_CODE_ILLEGAL_ARG`; this
/// checks the same bounds so the caller learns why rather than getting a bare
/// error code back.
pub fn set_advert_position(
    latitude_micro: i32,
    longitude_micro: i32,
) -> Result<Vec<u8>, CommandError> {
    /// The firmware's own bounds, in micro-degrees.
    const LATITUDE_LIMIT: i32 = 90_000_000;
    const LONGITUDE_LIMIT: i32 = 180_000_000;

    check_range(latitude_micro, "the latitude", LATITUDE_LIMIT)?;
    check_range(longitude_micro, "the longitude", LONGITUDE_LIMIT)?;

    let mut frame = Vec::with_capacity(9);
    frame.push(u8::from(Command::SetAdvertLatLon));
    frame.extend_from_slice(&latitude_micro.to_le_bytes());
    frame.extend_from_slice(&longitude_micro.to_le_bytes());
    // Altitude would follow here; the firmware reads it only from len >= 13
    // and marks it "for FUTURE support", so it is left off.

    Ok(frame)
}

/// Sets the node's clock. `CMD_SET_DEVICE_TIME`, `len >= 5`.
///
/// The firmware refuses to go backwards: a value below its current time comes
/// back as `ERR_CODE_ILLEGAL_ARG`.
pub fn set_device_time(unix_seconds: u32) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5);
    frame.push(u8::from(Command::SetDeviceTime));
    frame.extend_from_slice(&unix_seconds.to_le_bytes());
    frame
}

/// Asks for the node's clock. `CMD_GET_DEVICE_TIME`, no payload.
pub fn get_device_time() -> Vec<u8> {
    vec![u8::from(Command::GetDeviceTime)]
}

/// Forgets the known route to a contact, so the next packet floods again.
pub fn reset_path(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::ResetPath, public_key)
}

/// Deletes a contact from the node.
pub fn remove_contact(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::RemoveContact, public_key)
}

/// Broadcasts a contact to neighbours, one hop only.
pub fn share_contact(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::ShareContact, public_key)
}

/// Asks for one contact by its full key. Answered by `RESP_CODE_CONTACT`.
pub fn get_contact_by_key(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::GetContactByKey, public_key)
}

/// Asks whether a login session with this contact is still open.
pub fn has_connection(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::HasConnection, public_key)
}

/// Ends a login session with this contact.
pub fn logout(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::Logout, public_key)
}

/// Logs in to a repeater or room server.
///
/// # The password travels in the clear through this process
///
/// It goes into the frame as plain text and over the wire to the node. Nothing
/// here may log it, and whatever calls this must not either — see
/// `SECURITY.md`.
pub fn send_login(public_key: &[u8; 32], password: &str) -> Result<Vec<u8>, CommandError> {
    // The frame has to fit; the firmware writes a null terminator at `len`,
    // so the text itself may fill the rest.
    let allowed = crate::frame::MAX_FRAME_SIZE - 1 - 32 - 1;
    if password.len() > allowed {
        return Err(CommandError::TooLong {
            what: "the password",
            len: password.len(),
            allowed,
        });
    }

    let mut frame = Vec::with_capacity(33 + password.len());
    frame.push(u8::from(Command::SendLogin));
    frame.extend_from_slice(public_key);
    frame.extend_from_slice(password.as_bytes());

    Ok(frame)
}

/// Asks a contact for its status. Answered later by `PUSH_CODE_STATUS_RESPONSE`.
pub fn send_status_request(public_key: &[u8; 32]) -> Vec<u8> {
    with_key(Command::SendStatusReq, public_key)
}

/// Asks the node which route it last heard from a contact.
///
/// `CMD_GET_ADVERT_PATH`, `len >= PUB_KEY_SIZE + 2` — the byte after the
/// opcode is reserved and sent as zero.
pub fn get_advert_path(public_key: &[u8; 32]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(34);
    frame.push(u8::from(Command::GetAdvertPath));
    frame.push(0); // reserved
    frame.extend_from_slice(public_key);
    frame
}

/// Asks the node for statistics of the given kind.
pub fn get_stats(kind: u8) -> Vec<u8> {
    vec![u8::from(Command::GetStats), kind]
}

/// Announces the client to the node. `CMD_APP_START`, `len >= 8`.
///
/// ```text
/// offset  size  field
///      0     1  opcode
///      1     7  reserved — the firmware's comment says "reserved future"
///      8     …  application name, to the end of the frame
/// ```
///
/// This is the node's idea of a session start: it logs the name, resets any
/// half-finished contact listing, and answers with `RESP_CODE_SELF_INFO` —
/// the node's own key, position and transmit power, which is not obtainable
/// any other way.
///
/// **A frame shorter than eight bytes is silently ignored.** The branch simply
/// does not match, so a bare opcode gets no answer and no error.
pub fn app_start(app_name: &str) -> Result<Vec<u8>, CommandError> {
    let allowed = crate::frame::MAX_FRAME_SIZE - 8;
    if app_name.len() > allowed {
        return Err(CommandError::TooLong {
            what: "the application name",
            len: app_name.len(),
            allowed,
        });
    }

    let mut frame = Vec::with_capacity(8 + app_name.len());
    frame.push(u8::from(Command::AppStart));
    frame.extend_from_slice(&[0; 7]);
    frame.extend_from_slice(app_name.as_bytes());

    Ok(frame)
}

/// Asks for the whole contact list.
///
/// Answered by `RESP_CODE_CONTACTS_START`, then one `RESP_CODE_CONTACT` per
/// contact, then `RESP_CODE_END_OF_CONTACTS`.
pub fn get_contacts() -> Vec<u8> {
    vec![u8::from(Command::GetContacts)]
}

/// Takes the next waiting message off the node's queue.
///
/// Answered by a message frame, or `RESP_CODE_NO_MORE_MESSAGES` when the
/// queue is empty. Reading is destructive: the node hands each message over
/// once.
pub fn sync_next_message() -> Vec<u8> {
    vec![u8::from(Command::SyncNextMessage)]
}

/// Asks about one channel slot. Answered by `RESP_CODE_CHANNEL_INFO`.
///
/// There is no command that lists channels; a caller walks the indices and
/// stops at the first `ERR_CODE_NOT_FOUND`.
pub fn get_channel(index: u8) -> Vec<u8> {
    vec![u8::from(Command::GetChannel), index]
}

/// Asks for the node's custom variables.
pub fn get_custom_vars() -> Vec<u8> {
    vec![u8::from(Command::GetCustomVars)]
}

/// Asks for the node's tuning parameters.
pub fn get_tuning_params() -> Vec<u8> {
    vec![u8::from(Command::GetTuningParams)]
}

/// Restarts the node. Pending contact changes are written first.
///
/// The frame carries the word `reboot`; the firmware checks it before acting.
pub fn reboot() -> Vec<u8> {
    let mut frame = vec![u8::from(Command::Reboot)];
    frame.extend_from_slice(b"reboot");
    frame
}

/// Erases the node's filesystem and restarts it.
///
/// Contacts, channels and the node's identity are gone afterwards. The frame
/// carries the word `reset`. There is no undo and no confirmation beyond that
/// word.
pub fn factory_reset() -> Vec<u8> {
    let mut frame = vec![u8::from(Command::FactoryReset)];
    frame.extend_from_slice(b"reset");
    frame
}

/// Opcode followed by a full public key — the shape of most commands here.
fn with_key(command: Command, public_key: &[u8; 32]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(33);
    frame.push(u8::from(command));
    frame.extend_from_slice(public_key);
    frame
}

/// Refuses text the node would truncate or reject.
fn check_text(text: &str, what: &'static str, allowed: usize) -> Result<(), CommandError> {
    if text.is_empty() {
        return Err(CommandError::Empty { what });
    }

    // Bytes, not characters: one umlaut takes two of them.
    if text.len() > allowed {
        return Err(CommandError::TooLong {
            what,
            len: text.len(),
            allowed,
        });
    }

    Ok(())
}

/// Refuses a coordinate the firmware would answer with ERR_CODE_ILLEGAL_ARG.
fn check_range(value: i32, what: &'static str, limit: i32) -> Result<(), CommandError> {
    if value.abs() > limit {
        return Err(CommandError::OutOfRange { what, value, limit });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [0xAB; 32];

    /// Every command that takes nothing but a full key looks the same on the
    /// wire, and the firmware reads all of them the same way.
    #[test]
    fn key_only_commands_carry_the_full_key_after_the_opcode() {
        let cases: [(Vec<u8>, Command); 7] = [
            (reset_path(&KEY), Command::ResetPath),
            (remove_contact(&KEY), Command::RemoveContact),
            (share_contact(&KEY), Command::ShareContact),
            (get_contact_by_key(&KEY), Command::GetContactByKey),
            (has_connection(&KEY), Command::HasConnection),
            (logout(&KEY), Command::Logout),
            (send_status_request(&KEY), Command::SendStatusReq),
        ];

        for (frame, opcode) in cases {
            assert_eq!(frame[0], u8::from(opcode), "wrong opcode for {opcode:?}");
            assert_eq!(&frame[1..], &KEY, "wrong key for {opcode:?}");
            assert_eq!(frame.len(), 33, "wrong length for {opcode:?}");
        }
    }

    #[test]
    fn a_self_advert_says_how_far_it_should_travel() {
        // The flag is read only when present; without it the node sends one
        // hop, which is the quieter default.
        assert_eq!(
            send_self_advert(true),
            vec![u8::from(Command::SendSelfAdvert), 1]
        );
        assert_eq!(
            send_self_advert(false),
            vec![u8::from(Command::SendSelfAdvert), 0]
        );
    }

    #[test]
    fn a_name_runs_to_the_end_of_the_frame() {
        let frame = set_advert_name("Repeater Nord").unwrap();

        assert_eq!(frame[0], u8::from(Command::SetAdvertName));
        assert_eq!(&frame[1..], "Repeater Nord".as_bytes());
    }

    #[test]
    fn refuses_a_name_the_node_would_cut() {
        // The firmware truncates silently past 31 bytes. Silently losing the
        // end of someone's node name is worse than saying no.
        let error = set_advert_name(&"x".repeat(32)).unwrap_err();

        assert_eq!(
            error,
            CommandError::TooLong {
                what: "the node name",
                len: 32,
                allowed: MAX_NODE_NAME
            }
        );
    }

    #[test]
    fn measures_a_name_in_bytes_not_characters() {
        // Umlauts take two bytes, and the node counts what is on the wire.
        assert!(set_advert_name(&"ä".repeat(16)).is_err());
        assert!(set_advert_name(&"ä".repeat(15)).is_ok());
    }

    #[test]
    fn refuses_an_empty_name() {
        // len >= 2 means opcode plus at least one byte.
        assert_eq!(
            set_advert_name(""),
            Err(CommandError::Empty {
                what: "the node name"
            })
        );
    }

    #[test]
    fn a_position_travels_as_two_little_endian_micro_degrees() {
        let frame = set_advert_position(52_520_008, 13_404_954).unwrap();

        assert_eq!(frame[0], u8::from(Command::SetAdvertLatLon));
        assert_eq!(&frame[1..5], &52_520_008_i32.to_le_bytes());
        assert_eq!(&frame[5..9], &13_404_954_i32.to_le_bytes());
        assert_eq!(frame.len(), 9, "altitude is reserved for future use");
    }

    #[test]
    fn accepts_a_southern_and_western_position() {
        // Negative coordinates are ordinary; a parser that forgets the sign
        // puts Patagonia in Siberia.
        let frame = set_advert_position(-33_868_800, -151_209_300).unwrap();

        assert_eq!(&frame[1..5], &(-33_868_800_i32).to_le_bytes());
    }

    #[test]
    fn refuses_a_position_the_firmware_would_reject() {
        // Same bounds the firmware checks, so the caller learns the reason
        // instead of getting a bare ERR_CODE_ILLEGAL_ARG.
        assert!(set_advert_position(91_000_000, 0).is_err());
        assert!(set_advert_position(0, -181_000_000).is_err());
        assert!(set_advert_position(90_000_000, 180_000_000).is_ok());
    }

    #[test]
    fn a_clock_setting_carries_seconds_little_endian() {
        let frame = set_device_time(1_700_000_000);

        assert_eq!(frame[0], u8::from(Command::SetDeviceTime));
        assert_eq!(&frame[1..5], &1_700_000_000_u32.to_le_bytes());
    }

    #[test]
    fn commands_without_arguments_are_a_single_byte() {
        assert_eq!(get_device_time(), vec![u8::from(Command::GetDeviceTime)]);
        assert_eq!(get_custom_vars(), vec![u8::from(Command::GetCustomVars)]);
        assert_eq!(
            get_tuning_params(),
            vec![u8::from(Command::GetTuningParams)]
        );
    }

    #[test]
    fn a_login_carries_the_key_then_the_password() {
        let frame = send_login(&KEY, "geheim").unwrap();

        assert_eq!(frame[0], u8::from(Command::SendLogin));
        assert_eq!(&frame[1..33], &KEY);
        assert_eq!(&frame[33..], "geheim".as_bytes());
    }

    #[test]
    fn a_login_without_a_password_is_allowed() {
        // The firmware null-terminates whatever is there, including nothing —
        // repeaters without a password exist.
        assert_eq!(send_login(&KEY, "").unwrap().len(), 33);
    }

    #[test]
    fn an_advert_path_request_reserves_the_byte_after_the_opcode() {
        let frame = get_advert_path(&KEY);

        assert_eq!(frame[0], u8::from(Command::GetAdvertPath));
        assert_eq!(frame[1], 0, "reserved, sent as zero");
        assert_eq!(&frame[2..], &KEY);
        assert_eq!(frame.len(), 34);
    }

    #[test]
    fn an_app_start_reserves_seven_bytes_before_the_name() {
        // The firmware ignores anything shorter than eight bytes outright —
        // no answer, no error — so the reserved block is not optional.
        let frame = app_start("MeshDash").unwrap();

        assert_eq!(frame[0], u8::from(Command::AppStart));
        assert_eq!(&frame[1..8], &[0; 7], "reserved");
        assert_eq!(&frame[8..], b"MeshDash");
        assert!(frame.len() >= 8);
    }

    #[test]
    fn an_app_start_without_a_name_still_reaches_the_minimum() {
        assert_eq!(app_start("").unwrap().len(), 8);
    }

    #[test]
    fn a_stats_request_names_the_kind() {
        assert_eq!(get_stats(0), vec![u8::from(Command::GetStats), 0]);
    }

    #[test]
    fn the_destructive_commands_carry_their_magic_word() {
        // The firmware compares these before acting. They are the only thing
        // standing between a stray frame and a wiped node.
        assert_eq!(&reboot()[1..], b"reboot");
        assert_eq!(&factory_reset()[1..], b"reset");
        assert_eq!(reboot()[0], u8::from(Command::Reboot));
        assert_eq!(factory_reset()[0], u8::from(Command::FactoryReset));
    }
}
