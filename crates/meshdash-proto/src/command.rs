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
//! # Every branch is covered, and some of them bite
//!
//! All 58 command branches of `handleCmdFrame()` have a builder here. That is
//! a deliberate choice — a command that exists can be used, and verifying its
//! payload later is harder than verifying it now. It also means this module
//! contains things a dashboard should think twice about:
//!
//! - **[`export_private_key`] and [`import_private_key`]** move the node's
//!   identity. Whoever holds that key *is* the node. Most firmware builds have
//!   these compiled out and answer `RESP_CODE_DISABLED`.
//! - **[`set_flood_scope_key`] and [`set_default_flood_scope`]** carry shared
//!   keys.
//! - **[`send_raw_data`], [`send_raw_packet`], [`send_control_data`],
//!   [`send_channel_data`], [`send_anonymous_request`]** put bytes of the
//!   caller's choosing on the air. They are checked for shape, not for sense.
//! - **[`reboot`] and [`factory_reset`]** interrupt or erase the node.
//!
//! None of these is called by MeshDash today. What a module may do with a node
//! — and what it must ask the operator first — is decided where the link
//! lives, not here.
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

/// Adds a contact to the node, or updates one it already has.
///
/// `CMD_ADD_UPDATE_CONTACT`, `len >= 36`. The frame mirrors
/// `RESP_CODE_CONTACT` byte for byte — the firmware reads it with
/// `updateContactFromFrame()`, the counterpart of the function that writes a
/// contact out.
///
/// The full 148 bytes are always sent, position and modification time
/// included. The firmware treats everything past the timestamp as optional and
/// reads whatever is there; sending less means the node keeps its old values
/// for those fields, which is rarely what a caller means.
pub fn add_or_update_contact(
    contact: &crate::contact::Contact,
    last_modified: u32,
) -> Result<Vec<u8>, CommandError> {
    let mut frame = vec![0u8; 148];
    frame[0] = u8::from(Command::AddUpdateContact);
    frame[1..33].copy_from_slice(&contact.public_key);
    frame[33] = contact.contact_type;
    frame[34] = contact.flags;

    // A contact without a known route carries the firmware's own marker for
    // it; anything else would claim a route that does not exist.
    frame[35] = match &contact.path {
        None => 0xFF,
        Some(route) => {
            let shape = crate::path::PathShape {
                stations: route.stations,
                bytes_per_station: if route.stations == 0 {
                    1
                } else {
                    (route.hops.len() / usize::from(route.stations)).max(1) as u8
                },
            };
            crate::path::encode(shape).ok_or(CommandError::TooLong {
                what: "the route",
                len: route.hops.len(),
                allowed: crate::path::MAX_PATH_BYTES,
            })?
        }
    };

    if let Some(route) = &contact.path {
        let end = 36 + route.hops.len().min(crate::path::MAX_PATH_BYTES);
        frame[36..end].copy_from_slice(&route.hops[..end - 36]);
    }

    let name = contact.name.as_bytes();
    let name_end = 100 + name.len().min(32);
    frame[100..name_end].copy_from_slice(&name[..name_end - 100]);

    frame[132..136].copy_from_slice(&contact.last_advert.to_le_bytes());
    frame[136..140].copy_from_slice(&contact.latitude.unwrap_or(0).to_le_bytes());
    frame[140..144].copy_from_slice(&contact.longitude.unwrap_or(0).to_le_bytes());
    frame[144..148].copy_from_slice(&last_modified.to_le_bytes());

    Ok(frame)
}

/// Sets up one channel. `CMD_SET_CHANNEL`, `len >= 50`.
///
/// # This carries a shared secret
///
/// The key is what lets anyone read and write the channel. It travels in this
/// frame because there is no other way to configure a channel — but nothing
/// that calls this may log it, store it or show it. MeshDash never reads a key
/// back out of a node ([`crate::channel`] skips it on purpose), so the only
/// place one can exist is the moment a person types it in.
///
/// Only 128-bit keys work: the firmware answers a 32-byte key with
/// `ERR_CODE_UNSUPPORTED_CMD`.
pub fn set_channel(index: u8, name: &str, secret: &[u8; 16]) -> Result<Vec<u8>, CommandError> {
    if name.len() > 32 {
        return Err(CommandError::TooLong {
            what: "the channel name",
            len: name.len(),
            allowed: 32,
        });
    }

    let mut frame = vec![0u8; 50];
    frame[0] = u8::from(Command::SetChannel);
    frame[1] = index;
    frame[2..2 + name.len()].copy_from_slice(name.as_bytes());
    frame[34..50].copy_from_slice(secret);

    Ok(frame)
}

