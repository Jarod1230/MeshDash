//! Domain modules. Everything MeshDash actually does for an operator lives
//! here, split into modules that do not know about each other.
//!
//! # Rules for modules
//!
//! - A module owns its own tables, prefixed `<module>_`. It never reads or
//!   writes another module's tables.
//! - Modules do not call each other. Coupling runs through the event bus, so
//!   that a module stays removable.
//! - A module must start even when every other module is disabled. If it
//!   cannot, the cut is wrong.
//!
//! The test for a good cut: removing a module means touching two registration
//! lists and nothing else. See `docs/module-system.md`.

pub mod messages;
pub mod nodes;
pub mod system;
