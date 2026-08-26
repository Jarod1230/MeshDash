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
//! # Some answers are a series
//!
//! Not every exchange is one question, one answer: the contact list arrives as
//! a start marker, one frame per contact and an end marker. Taking the first
//! frame as *the* answer would drop the rest, and the exchange would be out of
//! step from then on. [`LinkHandle::request_until`] collects until a caller
//! -supplied predicate says the series is complete.
//!
//! # Why this lives in the core, not in the transport
//!
//! Telling a push from a reply is protocol knowledge, and the transport crate
//! deliberately has none — it moves frames and knows nothing of their meaning.
//! Domain knowledge stays out of here too: this layer never learns what a node
//! or a message is.

use std::time::Duration;

use meshdash_proto::{command, opcode};
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

    /// The name this application announces at the start of a session.
    ///
    /// The node logs it and nothing more, but it is what an operator sees in
    /// the node's own log when wondering who is talking to it.
    pub app_name: String,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(5),
            reconnect: ReconnectConfig::default(),
            app_name: "MeshDash".to_owned(),
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

/// Decides when a multi-frame answer is complete.
///
/// Gets each non-push frame as it arrives; returning `true` ends the exchange
/// and that frame is included.
pub type IsFinal = Box<dyn Fn(&[u8]) -> bool + Send>;

/// One command waiting to be sent, with the channel for its answer.
struct Request {
    frame: Vec<u8>,
    /// `None` for an ordinary command, which ends after one frame.
    until: Option<IsFinal>,
    reply: oneshot::Sender<Result<Vec<Vec<u8>>, LinkError>>,
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
        let mut frames = self.send_request(frame, None).await?;

