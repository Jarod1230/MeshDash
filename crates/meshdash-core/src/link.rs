//! The actor that owns the connection to a node.
//!
//! A companion node answers commands one at a time, in order, and there is no
//! request tag to match an answer to its question. The link makes that
//! manageable: callers hand over a command and await their answer, while
//! everything the node says on its own goes to the [`crate::event::EventBus`].
//! Whether the connection is up is reported there too — for a dashboard that is
//! the first thing worth showing.
//!
//! # How an answer is recognised
//!
//! Purely by the opcode range — replies stay below `0x80`, pushes start there
//! (see [`meshdash_proto::opcode::is_push`]). While a command is outstanding,
//! the first non-push frame is its answer; pushes arriving in between are
//! forwarded and do not disturb the correlation.
//!
//! # Why this lives in the core, not in the transport
//!
//! Telling a push from a reply is protocol knowledge, and the transport crate
//! deliberately has none — it moves frames and knows nothing of their meaning.
//! Domain knowledge stays out of here too: this layer never learns what a node
//! or a message is.

use std::time::Duration;

use meshdash_proto::opcode;
use meshdash_transport::{Transport, TransportError};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::event::{AppEvent, EventBus};

/// How many commands may wait to be sent before callers are made to wait.
const COMMAND_QUEUE: usize = 32;

/// Settings for a link.
#[derive(Debug, Clone)]
pub struct LinkConfig {
    /// How long to wait for a node's answer before giving up on a command.
    ///
    /// Not a protocol value — the firmware does not promise a deadline. It is
    /// an operating choice: long enough for a busy node, short enough that a
    /// silent one does not block the queue forever.
    pub response_timeout: Duration,

    /// How to behave when the connection is gone.
    pub reconnect: ReconnectConfig,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(5),
            reconnect: ReconnectConfig::default(),
        }
    }
}

/// How persistently to reopen a connection that went away.
///
/// A pulled USB cable or a rebooting node must not end the service, so the link
/// keeps trying — but backs off, so a node that stays away does not turn into a
/// busy loop.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Wait before the first retry.
    pub initial_delay: Duration,

    /// Upper bound for the wait, however often reconnecting fails.
    pub max_delay: Duration,

    /// What the delay is multiplied by after each failed attempt.
    pub factor: u32,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        // Operating choices, not protocol values: fast enough that a brief
        // unplug is barely noticed, slow enough that an absent node costs
        // almost nothing.
        Self {
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            factor: 2,
        }
    }
}

/// Why a command did not produce an answer.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// The node did not answer within [`LinkConfig::response_timeout`].
    #[error("node did not answer within {0:?}")]
    Timeout(Duration),

    /// The connection failed while the command was in flight.
    #[error("link transport failed")]
    Transport(#[from] TransportError),

    /// The actor is gone, so no further command can be served.
    #[error("link is closed")]
    Closed,
}

/// One command waiting to be sent, with the channel for its answer.
struct Request {
    frame: Vec<u8>,
    reply: oneshot::Sender<Result<Vec<u8>, LinkError>>,
}

/// A cheap, cloneable handle to a running link.
///
/// Only for sending commands — what the node says on its own goes to the
/// [`EventBus`], so that modules have a single place to listen.
#[derive(Debug, Clone)]
pub struct LinkHandle {
    commands: mpsc::Sender<Request>,
}

impl LinkHandle {
    /// Sends a command and waits for the node's answer.
    ///
    /// Commands are served one at a time in arrival order, because the node
    /// works that way; callers do not need to coordinate.
    pub async fn request(&self, frame: Vec<u8>) -> Result<Vec<u8>, LinkError> {
        let (reply, answer) = oneshot::channel();

        self.commands
            .send(Request { frame, reply })
            .await
            .map_err(|_| LinkError::Closed)?;

        // A dropped sender means the actor died before answering.
        answer.await.map_err(|_| LinkError::Closed)?
    }
}

/// Builds a link without starting it yet.
///
/// Returned so that listeners can subscribe **before** the first event is
/// published. The bus keeps no backlog, so a link that connects while the
/// modules are still starting would report a connection nobody hears — and the
/// state stays wrong until the next disconnect. Use [`spawn`] where that does
/// not matter, such as in tests.
pub fn prepare<T>(
    transport: T,
    config: LinkConfig,
    events: EventBus,
) -> (LinkHandle, PreparedLink<T>)
where
    T: Transport + 'static,
{
    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);

    let handle = LinkHandle {
        commands: commands_tx,
    };

    let actor = Actor {
        transport,
        commands: commands_rx,
        events,
        config,
        pending: None,
    };

    (handle, PreparedLink { actor })
}