/// Sets the radio's frequency, bandwidth and coding.
///
/// The units are the ones the node uses everywhere: **kilohertz** for the
/// frequency, **hertz** for the bandwidth. The firmware checks
/// 150000…2500000 kHz, 7000…500000 Hz, spreading factor 5…12 and coding rate
/// 5…8; the same bounds are checked here so the caller learns which value was
/// wrong instead of receiving a bare error code.
///
/// `repeat` asks the node to also act as a repeater (firmware v9 and up).
pub fn set_radio_params(
    frequency_khz: u32,
    bandwidth_hz: u32,
    spreading_factor: u8,
    coding_rate: u8,
    repeat: bool,
) -> Result<Vec<u8>, CommandError> {
    check_bounds(
        frequency_khz,
        "the frequency in kilohertz",
        150_000,
        2_500_000,
    )?;
    check_bounds(bandwidth_hz, "the bandwidth in hertz", 7_000, 500_000)?;
    check_bounds(u32::from(spreading_factor), "the spreading factor", 5, 12)?;
    check_bounds(u32::from(coding_rate), "the coding rate", 5, 8)?;

    let mut frame = Vec::with_capacity(12);
    frame.push(u8::from(Command::SetRadioParams));
    frame.extend_from_slice(&frequency_khz.to_le_bytes());
    frame.extend_from_slice(&bandwidth_hz.to_le_bytes());
    frame.push(spreading_factor);
    frame.push(coding_rate);
    frame.push(u8::from(repeat));

    Ok(frame)
}

/// Sets the transmit power in dBm.
///
/// The firmware accepts −9 up to the board's maximum, which
/// [`crate::device::SelfInfo`] reports. Only the lower bound can be checked
/// here; the upper one belongs to the board.
pub fn set_transmit_power(dbm: i8) -> Result<Vec<u8>, CommandError> {
    if dbm < -9 {
        return Err(CommandError::OutOfRange {
            what: "the transmit power in dBm",
            value: i32::from(dbm),
            limit: 9,
        });
    }

    Ok(vec![u8::from(Command::SetRadioTxPower), dbm as u8])
}

/// Sets how long the node waits before receiving and before flooding onward.
pub fn set_tuning_params(receive_delay_ms: u32, airtime_factor_ms: u32) -> Vec<u8> {
    let mut frame = Vec::with_capacity(9);
    frame.push(u8::from(Command::SetTuningParams));
    frame.extend_from_slice(&receive_delay_ms.to_le_bytes());
    frame.extend_from_slice(&airtime_factor_ms.to_le_bytes());
    frame
}

/// Sets one of the node's custom variables.
///
/// They travel as `name:value` in one string, which means a name containing a
/// colon cannot be expressed — the firmware splits at the first one.
pub fn set_custom_var(name: &str, value: &str) -> Result<Vec<u8>, CommandError> {
    if name.is_empty() {
        return Err(CommandError::Empty {
            what: "the variable name",
        });
    }

    if name.contains(':') {
        return Err(CommandError::TooLong {
            what: "the variable name (it must not contain a colon)",
            len: name.len(),
            allowed: 0,
        });
    }

    let pair = format!("{name}:{value}");
    let allowed = crate::frame::MAX_FRAME_SIZE - 1;
    if pair.len() > allowed {
        return Err(CommandError::TooLong {
            what: "the variable",
            len: pair.len(),
            allowed,
        });
    }

    let mut frame = Vec::with_capacity(1 + pair.len());
    frame.push(u8::from(Command::SetCustomVar));
    frame.extend_from_slice(pair.as_bytes());

    Ok(frame)
}

/// Sets the node's Bluetooth pairing code.
///
/// Zero switches it off; anything else must be exactly six digits, which is
/// what the firmware checks.
pub fn set_device_pin(pin: u32) -> Result<Vec<u8>, CommandError> {
    if pin != 0 && !(100_000..=999_999).contains(&pin) {
        return Err(CommandError::OutOfRange {
            what: "the pairing code (zero, or six digits)",
            value: pin.min(i32::MAX as u32) as i32,
            limit: 999_999,
        });
    }

    let mut frame = Vec::with_capacity(5);
    frame.push(u8::from(Command::SetDevicePin));
    frame.extend_from_slice(&pin.to_le_bytes());

    Ok(frame)
}