        // Exactly one frame, because `until` was not set.
        Ok(frames.remove(0))
    }

    /// Sends a command and collects answers until `is_final` says stop.
    ///
    /// Some exchanges answer with a series rather than a single frame — the
    /// contact list arrives as a start marker, one frame per contact and an end
    /// marker. Treating the first as *the* answer would drop the rest and leave
    /// the exchange out of step.
    ///
    /// The response timeout applies to each frame, not to the whole series: a
    /// long list must not fail merely for being long.
    pub async fn request_until(
        &self,
        frame: Vec<u8>,
        is_final: IsFinal,
    ) -> Result<Vec<Vec<u8>>, LinkError> {
        self.send_request(frame, Some(is_final)).await
    }

    /// Hands a request to the actor and waits for its answer.
    async fn send_request(
        &self,
        frame: Vec<u8>,
        until: Option<IsFinal>,
    ) -> Result<Vec<Vec<u8>>, LinkError> {
        let (reply, answer) = oneshot::channel();

        self.commands
            .send(Request {
                frame,
                until,
                reply,
            })
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

            // The session start goes out before anyone is told the node is
            // there. It is the only source for the node's own key, name and
            // position — and it resets a half-finished contact listing in the
            // firmware, so a module that starts fetching contacts on
            // NodeConnected would have its listing cut off if this came after.
            let session = self.start_session().await;

            self.events.publish(AppEvent::NodeConnected);

            if let Some(payload) = session {
                self.events.publish(AppEvent::SessionStarted { payload });
            }

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
    async fn serve(&mut self, mut request: Request) -> Result<(), ()> {
        if let Err(error) = self.transport.send(&request.frame).await {
            let fatal = matches!(error, TransportError::Disconnected { .. });
            let _ = request.reply.send(Err(LinkError::Transport(error)));
            return if fatal { Err(()) } else { Ok(()) };
        }

        let outcome = self.collect_answer(request.until.take()).await;

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

    /// Gathers the answer, one frame or a series.
    ///
    /// The timeout is per frame rather than for the whole exchange: a contact
    /// list of a hundred entries must not fail merely for being long.
    #[allow(clippy::type_complexity)]
    async fn collect_answer(
        &mut self,
        until: Option<IsFinal>,
    ) -> Result<Result<Vec<Vec<u8>>, TransportError>, tokio::time::error::Elapsed> {
        let mut collected = Vec::new();

        loop {
            let frame =
                match tokio::time::timeout(self.config.response_timeout, self.await_answer()).await
                {
                    Ok(Ok(frame)) => frame,
                    Ok(Err(error)) => return Ok(Err(error)),
                    Err(elapsed) => return Err(elapsed),
                };

            let done = match &until {
                Some(is_final) => is_final(&frame),
                // An ordinary command ends with its first reply.
                None => true,
            };

            collected.push(frame);

            if done {
                return Ok(Ok(collected));
            }
        }
    }

    /// Announces this application to the node and returns its answer.
    ///
    /// Failing is not fatal: a node that will not answer is still a node worth
    /// reporting as connected. What is lost is its self-description, and the
    /// modules that want it simply do not hear about it.
    async fn start_session(&mut self) -> Option<Vec<u8>> {
        let frame = match command::app_start(&self.config.app_name) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::error!(%error, "the application name does not fit a frame");
                return None;
            }
        };

        if let Err(error) = self.transport.send(&frame).await {
            tracing::warn!(%error, "could not start a session with the node");
            return None;
        }

        match tokio::time::timeout(self.config.response_timeout, self.await_answer()).await {
            Ok(Ok(answer)) => Some(answer),
            Ok(Err(error)) => {
                tracing::warn!(%error, "the node broke off during the session start");
                None
            }
            Err(_elapsed) => {
                tracing::warn!("the node did not answer the session start");
                None
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
    ///
    /// Every connection now begins with the session start the link sends by
    /// itself, so the script answers that first. Without it the link would
    /// wait out its response timeout before serving anything, and every
    /// caller's answer would be off by one frame.
    fn idle_after(script: Vec<Step>) -> Vec<Step> {
        let mut full = vec![Step::Emit(session_answer())];
        full.extend(script);
        full.push(Step::Drop("script finished".into()));
        full
    }

    /// A minimal `RESP_CODE_SELF_INFO`, enough to end the session start.
    fn session_answer() -> Vec<u8> {
        let mut payload = vec![0u8; 58];
        payload[0] = u8::from(Response::SelfInfo);
        payload
    }

    /// The next event that is not the link's own session start.
    ///
    /// Every connection now announces one, and no test below is about it.
    async fn next_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> AppEvent {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::SessionStarted { .. } => continue,
                other => return other,
            }
        }
    }

    /// What a test sent, without the session start the link adds.
    fn commands(sent: &meshdash_transport::mock::SentFrames) -> Vec<Vec<u8>> {
        sent.snapshot().into_iter().skip(1).collect()
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
            commands(&sent),
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
    async fn starts_a_session_before_anyone_is_told_the_node_is_there() {
        // CMD_APP_START is the only way to learn the node's own key, name and
        // position — and it resets a half-finished contact listing
        // (`_iter_started = false` in MyMesh.cpp). A module that starts
        // fetching contacts on NodeConnected would have its listing cut off
        // if this went out afterwards.
        let transport = MockTransport::new(idle_after(vec![]));
        let sent = transport.sent_frames();
        let bus = EventBus::new();
        let (_link, prepared) = prepare(transport, brisk_reconnect(), bus.clone());
        let mut events = bus.subscribe();
        let _task = prepared.start();

        let first = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("expected an event")
            .unwrap();
        let second = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("expected a second event")
            .unwrap();

        assert_eq!(first, AppEvent::NodeConnected);
        assert_eq!(
            second,
            AppEvent::SessionStarted {
                payload: session_answer()
            }
        );
        assert_eq!(
            sent.snapshot().first().map(|frame| frame[0]),
            Some(u8::from(Command::AppStart)),
            "the session start must be the first thing on the wire"
        );
    }

    #[tokio::test]
    async fn a_node_that_will_not_start_a_session_is_still_announced() {
        // Knowing the node is reachable is worth more than knowing its name.
        let transport = MockTransport::new(idle_after(vec![]));
        let bus = EventBus::new();
        let (_link, prepared) = prepare(transport, brisk_reconnect(), bus.clone());
        let mut events = bus.subscribe();
        let _task = prepared.start();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), events.recv())
                .await
                .expect("expected an event")
                .unwrap(),
            AppEvent::NodeConnected
        );
    }

    #[tokio::test]
    async fn announces_that_the_node_is_reachable() {
        let transport = MockTransport::new(idle_after(vec![]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(next_event(&mut events).await, AppEvent::NodeConnected);
    }

    #[tokio::test]
    async fn announces_that_the_node_is_gone() {
        let transport = MockTransport::new(vec![Step::Drop("cable pulled".into())]);
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(next_event(&mut events).await, AppEvent::NodeConnected);
        assert!(matches!(
            next_event(&mut events).await,
            AppEvent::NodeDisconnected { .. }
        ));
    }

    #[tokio::test]
    async fn puts_pushes_on_the_bus() {
        let transport = MockTransport::new(idle_after(vec![emit(u8::from(Push::Advert))]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();

        let (_link, _task) = spawn(transport, brisk_reconnect(), bus);

        assert_eq!(next_event(&mut events).await, AppEvent::NodeConnected);
        assert_eq!(
            next_event(&mut events).await,
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

        assert_eq!(next_event(&mut events).await, AppEvent::NodeConnected);
        assert_eq!(
            next_event(&mut events).await,
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
    async fn collects_a_series_of_answers() {
        // The shape of a contact list: a start marker, entries, an end marker.
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Response::ContactsStart)),
            emit(u8::from(Response::Contact)),
            emit(u8::from(Response::Contact)),
            emit(u8::from(Response::EndOfContacts)),
        ]));
        let (link, _task) = spawn(transport, LinkConfig::default(), EventBus::new());

        let frames = link
            .request_until(
                frame(u8::from(Command::GetContacts)),
                Box::new(|frame: &[u8]| Response::from(frame[0]) == Response::EndOfContacts),
            )
            .await
            .unwrap();

        assert_eq!(frames.len(), 4, "start, two contacts and the end marker");
        assert_eq!(
            Response::from(frames[3][0]),
            Response::EndOfContacts,
            "the closing frame belongs to the answer"
        );
    }

    #[tokio::test]
    async fn forwards_pushes_during_a_series() {
        // A node may announce something halfway through a listing.
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Response::ContactsStart)),
            emit(u8::from(Push::MsgWaiting)),
            emit(u8::from(Response::EndOfContacts)),
        ]));
        let bus = EventBus::new();
        let mut events = bus.subscribe();
        let (link, _task) = spawn(transport, LinkConfig::default(), bus);

        let frames = link
            .request_until(
                frame(u8::from(Command::GetContacts)),
                Box::new(|frame: &[u8]| Response::from(frame[0]) == Response::EndOfContacts),
            )
            .await
            .unwrap();

        assert_eq!(frames.len(), 2, "the push is not part of the answer");
        assert_eq!(next_event(&mut events).await, AppEvent::NodeConnected);
        assert_eq!(
            next_event(&mut events).await,
            AppEvent::Push {
                payload: vec![u8::from(Push::MsgWaiting)]
            }
        );
    }

    #[tokio::test]
    async fn an_ordinary_command_still_ends_after_one_frame() {
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Response::Ok)),
            emit(u8::from(Response::SelfInfo)),
        ]));
        let (link, _task) = spawn(transport, LinkConfig::default(), EventBus::new());

        let answer = link
            .request(frame(u8::from(Command::AppStart)))
            .await
            .unwrap();

        assert_eq!(Response::from(answer[0]), Response::Ok);
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
            // Every connection starts a session, the second one included.
            Step::Emit(session_answer()),
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
