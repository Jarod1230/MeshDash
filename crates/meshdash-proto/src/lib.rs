//! Codec for the MeshCore companion protocol.
//!
//! This crate translates bytes to types and back. It performs **no I/O** and
//! pulls in no async runtime, so the most error-prone layer of the project can
//! be tested with plain unit tests over fixed byte arrays.
//!
//! # What is verified
//!
//! Framing, every opcode and the payloads listed below are checked against the
//! firmware source, MeshCore commit `d929643`. Frames are
//! `[marker: u8][len: u16 LE][payload]`, the marker is `0x3C` app-to-radio and
//! `0x3E` radio-to-app, `len` counts the payload only, and there is no
//! checksum.
//!
//! Implemented and source-verified: [`frame`], [`opcode`] (all 110 constants),
//! [`device`], [`contact`], [`advert`], [`message`], [`channel`], [`send`] and
//! [`battery`].
//!
//! Still unverified, and therefore unimplemented: the layout of
//! `RESP_CODE_STATS`, of `PUSH_CODE_TELEMETRY_RESPONSE` (CayenneLPP, a foreign
//! format), and the meaning of a contact's `type` and `flags` bytes. See
//! `docs/research/meshcore-companion-protocol.md` for the current state.
//!
//! # Rule for this crate
//!
//! Never guess an opcode, offset or field width. A wrong guess raises no error
//! — it silently writes wrong data. Every value needs a source, cited at the
//! value itself. Unknown opcodes must round-trip, not be dropped.

pub mod advert;
pub mod battery;
pub mod binary_request;
pub mod channel;
pub mod command;
pub mod contact;
pub mod device;
pub mod frame;
pub mod lpp;
pub mod message;
pub mod opcode;
pub mod path;
pub mod send;
