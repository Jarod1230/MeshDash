//! The actor that owns the connection to a node.
//!
//! A companion node answers commands one at a time, in order, and there is no
//! request tag to match an answer to its question. The link makes that
//! manageable: callers hand over a command and await their answer, while
//! everything the node says on its own is broadcast to whoever is listening.
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
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};

/// How many pushes are buffered for a slow subscriber before it misses some.
const PUSH_BUFFER: usize = 256;

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
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(5),
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
#[derive(Debug, Clone)]
pub struct LinkHandle {
    commands: mpsc::Sender<Request>,
    pushes: broadcast::Sender<Vec<u8>>,
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

    /// Listens to everything the node says unprompted.
    ///
    /// Only frames arriving **after** this call are delivered — there is no
    /// backlog, so anything the node said earlier is gone. Subscribe before the
    /// link gets busy if that matters.
    ///
    /// A subscriber that falls too far behind loses the oldest frames rather
    /// than stalling the link.
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.pushes.subscribe()
    }
}

/// Starts a link over `transport` and returns its handle plus the actor task.
pub fn spawn<T>(transport: T, config: LinkConfig) -> (LinkHandle, JoinHandle<()>)
where
    T: Transport + 'static,
{
    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);
    let (pushes_tx, _) = broadcast::channel(PUSH_BUFFER);

    let handle = LinkHandle {
        commands: commands_tx,
        pushes: pushes_tx.clone(),
    };

    let actor = Actor {
        transport,
        commands: commands_rx,
        pushes: pushes_tx,
        config,
    };

    (handle, tokio::spawn(actor.run()))
}

/// Owns the transport and is the only thing that touches it.
struct Actor<T> {
    transport: T,
    commands: mpsc::Receiver<Request>,
    pushes: broadcast::Sender<Vec<u8>>,
    config: LinkConfig,
}

impl<T> Actor<T>
where
    T: Transport,
{
    async fn run(mut self) {
        if let Err(error) = self.transport.connect().await {
            tracing::warn!(%error, "link could not open its transport");
            return;
        }

        loop {
            tokio::select! {
                // Prefer serving a caller over reading, so a queued command
                // does not wait behind an idle read.
                biased;

                command = self.commands.recv() => {
                    let Some(request) = command else {
                        // Every handle is gone; nobody can ask for anything.
                        break;
                    };
                    if self.serve(request).await.is_err() {
                        break;
                    }
                }

                // Cancel-safe: the transport keeps its decoder buffer, and no
                // await sits between reading bytes and handing them over.
                frame = self.transport.recv() => {
                    match frame {
                        Ok(frame) => self.dispatch_unprompted(frame),
                        Err(error) => {
                            tracing::info!(%error, "link transport ended while idle");
                            break;
                        }
                    }
                }
            }
        }

        let _ = self.transport.disconnect().await;
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

    /// Sends a push to every subscriber, if there is any.
    fn broadcast(&self, frame: Vec<u8>) {
        // An error only means nobody is listening, which is not a problem.
        let _ = self.pushes.send(frame);
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
        let (link, _task) = spawn(transport, LinkConfig::default());

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
    async fn forwards_a_push_that_arrives_while_a_command_is_waiting() {
        // The node announces a message before answering the command.
        let transport = MockTransport::new(idle_after(vec![
            emit(u8::from(Push::MsgWaiting)),
            emit(u8::from(Response::Ok)),
        ]));
        let (link, _task) = spawn(transport, LinkConfig::default());
        let mut pushes = link.subscribe();

        let answer = link
            .request(frame(u8::from(Command::AppStart)))
            .await
            .unwrap();

        assert_eq!(
            Response::from(answer[0]),
            Response::Ok,
            "the push must not be mistaken for the answer"
        );
        assert_eq!(
            Push::from(pushes.recv().await.unwrap()[0]),
            Push::MsgWaiting
        );
    }

    #[tokio::test]
    async fn broadcasts_pushes_that_arrive_unprompted() {
        let transport = MockTransport::new(idle_after(vec![emit(u8::from(Push::Advert))]));
        let (link, _task) = spawn(transport, LinkConfig::default());
        // Safe before the first await: the actor has not run yet, so nothing
        // has been broadcast that this subscriber could miss.
        let mut pushes = link.subscribe();

        assert_eq!(Push::from(pushes.recv().await.unwrap()[0]), Push::Advert);
    }

    #[tokio::test]
    async fn gives_up_on_a_silent_node() {
        // The script never answers; it only drops after a while.
        let transport = MockTransport::new(vec![Step::Drop("silent".into())]);
        let config = LinkConfig {
            response_timeout: Duration::from_millis(50),
        };
        let (link, _task) = spawn(transport, config);

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
        let (link, _task) = spawn(transport, LinkConfig::default());

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
        let (link, _task) = spawn(transport, LinkConfig::default());

        let error = link.request(frame(u8::from(Command::AppStart))).await;

        assert!(matches!(error, Err(LinkError::Transport(_))));
    }

    #[tokio::test]
    async fn refuses_commands_once_the_actor_has_stopped() {
        let transport = MockTransport::new(vec![Step::Drop("gone".into())]);
        let (link, task) = spawn(transport, LinkConfig::default());

        // Let the actor finish after its transport died.
        let _ = link.request(frame(u8::from(Command::AppStart))).await;
        task.await.unwrap();

        assert!(matches!(
            link.request(frame(u8::from(Command::AppStart))).await,
            Err(LinkError::Closed)
        ));
    }
}