/// Sets which contacts the node adds on its own.
pub fn set_autoadd_config(flags: u8, max_stations: u8) -> Vec<u8> {
    vec![u8::from(Command::SetAutoAddConfig), flags, max_stations]
}

/// Sends a trace along a chosen route — the mesh's traceroute.
///
/// `CMD_SEND_TRACE_PATH`, `len > 10`. The answer comes back later as a trace
/// push.
///
/// The low two bits of `flags` say how many bytes each station takes, and the
/// firmware refuses a route whose length is not a multiple of that — the same
/// check is made here, because the alternative is `ERR_CODE_ILLEGAL_ARG` with
/// no hint which of the two was wrong.
pub fn send_trace(
    tag: u32,
    authentication_code: u32,
    flags: u8,
    hop_hashes: &[u8],
) -> Result<Vec<u8>, CommandError> {
    if hop_hashes.is_empty() {
        return Err(CommandError::Empty {
            what: "the route to trace",
        });
    }

    let group = 1usize << (flags & 0b0000_0011);
    if hop_hashes.len() % group != 0 {
        return Err(CommandError::TooLong {
            what: "the route (its length must be a multiple of the station width)",
            len: hop_hashes.len(),
            allowed: group,
        });
    }

    let mut frame = Vec::with_capacity(10 + hop_hashes.len());
    frame.push(u8::from(Command::SendTracePath));
    frame.extend_from_slice(&tag.to_le_bytes());
    frame.extend_from_slice(&authentication_code.to_le_bytes());
    frame.push(flags);
    frame.extend_from_slice(hop_hashes);

    Ok(frame)
}

/// Asks the node to find the route to a contact, in both directions.
///
/// `CMD_SEND_PATH_DISCOVERY_REQ`, `len >= 34`; the byte after the opcode must
/// be zero. The firmware builds this as "flood plus telemetry request" and
/// answers later with a path discovery push.
pub fn send_path_discovery(public_key: &[u8; 32]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(34);
    frame.push(u8::from(Command::SendPathDiscoveryReq));
    frame.push(0); // must be zero
    frame.extend_from_slice(public_key);
    frame
}

/// Asks which frequencies this node may repeat on.
pub fn get_allowed_repeat_frequencies() -> Vec<u8> {
    vec![u8::from(Command::GetAllowedRepeatFreq)]
}

/// Asks which contacts the node adds on its own.
pub fn get_autoadd_config() -> Vec<u8> {
    vec![u8::from(Command::GetAutoAddConfig)]
}

/// Asks for the default flood scope.
pub fn get_default_flood_scope() -> Vec<u8> {
    vec![u8::from(Command::GetDefaultFloodScope)]
}

/// Exports a contact so it can be shared, or the node itself.
///
/// Without a key the node exports **itself**; the firmware decides by frame
/// length (`len < 33`). Answered by `RESP_CODE_EXPORT_CONTACT`.
pub fn export_contact(public_key: Option<&[u8; 32]>) -> Vec<u8> {
    match public_key {
        None => vec![u8::from(Command::ExportContact)],
        Some(key) => with_key(Command::ExportContact, key),
    }
}

/// Sets the node's remaining preferences.
///
/// Everything past the first byte is optional; the firmware reads as far as
/// the frame goes. All of it is sent here, because leaving a field off means
/// the node keeps its old value, which is rarely what a caller means.
///
/// `telemetry_permissions` packs three two-bit fields: base, location and
/// environment, from the low bits upward.
pub fn set_other_params(
    manual_add_contacts: bool,
    telemetry_permissions: u8,
    advert_location_policy: u8,
    multi_acknowledgements: u8,
) -> Vec<u8> {
    vec![
        u8::from(Command::SetOtherParams),
        u8::from(manual_add_contacts),
        telemetry_permissions,
        advert_location_policy,
        multi_acknowledgements,
    ]
}

/// Asks the node to hand over its **private** key.
///
/// Whoever holds that key is the node: they can sign as it and read what is
/// sent to it. Most firmware builds are compiled without
/// `ENABLE_PRIVATE_KEY_EXPORT` and answer `RESP_CODE_DISABLED` instead.
///
/// The answer is read by [`crate::response::private_key`], which exists only
/// because this command does.
pub fn export_private_key() -> Vec<u8> {
    vec![u8::from(Command::ExportPrivateKey)]
}

