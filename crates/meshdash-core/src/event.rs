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

use tokio::sync::broadcast;

/// How many events are kept for a subscriber that is falling behind.
///
/// Generous, because a subscriber writing to SQLite may pause briefly and
/// should not lose history for it.
const DEFAULT_CAPACITY: usize = 1024;

/// Something that happened and that modules may want to know about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// The link opened a connection to the node.
    NodeConnected,

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
        payload: Vec<u8>,
    },
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
