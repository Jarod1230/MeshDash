//! Broadcast of everything worth reacting to.
//!
//! The bus is how modules stay independent of each other: they do not call each
//! other, they listen to the same stream and decide for themselves what matters.
//! Several modules may process the same event without knowing about one another.
//!
//! # No domain knowledge here
//!
//! An event says what happened on the link, not what it means. That a frame
//! carrying opcode `0x80` is an advertisement — and what to store about it — is
//! a module's business, not the core's.

use serde::{Serialize, Serializer};
use tokio::sync::broadcast;

/// How many events are kept for a subscriber that is falling behind.
///
/// Generous, because a subscriber writing to SQLite may pause briefly and
/// should not lose history for it.
const DEFAULT_CAPACITY: usize = 1024;

/// Something that happened and that modules may want to know about.
///
/// Serialises with a `type` field, so a browser can switch on it:
/// `{"type": "node_connected"}`. Field names are `snake_case`, matching the
/// rest of the API — see `docs/conventions.md`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    /// The link opened a connection to the node.
    NodeConnected,

    /// The node answered the session start with its own description.
    ///
    /// Sent right after [`AppEvent::NodeConnected`] and only when the node
    /// answered. The payload is `RESP_CODE_SELF_INFO`, untouched — the core
    /// does not read it. It carries the node's own key, name, position and
    /// radio settings, which no other command returns.
    SessionStarted {
        /// Raw payload of the frame, first byte included.
        #[serde(serialize_with = "as_hex")]
        payload: Vec<u8>,
    },

    /// The connection to the node ended.
    NodeDisconnected {
        /// What ended it, for display and logging.
        reason: String,
    },

    /// The node sent something unprompted.
    ///
    /// The payload is passed on untouched, first byte included, so a module can
    /// read the opcode itself. Frames whose meaning we cannot decode yet still
    /// arrive here rather than being dropped.
    Push {
        /// Raw payload of the frame, without the transport's framing.
        ///
        /// Sent as a lowercase hex string rather than an array of numbers:
        /// shorter on the wire, readable in a log, and consistent with how the
        /// API spells binary data elsewhere.
        #[serde(serialize_with = "as_hex")]
        payload: Vec<u8>,
    },

    /// A module published something other modules may care about.
    ///
    /// The core carries this and does not read it: neither the permitted
    /// `kind` values nor the shape of `data` are its business. Both belong to
    /// the publishing module and are documented there. See
    /// `docs/decisions/0007-modul-ereignisse.md`.
    ///
    /// A receiver filters on `module` **and** `kind`, and skips a payload that
    /// does not match its expectations rather than failing on it.
    ///
    /// This goes out over `/api/v1/events` as well, so `data` must never carry
    /// a secret.
    Module {
        /// Which module published it.
        module: String,
        /// What it is, within that module's own vocabulary.
        kind: String,
        /// The payload, shaped by the publishing module.
        data: serde_json::Value,
    },
}

/// Writes bytes as a lowercase hex string.
fn as_hex<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        // Writing into a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    serializer.serialize_str(&hex)
}