/// Replaces the node's identity with the given key. `len >= 65`.
///
/// Everything the node was — its address in the mesh, its ability to read
/// messages sent to it — is gone afterwards and replaced by whatever this key
/// says. The firmware validates the key and reloads its contacts, because
/// every shared secret computed from the old identity is now wrong.
pub fn import_private_key(identity: &[u8; 64]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(65);
    frame.push(u8::from(Command::ImportPrivateKey));
    frame.extend_from_slice(identity);
    frame
}

/// Imports a contact from an exported advert packet. `len > 99`.
///
/// The payload is whatever [`export_contact`] produced on some node — an
/// advert packet, not a contact record. The firmware parses and verifies it.
pub fn import_contact(advert_packet: &[u8]) -> Result<Vec<u8>, CommandError> {
    // The branch needs more than 2 + 32 + 64 bytes in total, so the packet
    // itself has to exceed 98.
    if advert_packet.len() < 98 {
        return Err(CommandError::TooLong {
            what: "the advert packet (it is too short to be one)",
            len: advert_packet.len(),
            allowed: 98,
        });
    }

    let mut frame = Vec::with_capacity(1 + advert_packet.len());
    frame.push(u8::from(Command::ImportContact));
    frame.extend_from_slice(advert_packet);

    Ok(frame)
}

/// Sends a request to someone who need not be a contact yet. `len > 33`.
///
/// From firmware version 13 the node adds an unknown key as a contact of type
/// "none" rather than refusing — so this quietly grows the contact list.
pub fn send_anonymous_request(
    public_key: &[u8; 32],
    request: &[u8],
) -> Result<Vec<u8>, CommandError> {
    if request.is_empty() {
        return Err(CommandError::Empty {
            what: "the request body",
        });
    }

    let mut frame = Vec::with_capacity(33 + request.len());
    frame.push(u8::from(Command::SendAnonReq));
    frame.extend_from_slice(public_key);
    frame.extend_from_slice(request);

    Ok(frame)
}

