//! MeshDash binary.
//!
//! Assembles the router from the module registry, serves the API and the
//! WebSocket event stream, and embeds the built frontend.
//!
//! # Scaffolding only
//!
//! Nothing is served yet. Step 5 of `docs/roadmap.md` fills this in.

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    println!("{NAME} {VERSION}");
    println!("Scaffolding only — no functionality implemented yet.");
    println!("Next steps: docs/roadmap.md");
}
