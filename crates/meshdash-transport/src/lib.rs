//! Transports to a MeshCore companion node.
//!
//! Owns connections and reconnection, not opcodes. Serial and TCP come first,
//! BLE later — see `docs/decisions/0003-transport-priorisierung.md`. A mock
//! transport is part of this crate from the start, because without it neither
//! CI nor a contributor without hardware can test anything.
//!
//! # The trait speaks in frames, not bytes
//!
//! [`Transport`] hands over whole frames and says nothing about how they are
//! delimited. That is deliberate: BLE frames are bounded by the characteristic
//! itself, serial and TCP frames by a length header. Putting the length prefix
//! into the shared interface would make BLE impossible to add later without a
//! rewrite, so each implementation does its own framing.
//!
//! # Step 3 in progress
//!
//! The trait and the mock exist. Serial and TCP do not yet — see
//! `docs/roadmap.md`.

pub mod mock;

/// Why a transport operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The link is not usable: never opened, closed by the peer, or a cable
    /// pulled. The caller is expected to reconnect rather than to give up.
    #[error("transport is not connected: {reason}")]
    Disconnected {
        /// What ended the connection, for the log.
        reason: String,
    },

    /// The underlying device or socket failed.
    #[error("transport I/O failed")]
    Io(#[from] std::io::Error),
}

/// A connection to a companion node, in whole frames.
///
/// Implementations own connecting, framing and reconnecting. They know nothing
/// about opcodes — what a frame *means* is `meshdash-proto`'s business.
#[async_trait::async_trait]
pub trait Transport: Send {
    /// Opens the connection, or returns why it could not be opened.
    async fn connect(&mut self) -> Result<(), TransportError>;

    /// Sends one frame to the node.
    async fn send(&mut self, frame: &[u8]) -> Result<(), TransportError>;

    /// Waits for the next frame from the node.
    ///
    /// Returns [`TransportError::Disconnected`] when the link ends. A caller
    /// looping over this should treat that as "reconnect", not as "stop".
    async fn recv(&mut self) -> Result<Vec<u8>, TransportError>;

    /// Closes the connection. Calling it on a closed transport is not an error.
    async fn disconnect(&mut self) -> Result<(), TransportError>;
}
