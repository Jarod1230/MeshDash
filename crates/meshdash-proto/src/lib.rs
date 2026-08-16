//! Codec for the MeshCore companion protocol.
//!
//! This crate translates bytes to types and back. It performs **no I/O** and
//! pulls in no async runtime, so the most error-prone layer of the project can
//! be tested with plain unit tests over fixed byte arrays.
//!
//! # Scaffolding only
//!
//! Nothing is implemented yet. Step 2 of `docs/roadmap.md` fills this in.
//!
//! The serial and TCP framing is verified against the firmware source and is
//! no longer a blocker: frames are `[marker: u8][len: u16 LE][payload]`, the
//! marker is `0x3C` app-to-radio and `0x3E` radio-to-app, `len` counts the
//! payload only, and there is no checksum. The **opcodes are still
//! unverified**. See `docs/research/meshcore-companion-protocol.md`.
//!
//! # Rule for this crate
//!
//! Never guess an opcode, offset or field width. A wrong guess raises no error
//! — it silently writes wrong data. Every value needs a source, cited at the
//! value itself. Unknown opcodes must round-trip, not be dropped.
