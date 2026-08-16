//! Opcodes of the companion protocol.
//!
//! Every value here was read from the `#define`s at the top of
//! `examples/companion_radio/MyMesh.cpp`, MeshCore commit `d929643`, firmware
//! `v1.17.1` (`FIRMWARE_VER_CODE` 13). The tables in
//! `docs/research/meshcore-companion-protocol.md` carry the same values with
//! the full source discussion.
//!
//! # Unknown values round-trip
//!
//! Each enum keeps an `Unknown(u8)` variant. The tables are complete for *that*
//! firmware, not for future ones, and a node we do not fully understand must
//! not cost us the byte. Converting to `u8` and back always yields the original
//! value — including for unknown ones.
//!
//! # This layer knows opcodes, not payloads
//!
//! The payload layouts behind these codes are **not** verified. Knowing that a
//! frame starts with `0x0C` says nothing about how to read the rest of it.

/// Declares an opcode enum together with both conversions.
///
/// Writing the two directions by hand would mean maintaining the same table
/// twice, and a single typo there produces silently wrong data rather than a
/// compile error. Generating both from one list makes that impossible: the
/// round trip holds by construction.
macro_rules! opcode_enum {
    (
        $(#[$enum_doc:meta])*
        $name:ident {
            $(
                $(#[$variant_doc:meta])*
                $variant:ident = $value:literal,
            )*
        }
    ) => {
        $(#[$enum_doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $(
                $(#[$variant_doc])*
                $variant,
            )*
            /// A value this firmware version does not define. Kept so it can be
            /// logged and passed on instead of being dropped.
            Unknown(u8),
        }

        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                match value {
                    $($value => Self::$variant,)*
                    other => Self::Unknown(other),
                }
            }
        }

        impl From<$name> for u8 {
            fn from(code: $name) -> Self {
                match code {
                    $($name::$variant => $value,)*
                    $name::Unknown(other) => other,
                }
            }
        }
    };
}

opcode_enum! {
    /// Error codes carried in the payload of [`Response::Err`].
    ErrorCode {
        /// Command not implemented by this firmware.
        UnsupportedCmd = 1,
        /// Target not found.
        NotFound = 2,
        /// A table or queue is full; retry later.
        TableFull = 3,
        /// Device is not in a state that allows this command.
        BadState = 4,
        /// Filesystem error.
        FileIoError = 5,
        /// Argument rejected as invalid.
        IllegalArg = 6,
    }
}

opcode_enum! {
    /// Commands sent from the app to the radio.
    ///
    /// Numbers 44 to 49 are parked in the firmware for possible WiFi
    /// operations, and 53 is absent without a stated reason. Both gaps decode
    /// as [`Command::Unknown`].
    Command {
        /// Must be sent early; the radio answers with [`Response::SelfInfo`].
        AppStart = 1,
        SendTxtMsg = 2,
        SendChannelTxtMsg = 3,
        /// Takes an optional `since` timestamp for an incremental sync.
        GetContacts = 4,
        GetDeviceTime = 5,
        SetDeviceTime = 6,
        SendSelfAdvert = 7,
        SetAdvertName = 8,
        AddUpdateContact = 9,
        /// Fetches one pending message, until [`Response::NoMoreMessages`].
        SyncNextMessage = 10,
        SetRadioParams = 11,
        SetRadioTxPower = 12,
        ResetPath = 13,
        SetAdvertLatLon = 14,
        RemoveContact = 15,
        ShareContact = 16,
        ExportContact = 17,
        ImportContact = 18,
        Reboot = 19,
        /// Named `CMD_GET_BATTERY_VOLTAGE` in older firmware.
        GetBattAndStorage = 20,
        SetTuningParams = 21,
        /// Second byte announces the protocol version the app understands.
        /// That choice decides which response variants the radio sends.
        DeviceQuery = 22,
        ExportPrivateKey = 23,
        ImportPrivateKey = 24,
        SendRawData = 25,
        SendLogin = 26,
        SendStatusReq = 27,
        HasConnection = 28,
        Logout = 29,
        GetContactByKey = 30,
        GetChannel = 31,
        SetChannel = 32,
        SignStart = 33,
        SignData = 34,
        SignFinish = 35,
        SendTracePath = 36,
        SetDevicePin = 37,
        SetOtherParams = 38,
        /// Marked in the firmware as a candidate for removal.
        SendTelemetryReq = 39,
        GetCustomVars = 40,
        SetCustomVar = 41,
        GetAdvertPath = 42,
        GetTuningParams = 43,
        SendBinaryReq = 50,
        FactoryReset = 51,
        SendPathDiscoveryReq = 52,
        /// Protocol version 8 and above.
        SetFloodScopeKey = 54,
        /// Protocol version 8 and above.
        SendControlData = 55,
        /// Protocol version 8 and above. Second byte selects the stats type,
        /// see [`StatsType`].
        GetStats = 56,
        SendAnonReq = 57,
        SetAutoAddConfig = 58,
        GetAutoAddConfig = 59,
        GetAllowedRepeatFreq = 60,
        SetPathHashMode = 61,
        SendChannelData = 62,
        SetDefaultFloodScope = 63,
        GetDefaultFloodScope = 64,
        SendRawPacket = 65,
    }
}

