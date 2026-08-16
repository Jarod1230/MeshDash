//! The parts every module needs: configuration, SQLite storage, the event bus,
//! the `Link` to the node, and the module registry.
//!
//! # Scaffolding only
//!
//! Nothing is implemented yet. Step 4 of `docs/roadmap.md` fills this in.
//!
//! # Rule for this crate
//!
//! This crate knows **no domain concepts**. It has no idea what a node, a
//! message or a battery reading is — those live in `meshdash-modules`. If you
//! are about to add something here that names a domain concept, it belongs in
//! a module instead. See `docs/module-system.md`.
//!
//! The one thing this crate owes the rest of the project is a well-cut
//! `Module` trait. If that contract is sloppy, domain logic leaks in here
//! anyway and the modularity is only claimed, not real.

pub mod link;