/// A link that is built but not yet running.
pub struct PreparedLink<T> {
    actor: Actor<T>,
}

impl<T> PreparedLink<T>
where
    T: Transport + 'static,
{
    /// Starts the actor. From here on it connects and reports on the bus.
    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(self.actor.run())
    }
}

/// Starts a link over `transport` and returns its handle plus the actor task.
///
/// Everything the node reports goes to `events`, including when the connection
/// opens and closes — the state modules need in order to show whether the mesh
/// is reachable at all.
///
/// Starts immediately, so anything that must not miss the first event should
/// use [`prepare`] instead.
pub fn spawn<T>(transport: T, config: LinkConfig, events: EventBus) -> (LinkHandle, JoinHandle<()>)
where
    T: Transport + 'static,
{
    let (handle, prepared) = prepare(transport, config, events);
    (handle, prepared.start())
}

/// Owns the transport and is the only thing that touches it.
struct Actor<T> {
    transport: T,
    commands: mpsc::Receiver<Request>,
    events: EventBus,
    config: LinkConfig,
    /// A command that arrived while the link was down. Held rather than
    /// rejected, so a brief unplug does not turn into a failed request.
    pending: Option<Request>,
}

/// Why the serving loop stopped.
enum Interruption {
    /// The connection died; reopening it is worth a try.
    Disconnected {
        /// Whether a frame was actually exchanged before it died.
        ///
        /// Decides whether the backoff starts over. A connection that opens
        /// and immediately collapses — a failing cable, a node stuck in a
        /// reboot loop — must not reset it, or reconnecting becomes a busy
        /// loop that costs a CPU core and achieves nothing.
        made_progress: bool,
    },
    /// Every handle is gone, so there is nobody left to serve.
    NoHandlesLeft,
}