opcode_enum! {
    /// Replies the radio sends in response to a [`Command`].
    Response {
        Ok = 0,
        /// Payload is one [`ErrorCode`].
        Err = 1,
        /// First reply to [`Command::GetContacts`].
        ContactsStart = 2,
        /// Sent once per contact.
        Contact = 3,
        /// Last reply to [`Command::GetContacts`].
        EndOfContacts = 4,
        SelfInfo = 5,
        Sent = 6,
        /// Only sent to apps announcing a protocol version below 3.
        ContactMsgRecv = 7,
        /// Only sent to apps announcing a protocol version below 3.
        ChannelMsgRecv = 8,
        CurrTime = 9,
        NoMoreMessages = 10,
        ExportContact = 11,
        BattAndStorage = 12,
        DeviceInfo = 13,
        PrivateKey = 14,
        Disabled = 15,
        /// Carries SNR. Sent from protocol version 3 onwards.
        ContactMsgRecvV3 = 16,
        /// Carries SNR. Sent from protocol version 3 onwards.
        ChannelMsgRecvV3 = 17,
        ChannelInfo = 18,
        SignStart = 19,
        Signature = 20,
        CustomVars = 21,
        AdvertPath = 22,
        TuningParams = 23,
        /// Protocol version 8 and above. Second byte is the stats type.
        Stats = 24,
        AutoAddConfig = 25,
        /// Spelled `RESP_ALLOWED_REPEAT_FREQ` in the firmware, without `CODE`.
        AllowedRepeatFreq = 26,
        ChannelDataRecv = 27,
        DefaultFloodScope = 28,
    }
}

opcode_enum! {
    /// Messages the radio sends on its own, without being asked.
    Push {
        Advert = 0x80,
        PathUpdated = 0x81,
        SendConfirmed = 0x82,
        /// The radio only signals that something is pending. Collect it with
        /// [`Command::SyncNextMessage`].
        MsgWaiting = 0x83,
        RawData = 0x84,
        LoginSuccess = 0x85,
        LoginFail = 0x86,
        StatusResponse = 0x87,
        LogRxData = 0x88,
        TraceData = 0x89,
        NewAdvert = 0x8A,
        TelemetryResponse = 0x8B,
        BinaryResponse = 0x8C,
        PathDiscoveryResponse = 0x8D,
        /// Protocol version 8 and above.
        ControlData = 0x8E,
        /// A contact was evicted to make room. Worth recording rather than
        /// dropping — it marks a gap in the history.
        ContactDeleted = 0x8F,
        /// Contact storage is full. Same reasoning as above.
        ContactsFull = 0x90,
    }
}