/// Sends a datagram into a channel. `len >= 4`.
///
/// ```text
/// 1  channel index
/// 1  route length byte, or 0xFF to flood
/// n  the route, when one is given
/// …  the payload
/// ```
///
/// `route` of `None` means flood. A route that the firmware would call invalid
/// is refused here, because its answer would be a bare `ERR_CODE_ILLEGAL_ARG`
/// naming neither which field nor why.
pub fn send_channel_data(
    channel_index: u8,
    route: Option<&[u8]>,
    payload: &[u8],
) -> Result<Vec<u8>, CommandError> {
    let mut frame = Vec::with_capacity(3 + payload.len());
    frame.push(u8::from(Command::SendChannelData));
    frame.push(channel_index);

    match route {
        None => frame.push(0xFF), // flood
        Some(hops) => {
            let shape = crate::path::PathShape {
                stations: u8::try_from(hops.len()).map_err(|_| CommandError::TooLong {
                    what: "the route",
                    len: hops.len(),
                    allowed: 63,
                })?,
                bytes_per_station: 1,
            };
            frame.push(crate::path::encode(shape).ok_or(CommandError::TooLong {
                what: "the route",
                len: hops.len(),
                allowed: 63,
            })?);
            frame.extend_from_slice(hops);
        }
    }

    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Sends control data one hop. `len >= 2`.
///
/// The firmware only takes the branch when the **high bit of the first payload
/// byte is set**; without it the frame falls through and comes back as an
/// unsupported command. That bit is checked here so the caller learns why.
pub fn send_control_data(payload: &[u8]) -> Result<Vec<u8>, CommandError> {
    let Some(&first) = payload.first() else {
        return Err(CommandError::Empty {
            what: "the control payload",
        });
    };

    if first & 0x80 == 0 {
        return Err(CommandError::OutOfRange {
            what: "the first control byte (its high bit must be set)",
            value: i32::from(first),
            limit: 0x80,
        });
    }

    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(u8::from(Command::SendControlData));
    frame.extend_from_slice(payload);

    Ok(frame)
}

/// Sends raw data along a known route. `len >= 6`.
///
/// Flooding is **not** supported: the firmware answers a negative route length
/// with `ERR_CODE_UNSUPPORTED_CMD`. The payload needs at least four bytes.
pub fn send_raw_data(route: &[u8], payload: &[u8]) -> Result<Vec<u8>, CommandError> {
    if payload.len() < 4 {
        return Err(CommandError::TooLong {
            what: "the payload (the firmware needs at least four bytes)",
            len: payload.len(),
            allowed: 4,
        });
    }

    let route_len = u8::try_from(route.len()).map_err(|_| CommandError::TooLong {
        what: "the route",
        len: route.len(),
        allowed: 127,
    })?;

    if route_len > 127 {
        // The firmware reads this byte as int8_t and rejects negatives.
        return Err(CommandError::TooLong {
            what: "the route",
            len: route.len(),
            allowed: 127,
        });
    }

    let mut frame = Vec::with_capacity(2 + route.len() + payload.len());
    frame.push(u8::from(Command::SendRawData));
    frame.push(route_len);
    frame.extend_from_slice(route);
    frame.extend_from_slice(payload);

    Ok(frame)
}

/// Puts an already-formed packet on the air. `len >= 4`.
///
/// The node parses it and refuses what it cannot read. Nothing here checks the
/// packet's contents — that is the point of the command.
pub fn send_raw_packet(priority: u8, packet: &[u8]) -> Result<Vec<u8>, CommandError> {
    if packet.len() < 2 {
        return Err(CommandError::TooLong {
            what: "the packet",
            len: packet.len(),
            allowed: 2,
        });
    }

    let mut frame = Vec::with_capacity(2 + packet.len());
    frame.push(u8::from(Command::SendRawPacket));
    frame.push(priority);
    frame.extend_from_slice(packet);

    Ok(frame)
}

/// Asks a contact for telemetry the old way.
///
/// Superseded by [`crate::binary_request::encode_telemetry_request`]: the
/// firmware marks this "can deprecate, in favour of CMD_SEND_BINARY_REQ". Kept
/// for nodes whose firmware predates the replacement.
///
/// The three bytes after the opcode are reserved and sent as zero; with no key
/// at all the node reports its **own** telemetry.
pub fn send_telemetry_request(public_key: Option<&[u8; 32]>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(36);
    frame.push(u8::from(Command::SendTelemetryReq));
    frame.extend_from_slice(&[0, 0, 0]);

    if let Some(key) = public_key {
        frame.extend_from_slice(key);
    }

    frame
}

/// Sets the default flood scope, or clears it.
///
/// # This carries a shared key
///
/// The scope key decides which nodes relay a flood. Passing `None` clears both
/// name and key.
pub fn set_default_flood_scope(scope: Option<(&str, &[u8; 16])>) -> Result<Vec<u8>, CommandError> {
    let Some((name, key)) = scope else {
        return Ok(vec![u8::from(Command::SetDefaultFloodScope)]);
    };

    // The firmware needs a name of one to thirty characters; it measures with
    // strlen over a 31-byte field, so the terminator has to fit too.
    if name.is_empty() {
        return Err(CommandError::Empty {
            what: "the scope name",
        });
    }

    if name.len() > 30 {
        return Err(CommandError::TooLong {
            what: "the scope name",
            len: name.len(),
            allowed: 30,
        });
    }

    let mut frame = vec![0u8; 1 + 31 + 16];
    frame[0] = u8::from(Command::SetDefaultFloodScope);
    frame[1..1 + name.len()].copy_from_slice(name.as_bytes());
    frame[32..48].copy_from_slice(key);

    Ok(frame)
}

/// Overrides the scope key used for sending, or resets it.
///
/// `Some(key)` sets an override, `None` clears it back to the default.
pub fn set_flood_scope_key(key: Option<&[u8; 16]>) -> Vec<u8> {
    let mut frame = vec![u8::from(Command::SetFloodScopeKey), 0];

    if let Some(key) = key {
        frame.extend_from_slice(key);
    }

    frame
}

/// Sends without any scope at all — firmware version 12 and up.
pub fn send_unscoped() -> Vec<u8> {
    vec![u8::from(Command::SetFloodScopeKey), 1]
}

/// Chooses how the node hashes path entries. `len >= 3`.
///
/// The firmware accepts modes 0, 1 and 2; three or higher is refused.
pub fn set_path_hash_mode(mode: u8) -> Result<Vec<u8>, CommandError> {
    if mode >= 3 {
        return Err(CommandError::OutOfRange {
            what: "the path hash mode",
            value: i32::from(mode),
            limit: 2,
        });
    }

    Ok(vec![u8::from(Command::SetPathHashMode), 0, mode])
}

/// Begins a signing exchange.
///
/// The node answers with how many bytes it will accept, then takes them
/// through [`sign_data`] and produces a signature on [`sign_finish`]. The
/// buffer lives on the node between the three calls, so an abandoned exchange
/// leaves it allocated until the next start.
pub fn sign_start() -> Vec<u8> {
    vec![u8::from(Command::SignStart)]
}

/// Adds bytes to the signing buffer. `len > 1`.
///
/// Answered with an error when no exchange is open (`ERR_CODE_BAD_STATE`) or
/// when the buffer would overflow (`ERR_CODE_TABLE_FULL`).
pub fn sign_data(chunk: &[u8]) -> Result<Vec<u8>, CommandError> {
    if chunk.is_empty() {
        return Err(CommandError::Empty {
            what: "the data to sign",
        });
    }

    let mut frame = Vec::with_capacity(1 + chunk.len());
    frame.push(u8::from(Command::SignData));
    frame.extend_from_slice(chunk);

    Ok(frame)
}

/// Finishes the exchange and asks for the signature.
pub fn sign_finish() -> Vec<u8> {
    vec![u8::from(Command::SignFinish)]
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

/// Refuses a value outside the range the firmware accepts.
fn check_bounds(value: u32, what: &'static str, low: u32, high: u32) -> Result<(), CommandError> {
    if !(low..=high).contains(&value) {
        return Err(CommandError::OutOfRange {
            what,
            value: value.min(i32::MAX as u32) as i32,
            limit: high.min(i32::MAX as u32) as i32,
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

    #[test]
    fn a_contact_frame_mirrors_the_one_the_node_sends() {
        // The firmware reads this with updateContactFromFrame(), the exact
        // counterpart of the function that writes a contact out — so the
        // layout has to match RESP_CODE_CONTACT byte for byte.
        let contact = crate::contact::Contact {
            public_key: [0xAB; 32],
            contact_type: 2,
            flags: 1,
            path: Some(crate::contact::Route {
                stations: 2,
                hops: vec![0x11, 0x22],
            }),
            name: "Repeater Nord".into(),
            last_advert: 1_700_000_000,
            latitude: Some(52_520_008),
            longitude: Some(13_404_954),
            last_modified: 0,
        };

        let frame = add_or_update_contact(&contact, 1_700_000_100).unwrap();

        assert_eq!(frame.len(), 148);
        assert_eq!(frame[0], u8::from(Command::AddUpdateContact));
        assert_eq!(&frame[1..33], &[0xAB; 32]);
        assert_eq!(frame[33], 2);
        assert_eq!(frame[35], 2, "two stations, one byte each");
        assert_eq!(&frame[36..38], &[0x11, 0x22]);
        assert_eq!(&frame[100..113], b"Repeater Nord");
        assert_eq!(&frame[136..140], &52_520_008_i32.to_le_bytes());
        assert_eq!(&frame[144..148], &1_700_000_100_u32.to_le_bytes());
    }

    #[test]
    fn a_contact_without_a_route_keeps_the_unknown_marker() {
        // Writing zero would claim a direct route the node does not have.
        let contact = crate::contact::Contact {
            public_key: [0; 32],
            contact_type: 2,
            flags: 0,
            path: None,
            name: String::new(),
            last_advert: 0,
            latitude: None,
            longitude: None,
            last_modified: 0,
        };

        assert_eq!(add_or_update_contact(&contact, 0).unwrap()[35], 0xFF);
    }

    #[test]
    fn a_channel_carries_its_name_then_its_key() {
        let frame = set_channel(2, "Notfunk", &[0x99; 16]).unwrap();

        assert_eq!(frame.len(), 50);
        assert_eq!(frame[1], 2);
        assert_eq!(&frame[2..9], b"Notfunk");
        assert_eq!(&frame[34..50], &[0x99; 16]);
    }

    #[test]
    fn radio_parameters_are_checked_against_the_bounds_the_firmware_checks() {
        // 869.618 MHz at 62.5 kHz — the European mesh settings, in the units
        // the node uses: kilohertz for one, hertz for the other.
        let frame = set_radio_params(869_618, 62_500, 11, 5, false).unwrap();

        assert_eq!(&frame[1..5], &869_618_u32.to_le_bytes());
        assert_eq!(&frame[5..9], &62_500_u32.to_le_bytes());
        assert_eq!(frame[9], 11);
        assert_eq!(frame[10], 5);
        assert_eq!(frame[11], 0, "not repeating");

        assert!(set_radio_params(100_000, 62_500, 11, 5, false).is_err());
        assert!(set_radio_params(869_618, 62_500, 13, 5, false).is_err());
        assert!(set_radio_params(869_618, 62_500, 11, 9, false).is_err());
    }

    #[test]
    fn transmit_power_may_be_negative_but_not_below_minus_nine() {
        assert_eq!(set_transmit_power(-9).unwrap()[1] as i8, -9);
        assert!(set_transmit_power(-10).is_err());
        // The upper bound belongs to the board and is not checked here.
        assert!(set_transmit_power(30).is_ok());
    }

    #[test]
    fn a_custom_variable_travels_as_one_pair() {
        let frame = set_custom_var("gps", "1").unwrap();

        assert_eq!(&frame[1..], b"gps:1");
    }

    #[test]
    fn refuses_a_variable_name_with_a_colon_in_it() {
        // The firmware splits at the first colon, so such a name cannot be
        // expressed at all — better to say so than to send something that
        // silently means something else.
        assert!(set_custom_var("a:b", "1").is_err());
    }

    #[test]
    fn a_pairing_code_is_off_or_exactly_six_digits() {
        assert!(set_device_pin(0).is_ok());
        assert!(set_device_pin(123_456).is_ok());
        assert!(set_device_pin(12_345).is_err());
        assert!(set_device_pin(1_234_567).is_err());
    }

    #[test]
    fn a_trace_carries_its_tag_code_flags_and_route() {
        let frame = send_trace(42, 0xABCD, 0, &[0x11, 0x22, 0x33]).unwrap();

        assert_eq!(frame[0], u8::from(Command::SendTracePath));
        assert_eq!(&frame[1..5], &42_u32.to_le_bytes());
        assert_eq!(&frame[5..9], &0xABCD_u32.to_le_bytes());
        assert_eq!(frame[9], 0);
        assert_eq!(&frame[10..], &[0x11, 0x22, 0x33]);
    }

    #[test]
    fn refuses_a_trace_route_that_does_not_divide_by_the_station_width() {
        // With flags 1 each station takes two bytes, so an odd number of
        // bytes describes no route the firmware would accept.
        assert!(send_trace(1, 0, 0b01, &[0x11, 0x22, 0x33]).is_err());
        assert!(send_trace(1, 0, 0b01, &[0x11, 0x22]).is_ok());
    }

    #[test]
    fn refuses_an_empty_trace() {
        assert!(send_trace(1, 0, 0, &[]).is_err());
    }

    #[test]
    fn a_path_discovery_reserves_the_byte_after_the_opcode() {
        // The firmware checks it is zero before doing anything.
        let frame = send_path_discovery(&KEY);

        assert_eq!(frame[0], u8::from(Command::SendPathDiscoveryReq));
        assert_eq!(frame[1], 0);
        assert_eq!(&frame[2..], &KEY);
    }

    #[test]
    fn exporting_without_a_key_exports_the_node_itself() {
        // The firmware decides by frame length: shorter than 33 means self.
        assert_eq!(export_contact(None).len(), 1);
        assert_eq!(export_contact(Some(&KEY)).len(), 33);
    }

    #[test]
    fn the_remaining_preferences_travel_in_full() {
        let frame = set_other_params(true, 0b01_01_01, 2, 1);

        assert_eq!(frame[0], u8::from(Command::SetOtherParams));
        assert_eq!(frame[1], 1);
        assert_eq!(frame[2], 0b01_01_01, "three two-bit permission fields");
        assert_eq!(frame.len(), 5, "nothing left off");
    }

    #[test]
    fn the_key_commands_carry_exactly_what_the_firmware_reads() {
        assert_eq!(
            export_private_key(),
            vec![u8::from(Command::ExportPrivateKey)]
        );

        let frame = import_private_key(&[0x11; 64]);
        assert_eq!(frame.len(), 65, "the branch needs len >= 65");
        assert_eq!(&frame[1..], &[0x11; 64]);
    }

    #[test]
    fn a_channel_datagram_floods_when_given_no_route() {
        // 0xFF is the flood marker here, the same byte that means "no route
        // known" for a contact.
        let frame = send_channel_data(2, None, b"daten").unwrap();

        assert_eq!(frame[1], 2);
        assert_eq!(frame[2], 0xFF);
        assert_eq!(&frame[3..], b"daten");
    }

    #[test]
    fn a_channel_datagram_can_name_its_route() {
        let frame = send_channel_data(0, Some(&[0x11, 0x22]), b"daten").unwrap();

        assert_eq!(frame[2], 2, "two stations, one byte each");
        assert_eq!(&frame[3..5], &[0x11, 0x22]);
        assert_eq!(&frame[5..], b"daten");
    }

    #[test]
    fn control_data_needs_the_high_bit_of_its_first_byte() {
        // Without it the firmware branch does not match at all: no answer, no
        // error, just an unsupported-command reply from the end of the chain.
        assert!(send_control_data(&[0x80, 1, 2]).is_ok());
        assert!(send_control_data(&[0x01, 1, 2]).is_err());
        assert!(send_control_data(&[]).is_err());
    }

    #[test]
    fn raw_data_needs_four_bytes_and_a_route() {
        // Flooding is not supported here — the firmware answers a negative
        // route length with ERR_CODE_UNSUPPORTED_CMD.
        let frame = send_raw_data(&[0x11], b"vier").unwrap();

        assert_eq!(frame[1], 1, "one hop");
        assert_eq!(&frame[2..3], &[0x11]);
        assert_eq!(&frame[3..], b"vier");

        assert!(send_raw_data(&[0x11], b"kurz").is_ok());
        assert!(send_raw_data(&[0x11], b"dre").is_err());
    }

    #[test]
    fn a_telemetry_request_without_a_key_asks_the_node_itself() {
        // Three reserved bytes either way; the key decides whom it is about.
        assert_eq!(send_telemetry_request(None).len(), 4);
        assert_eq!(send_telemetry_request(Some(&KEY)).len(), 36);
        assert_eq!(&send_telemetry_request(Some(&KEY))[1..4], &[0, 0, 0]);
    }

    #[test]
    fn a_flood_scope_is_set_or_cleared() {
        let frame = set_default_flood_scope(Some(("Notfunk", &[0x99; 16]))).unwrap();

        assert_eq!(frame.len(), 48);
        assert_eq!(&frame[1..8], b"Notfunk");
        assert_eq!(&frame[32..48], &[0x99; 16]);

        // Clearing is the bare opcode.
        assert_eq!(set_default_flood_scope(None).unwrap().len(), 1);
    }

    #[test]
    fn refuses_a_scope_name_that_leaves_no_room_for_its_terminator() {
        // The firmware measures with strlen over 31 bytes, so 30 characters
        // is the most that fits.
        assert!(set_default_flood_scope(Some((&"x".repeat(30), &[0; 16]))).is_ok());
        assert!(set_default_flood_scope(Some((&"x".repeat(31), &[0; 16]))).is_err());
        assert!(set_default_flood_scope(Some(("", &[0; 16]))).is_err());
    }

    #[test]
    fn the_scope_override_can_be_set_reset_or_switched_off() {
        assert_eq!(set_flood_scope_key(Some(&[0x77; 16])).len(), 18);
        assert_eq!(
            set_flood_scope_key(None),
            vec![u8::from(Command::SetFloodScopeKey), 0]
        );
        assert_eq!(
            send_unscoped(),
            vec![u8::from(Command::SetFloodScopeKey), 1]
        );
    }

    #[test]
    fn the_path_hash_mode_is_bounded_by_the_firmware() {
        assert_eq!(
            set_path_hash_mode(2).unwrap(),
            vec![u8::from(Command::SetPathHashMode), 0, 2]
        );
        assert!(set_path_hash_mode(3).is_err());
    }

    #[test]
    fn the_signing_exchange_is_three_frames() {
        assert_eq!(sign_start(), vec![u8::from(Command::SignStart)]);
        assert_eq!(&sign_data(b"zu signieren").unwrap()[1..], b"zu signieren");
        assert_eq!(sign_finish(), vec![u8::from(Command::SignFinish)]);
        // The middle one needs at least one byte: len > 1.
        assert!(sign_data(&[]).is_err());
    }

    #[test]
    fn an_anonymous_request_needs_a_body() {
        assert_eq!(send_anonymous_request(&KEY, b"x").unwrap().len(), 34);
        assert!(send_anonymous_request(&KEY, &[]).is_err());
    }

    #[test]
    fn an_imported_contact_must_be_long_enough_to_be_an_advert() {
        assert!(import_contact(&[0; 98]).is_ok());
        assert!(import_contact(&[0; 50]).is_err());
    }

    #[test]
    fn a_raw_packet_carries_its_priority_first() {
        let frame = send_raw_packet(3, &[0xDE, 0xAD]).unwrap();

        assert_eq!(frame[0], u8::from(Command::SendRawPacket));
        assert_eq!(frame[1], 3);
        assert_eq!(&frame[2..], &[0xDE, 0xAD]);
    }
}