impl<T> Actor<T>
where
    T: Transport,
{
    async fn run(mut self) {
        let mut delay = self.config.reconnect.initial_delay;

        loop {
            if let Err(error) = self.transport.connect().await {
                tracing::warn!(%error, ?delay, "link could not connect, retrying");
                if !self.wait_before_retry(delay).await {
                    break;
                }
                delay = self.next_delay(delay);
                continue;
            }

            self.events.publish(AppEvent::NodeConnected);

            let interruption = self.serve_until_disconnected().await;
            self.events.publish(AppEvent::NodeDisconnected {
                reason: "connection to the node ended".to_owned(),
            });

            match interruption {
                Interruption::NoHandlesLeft => break,
                Interruption::Disconnected { made_progress } => {
                    // A connection that carried traffic earns a fresh budget;
                    // one that collapsed straight away does not.
                    if made_progress {
                        delay = self.config.reconnect.initial_delay;
                    }

                    // Wait even though opening worked: without this, a link
                    // that dies immediately would be reopened without pause.
                    if !self.wait_before_retry(delay).await {
                        break;
                    }
                    delay = self.next_delay(delay);
                }
            }
        }

        let _ = self.transport.disconnect().await;
    }

    /// Serves callers until the connection dies or nobody is left.
    async fn serve_until_disconnected(&mut self) -> Interruption {
        let mut made_progress = false;

        // A command that waited out the outage goes first.
        if let Some(request) = self.pending.take() {
            match self.serve(request).await {
                Ok(()) => made_progress = true,
                Err(()) => return Interruption::Disconnected { made_progress },
            }
        }

        loop {
            tokio::select! {
                // Prefer serving a caller over reading, so a queued command
                // does not wait behind an idle read.
                biased;

                command = self.commands.recv() => {
                    let Some(request) = command else {
                        return Interruption::NoHandlesLeft;
                    };
                    match self.serve(request).await {
                        Ok(()) => made_progress = true,
                        Err(()) => return Interruption::Disconnected { made_progress },
                    }
                }

                // Cancel-safe: the transport keeps its decoder buffer, and no
                // await sits between reading bytes and handing them over.
                frame = self.transport.recv() => {
                    match frame {
                        Ok(frame) => {
                            made_progress = true;
                            self.dispatch_unprompted(frame);
                        }
                        Err(error) => {
                            tracing::info!(%error, "link transport ended while idle");
                            return Interruption::Disconnected { made_progress };
                        }
                    }
                }
            }
        }
    }

    /// Waits out the backoff. Returns `false` if nobody is left to serve.
    ///
    /// Keeps listening while waiting, for two reasons: a command that arrives
    /// now should be served after reconnecting rather than rejected, and an
    /// actor whose handles are all gone must stop instead of retrying forever.
    async fn wait_before_retry(&mut self, delay: Duration) -> bool {
        let deadline = tokio::time::sleep(delay);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                () = &mut deadline => return true,

                // Only take a command while no other one is held; further
                // commands stay queued until this one has been served.
                command = self.commands.recv(), if self.pending.is_none() => {
                    match command {
                        Some(request) => self.pending = Some(request),
                        None => return false,
                    }
                }
            }
        }
    }

    /// The next backoff delay, never above the configured ceiling.
    fn next_delay(&self, current: Duration) -> Duration {
        current
            .saturating_mul(self.config.reconnect.factor)
            .min(self.config.reconnect.max_delay)
    }

    /// Sends one command and waits for its answer, forwarding pushes meanwhile.
    ///
    /// Returns `Err(())` when the transport died and the actor should stop.
    async fn serve(&mut self, request: Request) -> Result<(), ()> {
        if let Err(error) = self.transport.send(&request.frame).await {
            let fatal = matches!(error, TransportError::Disconnected { .. });
            let _ = request.reply.send(Err(LinkError::Transport(error)));
            return if fatal { Err(()) } else { Ok(()) };
        }

        let outcome = tokio::time::timeout(self.config.response_timeout, self.await_answer()).await;

        match outcome {
            Ok(Ok(answer)) => {
                let _ = request.reply.send(Ok(answer));
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = request.reply.send(Err(LinkError::Transport(error)));
                Err(())
            }
            Err(_elapsed) => {
                // The node may still answer late; that frame would then be
                // taken for the next command's answer. Ending the connection
                // is the honest way out of an unsynchronised exchange.
                tracing::warn!(
                    timeout = ?self.config.response_timeout,
                    "node did not answer in time, dropping the link"
                );
                let _ = request
                    .reply
                    .send(Err(LinkError::Timeout(self.config.response_timeout)));
                Err(())
            }
        }
    }

    /// Reads until a reply shows up, broadcasting any pushes on the way.
    async fn await_answer(&mut self) -> Result<Vec<u8>, TransportError> {
        loop {
            let frame = self.transport.recv().await?;

            match frame.first() {
                Some(&opcode) if opcode::is_push(opcode) => {
                    self.broadcast(frame);
                }
                Some(_) => return Ok(frame),
                // No opcode, so it cannot be anyone's answer.
                None => tracing::warn!("discarding an empty frame while awaiting a reply"),
            }
        }
    }

    /// Handles a frame that arrived without a command outstanding.
    fn dispatch_unprompted(&self, frame: Vec<u8>) {
        match frame.first() {
            Some(&opcode) if opcode::is_push(opcode) => self.broadcast(frame),
            // A reply with nothing to reply to: the exchange is out of step,
            // and silently keeping it would corrupt the next correlation.
            Some(opcode) => {
                tracing::warn!(opcode, "discarding a reply nobody asked for");
            }
            None => tracing::warn!("discarding an empty frame"),
        }
    }

    /// Puts a push on the bus for whoever is interested.
    fn broadcast(&self, frame: Vec<u8>) {
        self.events.publish(AppEvent::Push { payload: frame });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshdash_proto::opcode::{Command, Push, Response};
    use meshdash_transport::mock::{MockTransport, Step};

    fn frame(first: u8) -> Vec<u8> {
        vec![first]
    }

    fn emit(first: u8) -> Step {
        Step::Emit(frame(first))
    }

    /// Keeps the actor from ending while a test still expects it to serve.
    fn idle_after(mut script: Vec<Step>) -> Vec<Step> {
        script.push(Step::Drop("script finished".into()));
        script
    }

    #[tokio::test]
    async fn answers_a_command_with_the_nodes_reply() {
        let transport = MockTransport::new(idle_after(vec![emit(u8::from(Response::Ok))]));
        let sent = transport.sent_frames();
        let (link, _task) = spawn(transport, LinkConfig::default(), EventBus::new());

        let answer = link
            .request(frame(u8::from(Command::GetDeviceTime)))
            .await
            .unwrap();

        assert_eq!(Response::from(answer[0]), Response::Ok);
        assert_eq!(
            sent.snapshot(),
            vec![frame(u8::from(Command::GetDeviceTime))]
        );
    }

    #[tokio::test]
    async fn a_prepared_link_stays_quiet_until_started() {
        // The reason `prepare` exists: a listener that subscribes after the
        // link connected would never learn that it did, because the bus keeps
        // no backlog.
        let transport = MockTransport::new(idle_after(vec![]));
        let bus = EventBus::new();
        let (_link, prepared) = prepare(transport, brisk_reconnect(), bus.clone());

        // Subscribing late is safe as long as nothing has started.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut events = bus.subscribe();

        let _task = prepared.start();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .expect("expected an event")
                .unwrap(),
            AppEvent::NodeConnected,
            "a late subscriber must still see the first connection"
        );
    }

    #[tokio::test]
    async fn announces_that_the_node_is_reachable() {
        let transport = MockTransport::new(idle_after(vec![]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(events.recv().await.unwrap(), AppEvent::NodeConnected);
    }

    #[tokio::test]
    async fn announces_that_the_node_is_gone() {
        let transport = MockTransport::new(vec![Step::Drop("cable pulled".into())]);
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(events.recv().await.unwrap(), AppEvent::NodeConnected);
        assert!(matches!(
            events.recv().await.unwrap(),
            AppEvent::NodeDisconnected { .. }
        ));
    }

    #[tokio::test]
    async fn puts_pushes_on_the_bus() {
        let transport = MockTransport::new(idle_after(vec![emit(u8::from(Push::Advert))]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(events.recv().await.unwrap(), AppEvent::NodeConnected);
        assert_eq!(
            events.recv().await.unwrap(),
            AppEvent::Push {
                payload: vec![u8::from(Push::Advert)]
            }
        );
    }

    #[tokio::test]
    async fn forwards_a_push_that_arrives_while_a_command_is_waiting() {
        // The node announces a message before answering the command.
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Push::MsgWaiting)),
            emit(u8::from(Response::Ok)),
        ]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();
        let (link, _task) = spawn(transport, LinkConfig::default(), bus);

        let answer = link
            .request(frame(u8::from(Command::AppStart)))
            .await
            .unwrap();

        assert_eq!(
            Response::from(answer[0]),
            Response::Ok,
            "the push must not be mistaken for the answer"
        );

        assert_eq!(events.recv().await.unwrap(), AppEvent::NodeConnected);
        assert_eq!(
            events.recv().await.unwrap(),
            AppEvent::Push {
                payload: vec![u8::from(Push::MsgWaiting)]
            },
            "the push must reach the bus even though a command was in flight"
        );
    }

    #[tokio::test]
    async fn gives_up_on_a_silent_node() {
        // The script never answers; it only drops after a while.
        let transport = MockTransport::new(vec![Step::Drop("silent".into())]);
        let config = LinkConfig {
            response_timeout: Duration::from_millis(50),
            ..LinkConfig::default()
        };
        let (link, _task) = spawn(transport, config, EventBus::new());

        let error = link
            .request(frame(u8::from(Command::GetBattAndStorage)))
            .await;

        assert!(matches!(
            error,
            Err(LinkError::Timeout(_)) | Err(LinkError::Transport(_))
        ));
    }

    #[tokio::test]
    async fn serves_commands_one_after_another() {
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Response::Ok)),
            emit(u8::from(Response::SelfInfo)),
        ]));
        let (link, _task) = spawn(transport, LinkConfig::default(), EventBus::new());

        // Both are in flight at once; the node answers them in order.
        let first = link.request(frame(u8::from(Command::AppStart)));
        let second = link.request(frame(u8::from(Command::GetDeviceTime)));
        let (first, second) = tokio::join!(first, second);

        assert_eq!(Response::from(first.unwrap()[0]), Response::Ok);
        assert_eq!(Response::from(second.unwrap()[0]), Response::SelfInfo);
    }

    #[tokio::test]
    async fn reports_a_dropped_connection_to_the_waiting_caller() {
        let transport = MockTransport::new(vec![Step::Drop("cable pulled".into())]);
        let (link, _task) = spawn(transport, LinkConfig::default(), EventBus::new());

        let error = link.request(frame(u8::from(Command::AppStart))).await;

        assert!(matches!(error, Err(LinkError::Transport(_))));
    }

    /// A reconnect config with tiny delays, for tests that use paused time.
    fn brisk_reconnect() -> LinkConfig {
        LinkConfig {
            reconnect: ReconnectConfig {
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_millis(80),
                factor: 2,
            },
            ..LinkConfig::default()
        }
    }

    #[tokio::test]
    async fn carries_on_after_the_cable_is_pulled() {
        // Drops once, then serves again — as after replugging.
        let transport = MockTransport::new(idle_after(vec![
            Step::Drop("cable pulled".into()),
            emit(u8::from(Response::Ok)),
        ]));
        let (link, _task) = spawn(transport, brisk_reconnect(), EventBus::new());

        let answer = link.request(frame(u8::from(Command::AppStart))).await;

        // The command in flight when the link died still fails...
        assert!(answer.is_err());

        // ...but the link itself recovers and serves the next one.
        let answer = link
            .request(frame(u8::from(Command::GetDeviceTime)))
            .await
            .unwrap();
        assert_eq!(Response::from(answer[0]), Response::Ok);
    }

    #[tokio::test(start_paused = true)]
    async fn backs_off_further_with_every_failed_attempt() {
        // Three refusals, so two delays are applied before success.
        let transport =
            MockTransport::new(idle_after(vec![emit(u8::from(Response::Ok))])).failing_connects(3);
        let (link, _task) = spawn(transport, brisk_reconnect(), EventBus::new());

        let started = tokio::time::Instant::now();
        let answer = link
            .request(frame(u8::from(Command::AppStart)))
            .await
            .unwrap();
        let waited = started.elapsed();

        assert_eq!(Response::from(answer[0]), Response::Ok);
        // 10 + 20 + 40 ms of backoff, rather than 3 × 10 ms.
        assert!(
            waited >= Duration::from_millis(70),
            "delays must grow, waited only {waited:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn never_waits_longer_than_the_cap() {
        // Enough refusals that an uncapped backoff would run away.
        let transport =
            MockTransport::new(idle_after(vec![emit(u8::from(Response::Ok))])).failing_connects(6);
        let (link, _task) = spawn(transport, brisk_reconnect(), EventBus::new());

        let started = tokio::time::Instant::now();
        link.request(frame(u8::from(Command::AppStart)))
            .await
            .unwrap();
        let waited = started.elapsed();

        // Capped at 80 ms: 10+20+40+80+80+80 = 310 ms. Doubling unchecked
        // would already be 630 ms here.
        assert!(
            waited < Duration::from_millis(400),
            "backoff must be capped, waited {waited:?}"
        );
    }

    #[tokio::test]
    async fn stops_when_every_handle_is_gone() {
        let transport = MockTransport::new(idle_after(vec![emit(u8::from(Response::Ok))]));
        let (link, task) = spawn(transport, brisk_reconnect(), EventBus::new());

        drop(link);

        // Without this the actor would reconnect forever with nobody to serve.
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("actor must stop when nobody holds a handle")
            .unwrap();
    }

    #[tokio::test]
    async fn refuses_commands_once_the_actor_has_stopped() {
        let transport = MockTransport::new(vec![Step::Drop("gone".into())]);
        let (link, task) = spawn(transport, brisk_reconnect(), EventBus::new());

        // A lost connection no longer ends the actor — that is the point of
        // reconnecting — so end it from the outside to reach the closed state.
        task.abort();
        let _ = task.await;

        assert!(matches!(
            link.request(frame(u8::from(Command::AppStart))).await,
            Err(LinkError::Closed)
        ));
    }

    #[tokio::test]
    async fn keeps_running_when_the_connection_dies() {
        let transport = MockTransport::new(vec![Step::Drop("gone".into())]);
        let (link, task) = spawn(transport, brisk_reconnect(), EventBus::new());

        let _ = link.request(frame(u8::from(Command::AppStart))).await;

        // Still alive and retrying, rather than having given up.
        assert!(
            tokio::time::timeout(Duration::from_millis(200), task)
                .await
                .is_err(),
            "a dropped connection must not end the link"
        );
    }
}