opcode_enum! {
    /// Selects which statistics [`Command::GetStats`] asks for, sent as the
    /// second byte of that command.
    StatsType {
        Core = 0,
        Radio = 1,
        Packets = 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_error_codes() {
        assert_eq!(ErrorCode::from(1), ErrorCode::UnsupportedCmd);
        assert_eq!(ErrorCode::from(6), ErrorCode::IllegalArg);
    }

    #[test]
    fn maps_error_codes_back_to_their_byte() {
        assert_eq!(u8::from(ErrorCode::UnsupportedCmd), 1);
        assert_eq!(u8::from(ErrorCode::IllegalArg), 6);
    }

    #[test]
    fn keeps_an_undefined_error_code() {
        assert_eq!(ErrorCode::from(99), ErrorCode::Unknown(99));
        assert_eq!(u8::from(ErrorCode::Unknown(99)), 99);
    }

    #[test]
    fn round_trips_every_error_code_byte() {
        for byte in 0..=u8::MAX {
            assert_eq!(
                u8::from(ErrorCode::from(byte)),
                byte,
                "byte {byte} was lost"
            );
        }
    }

    #[test]
    fn maps_commands_across_the_whole_range() {
        assert_eq!(Command::from(1), Command::AppStart);
        assert_eq!(Command::from(10), Command::SyncNextMessage);
        assert_eq!(Command::from(22), Command::DeviceQuery);
        assert_eq!(Command::from(56), Command::GetStats);
        assert_eq!(Command::from(65), Command::SendRawPacket);
    }

    #[test]
    fn treats_reserved_command_numbers_as_unknown() {
        // 44..=49 are parked for possible WiFi operations, 53 is simply absent.
        for reserved in [44, 45, 46, 47, 48, 49, 53] {
            assert_eq!(
                Command::from(reserved),
                Command::Unknown(reserved),
                "{reserved} is not defined by the firmware"
            );
        }
    }

    #[test]
    fn separates_command_opcodes_from_frame_markers() {
        // Both bytes double as direction markers on the frame layer. They are
        // ordinary opcodes here, and confusing the two levels is a real risk.
        assert_eq!(Command::from(0x3E), Command::SendChannelData);
        assert_eq!(Command::from(0x3C), Command::GetAllowedRepeatFreq);
    }

    #[test]
    fn maps_responses_across_the_whole_range() {
        assert_eq!(Response::from(0), Response::Ok);
        assert_eq!(Response::from(1), Response::Err);
        assert_eq!(Response::from(5), Response::SelfInfo);
        assert_eq!(Response::from(16), Response::ContactMsgRecvV3);
        assert_eq!(Response::from(28), Response::DefaultFloodScope);
    }

    #[test]
    fn maps_pushes_across_the_whole_range() {
        assert_eq!(Push::from(0x80), Push::Advert);
        assert_eq!(Push::from(0x83), Push::MsgWaiting);
        assert_eq!(Push::from(0x8B), Push::TelemetryResponse);
        assert_eq!(Push::from(0x90), Push::ContactsFull);
    }

    #[test]
    fn keeps_undefined_opcodes_of_every_kind() {
        assert_eq!(Command::from(200), Command::Unknown(200));
        assert_eq!(Response::from(200), Response::Unknown(200));
        assert_eq!(Push::from(0x00), Push::Unknown(0x00));
    }

    #[test]
    fn maps_the_statistics_types() {
        assert_eq!(StatsType::from(0), StatsType::Core);
        assert_eq!(StatsType::from(1), StatsType::Radio);
        assert_eq!(StatsType::from(2), StatsType::Packets);
        assert_eq!(StatsType::from(7), StatsType::Unknown(7));

        for byte in 0..=u8::MAX {
            assert_eq!(u8::from(StatsType::from(byte)), byte, "stats {byte} lost");
        }
    }

    /// The opcodes the planned modules depend on, checked individually because
    /// a wrong value here would silently mis-file real data.
    #[test]
    fn maps_the_opcodes_the_modules_will_use() {
        // system
        assert_eq!(Command::from(20), Command::GetBattAndStorage);
        assert_eq!(Response::from(12), Response::BattAndStorage);
        assert_eq!(Response::from(13), Response::DeviceInfo);
        // nodes
        assert_eq!(Command::from(4), Command::GetContacts);
        assert_eq!(Command::from(42), Command::GetAdvertPath);
        assert_eq!(Command::from(52), Command::SendPathDiscoveryReq);
        assert_eq!(Response::from(22), Response::AdvertPath);
        assert_eq!(Push::from(0x81), Push::PathUpdated);
        assert_eq!(Push::from(0x8A), Push::NewAdvert);
        assert_eq!(Push::from(0x8D), Push::PathDiscoveryResponse);
        // messages
        assert_eq!(Command::from(2), Command::SendTxtMsg);
        assert_eq!(Response::from(10), Response::NoMoreMessages);
        assert_eq!(Push::from(0x82), Push::SendConfirmed);
        // telemetry
        assert_eq!(Command::from(39), Command::SendTelemetryReq);
        assert_eq!(Response::from(24), Response::Stats);
        assert_eq!(Push::from(0x8B), Push::TelemetryResponse);
        // gaps in the history a dashboard must not miss
        assert_eq!(Push::from(0x8F), Push::ContactDeleted);
    }

    #[test]
    fn round_trips_every_byte_for_every_opcode_kind() {
        // Also proves no value is listed twice: a duplicate would make one of
        // the two variants unreachable and break the round trip.
        for byte in 0..=u8::MAX {
            assert_eq!(u8::from(Command::from(byte)), byte, "command {byte} lost");
            assert_eq!(u8::from(Response::from(byte)), byte, "response {byte} lost");
            assert_eq!(u8::from(Push::from(byte)), byte, "push {byte} lost");
        }
    }
}
