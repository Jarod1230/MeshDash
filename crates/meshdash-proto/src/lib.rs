//! Codec for the MeshCore companion protocol.
//!
//! This crate translates bytes to types and back. It performs **no I/O** and
//! pulls in no async runtime, so the most error-prone layer of the project can
//! be tested with plain unit tests over fixed byte arrays.
//!
//! # Scaffolding only
//!
//! Nothing is implemented yet. Step 2 of `docs/roadmap.md` fills this in, and
//! it is **blocked**: the serial framing is not verified. The published
//! documentation contradicts itself on the direction markers, and the BLE
//! description does not transfer to a byte stream. See
//! `docs/research/meshcore-companion-protocol.md`.
//!
//! # Rule for this crate
//!
//! Never guess an opcode, offset or field width. A wrong guess raises no error
//! — it silently writes wrong data. Every value needs a source, cited at the
//! value itself. Unknown opcodes must round-trip, not be dropped.