/// Distributes [`AppEvent`]s to everyone listening.
///
/// Cloning shares the same bus. Publishing when nobody listens is not an error
/// — the core must not depend on any module being present.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AppEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl EventBus {
    /// Creates a bus with room for [`DEFAULT_CAPACITY`] buffered events.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a bus with a specific buffer size.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Announces an event to every current subscriber.
    ///
    /// Returns how many subscribers received it, which is mostly useful for
    /// diagnostics — a zero is normal, not a failure.
    pub fn publish(&self, event: AppEvent) -> usize {
        // The error case only means nobody is listening.
        self.sender.send(event).unwrap_or(0)
    }

    /// Starts listening.
    ///
    /// Only events published **after** this call arrive; there is no backlog.
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delivers_an_event_to_a_listener() {
        let bus = EventBus::new();
        let mut listener = bus.subscribe();

        bus.publish(AppEvent::NodeConnected);

        assert_eq!(listener.recv().await.unwrap(), AppEvent::NodeConnected);
    }

    #[tokio::test]
    async fn delivers_the_same_event_to_every_listener() {
        // The point of a bus: modules process the same event independently.
        let bus = EventBus::new();
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();

        bus.publish(AppEvent::Push {
            payload: vec![0x80, 0x01],
        });

        let expected = AppEvent::Push {
            payload: vec![0x80, 0x01],
        };
        assert_eq!(first.recv().await.unwrap(), expected);
        assert_eq!(second.recv().await.unwrap(), expected);
    }

    #[tokio::test]
    async fn publishing_without_listeners_is_fine() {
        let bus = EventBus::new();

        // The core must run even with every module switched off.
        assert_eq!(bus.publish(AppEvent::NodeConnected), 0);
    }

    #[tokio::test]
    async fn reports_how_many_listeners_received_an_event() {
        let bus = EventBus::new();
        let _first = bus.subscribe();
        let _second = bus.subscribe();

        assert_eq!(bus.publish(AppEvent::NodeConnected), 2);
    }

    #[tokio::test]
    async fn a_clone_shares_the_same_bus() {
        let bus = EventBus::new();
        let mut listener = bus.subscribe();

        // Handing a clone to a module must not create a separate bus.
        let elsewhere = bus.clone();
        elsewhere.publish(AppEvent::NodeConnected);

        assert_eq!(listener.recv().await.unwrap(), AppEvent::NodeConnected);
    }

    #[tokio::test]
    async fn keeps_events_in_order() {
        let bus = EventBus::new();
        let mut listener = bus.subscribe();

        bus.publish(AppEvent::NodeConnected);
        bus.publish(AppEvent::Push {
            payload: vec![0x83],
        });
        bus.publish(AppEvent::NodeDisconnected {
            reason: "cable pulled".into(),
        });

        assert_eq!(listener.recv().await.unwrap(), AppEvent::NodeConnected);
        assert_eq!(
            listener.recv().await.unwrap(),
            AppEvent::Push {
                payload: vec![0x83]
            }
        );
        assert_eq!(
            listener.recv().await.unwrap(),
            AppEvent::NodeDisconnected {
                reason: "cable pulled".into()
            }
        );
    }

    #[test]
    fn serialises_with_a_type_a_browser_can_switch_on() {
        let json = serde_json::to_string(&AppEvent::NodeConnected).unwrap();

        assert_eq!(json, r#"{"type":"node_connected"}"#);
    }

    #[test]
    fn serialises_the_reason_a_connection_ended() {
        let json = serde_json::to_string(&AppEvent::NodeDisconnected {
            reason: "cable pulled".into(),
        })
        .unwrap();

        assert_eq!(
            json,
            r#"{"type":"node_disconnected","reason":"cable pulled"}"#
        );
    }

    #[test]
    fn serialises_a_payload_as_lowercase_hex() {
        let json = serde_json::to_string(&AppEvent::Push {
            payload: vec![0x80, 0x0f, 0xAB],
        })
        .unwrap();

        // Not an array of numbers: shorter, readable, and consistent with how
        // the API spells binary data elsewhere.
        assert_eq!(json, r#"{"type":"push","payload":"800fab"}"#);
    }

    #[test]
    fn serialises_an_empty_payload_as_an_empty_string() {
        let json = serde_json::to_string(&AppEvent::Push { payload: vec![] }).unwrap();

        assert_eq!(json, r#"{"type":"push","payload":""}"#);
    }

    #[tokio::test]
    async fn a_listener_that_falls_too_far_behind_loses_the_oldest() {
        let bus = EventBus::with_capacity(2);
        let mut listener = bus.subscribe();

        for _ in 0..5 {
            bus.publish(AppEvent::NodeConnected);
        }

        // Losing history is better than stalling the link; the loss is
        // reported rather than hidden.
        assert!(matches!(
            listener.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }
}
