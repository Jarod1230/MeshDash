//! Transports to a MeshCore companion node.
//!
//! Owns connections and reconnection, not opcodes. Serial and TCP come first,
//! BLE later — see `docs/decisions/0003-transport-priorisierung.md`. A mock
//! transport is part of this crate from the start, because without it neither
//! CI nor a contributor without hardware can test anything.
//!
//! # Scaffolding only
//!
//! Nothing is implemented yet. Step 3 of `docs/roadmap.md` fills this in.
//!
//! # Constraint on the future `Transport` trait
//!
//! The trait must **not** assume a length prefix. BLE frames are delimited by
//! the characteristic itself, serial and TCP frames by a length header. Frame
//! delimitation therefore belongs in each transport implementation, not in the
//! shared interface — otherwise BLE cannot be added later without a rewrite.
