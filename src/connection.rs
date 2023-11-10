#[cfg(feature = "voice")]
use std::collections::HashMap;
use std::mem;
use std::pin::Pin;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;
use websockets::WebSocket;

use crate::io::{GatewayEventStream, SharedSink};
use crate::io::{JsonSink, JsonStream, JsonStreamError};
#[cfg(feature = "voice")]
use crate::voice::VoiceConnection;
use crate::{model::*, Discord};
use crate::{Error, Result};

const GATEWAY_VERSION: u64 = 6;

#[derive(Clone)]
pub struct ConnectionBuilder<'a> {
    base_url: String,
    token: &'a str,

    //large_threshold: Option<u32>,
    shard: Option<[u8; 2]>,
    intents: Option<Intents>,
    // TODO: presence
}

impl<'a> ConnectionBuilder<'a> {
    pub(crate) fn new(base_url: String, token: &'a str) -> Self {
        ConnectionBuilder {
            base_url,
            token,
            //large_threshold: None,
            shard: None,
            intents: None,
        }
    }

    /// Connect to only a specific shard.
    ///
    /// The `shard_id` is indexed at 0 while `total_shards` is indexed at 1.
    pub fn sharding(&mut self, shard_id: u8, total_shards: u8) -> &mut Self {
        self.shard = Some([shard_id, total_shards]);
        self
    }

    pub fn intents(&mut self, intents: Intents) -> &mut Self {
        self.intents = Some(intents);
        self
    }

    /// Establish a websocket connection over which events can be received.
    ///
    /// Also returns the `ReadyEvent` sent by Discord upon establishing the
    /// connection, which contains the initial state as seen by the client.
    pub async fn connect(&self) -> Result<(Connection, ReadyEvent)> {
        let mut d = json! {{
            "token": self.token,
            "properties": {
                "$os": ::std::env::consts::OS,
                "$browser": "Discord library for Rust",
                "$device": "discord-tokio",
                "$referring_domain": "",
                "$referrer": "",
            },
            "large_threshold": 250,
            "compress": true,
            "v": GATEWAY_VERSION,
        }};
        if let Some(info) = self.shard {
            d["shard"] = json![[info[0], info[1]]];
        }
        if let Some(intents) = self.intents {
            d["intents"] = intents.bits().into();
        }
        let identify = json! {{
            "op": 2,
            "d": d
        }};
        Connection::establish_connection(&self.base_url, self.token.clone(), identify).await
    }
}

/// An active WebSocket connection to the Discord gateway.
#[derive(Debug)]
pub struct Connection {
    /// Receiver of raw JSON events from the Discord gateway.
    receiver: GatewayEventStream,
    /// Shared sender for sending presence updates and the similar.
    sender: SharedSink<JsonSink, Value>,

    /// Channel for sending message sequence numbers.
    sequence_send: mpsc::Sender<u64>,
    /// The latest sequence number received.
    last_sequence: u64,

    /// Shutdown handle to the heartbeat task.
    ///
    /// This field is always `Some` unless if in the middle of
    /// a shutdown or reconnect operation.
    shutdown_heartbeat: Option<oneshot::Sender<()>>,

    #[cfg(feature = "voice")]
    voice_handles: HashMap<Option<ServerId>, VoiceConnection>,

    /// The user ID of the user logged in the Voice Gateway?
    #[cfg(feature = "voice")]
    user_id: UserId,

    /// The ID of this current session.
    ///
    /// Used to reconnect to Discord and continue event sending where we left off.
    session_id: Option<String>,

    /// How to reconnect to Discord
    reconnect: ReconnectData,
}

impl Connection {
    /// Establish a connection to the Discord websocket servers.
    ///
    /// Returns both the `Connection` and the `ReadyEvent` which is always the
    /// first event received and contains initial state information.
    ///
    /// Usually called internally by `Discord::connect`, which provides both
    /// the token and URL and an optional user-given shard ID and total shard
    /// count.
    pub async fn new(
        base_url: &str,
        token: &str,
        shard: Option<[u8; 2]>,
    ) -> Result<(Connection, ReadyEvent)> {
        ConnectionBuilder {
            shard,
            ..ConnectionBuilder::new(base_url.to_owned(), token)
        }
        .connect()
        .await
    }

    /// Establish a connection to Discord
    async fn establish_connection(
        base_url: &str,
        token: &str,
        identify: serde_json::Value,
    ) -> Result<(Connection, ReadyEvent)> {
        trace!("Gateway: {}", base_url);

        // establish the websocket connection
        let url = build_gateway_url(base_url);
        let ws = WebSocket::connect(&url).await?;

        let (receiver, sender) = ws.split();
        let mut receiver = GatewayEventStream::new(JsonStream::<Value>::new(receiver));
        let mut sender = JsonSink::new(sender);

        // send the handshake
        sender.send(&identify).await.map_err(|err| match err {
            JsonStreamError::Ws(ws) => Error::WebSocket(ws),
            JsonStreamError::Json(json) => Error::Json(json),
        })?;

        // read the Hello and spawn the keepalive thread
        let hello = match receiver.next().await {
            Some(Ok(event)) => event,
            Some(Err(err)) => return Err(err),
            None => {
                return Err(Error::WebSocket(
                    websockets::WebSocketError::WebSocketClosedError,
                ))
            }
        };

        let heartbeat_interval = match hello {
            GatewayEvent::Hello(interval) => interval,
            other => {
                debug!("Unexpected event: {:?}", other);
                return Err(Error::Protocol("Expected Hello during handshake"));
            }
        };

        let last_sequence;

        let mut shared_sender = SharedSink::new(sender);
        let (sequence_send, sequence_recv) = mpsc::channel(16);
        let (shutdown_send, shutdown_recv) = oneshot::channel();

        tokio::spawn(heartbeat(
            Duration::from_millis(heartbeat_interval),
            shared_sender.clone(),
            sequence_recv,
            shutdown_recv,
        ));

        // read the Ready event
        let ready = match receiver.next().await {
            Some(Ok(event)) => event,
            Some(Err(err)) => return Err(err),
            None => {
                return Err(Error::WebSocket(
                    websockets::WebSocketError::WebSocketClosedError,
                ))
            }
        };

        let ready = match ready {
            GatewayEvent::Dispatch(seq, Event::Ready(event)) => {
                let _ = sequence_send.send(seq);
                last_sequence = seq;
                event
            }
            GatewayEvent::InvalidateSession => {
                debug!("Session invalidated, reidentifying");

                shared_sender.send(identify.clone()).await?;

                let event = match receiver.next().await {
                    Some(Ok(event)) => event,
                    Some(Err(err)) => return Err(err),
                    None => {
                        return Err(Error::WebSocket(
                            websockets::WebSocketError::WebSocketClosedError,
                        ))
                    }
                };

                match event {
                    GatewayEvent::Dispatch(seq, Event::Ready(ready)) => {
                        let _ = sequence_send.send(seq).await;
                        last_sequence = seq;
                        ready
                    }
                    GatewayEvent::InvalidateSession => {
                        return Err(Error::Protocol(
                            "Invalid session during handshake. \
                            Double-check your token or consider waiting 5 seconds between starting shards.",
                        ))
                    }
                    other => {
                        debug!("Unexpected event: {:?}", other);
                        return Err(Error::Protocol("Expected Ready during handshake"));
                    }
                }
            }
            other => {
                debug!("Unexpected event: {:?}", other);
                return Err(Error::Protocol(
                    "Expected Ready or InvalidateSession during handshake",
                ));
            }
        };

        if ready.version != GATEWAY_VERSION {
            warn!(
                "Got protocol version {} instead of {}",
                ready.version, GATEWAY_VERSION
            );
        }
        let session_id = ready.session_id.clone();

        // return the connection
        Ok((
            Connection {
                receiver: receiver,
                sender: shared_sender,

                sequence_send,
                last_sequence,

                shutdown_heartbeat: Some(shutdown_send),

                session_id: Some(session_id),
                // voice only
                user_id: ready.user.id,
                voice_handles: HashMap::new(),

                reconnect: ReconnectData {
                    url,
                    token: token.to_owned(),
                    identify,
                },
            },
            ready,
        ))
    }

    /// Change the game information that this client reports as playing.
    pub async fn set_game(&mut self, game: Option<Game>) -> Result<()> {
        self.set_presence(game, OnlineStatus::Online, false).await
    }

    /// Set the client to be playing this game, with defaults used for any
    /// extended information.
    pub async fn set_game_name(&mut self, name: String) -> Result<()> {
        self.set_presence(Some(Game::playing(name)), OnlineStatus::Online, false)
            .await
    }

    /// Sets the active presence of the client, including game and/or status
    /// information.
    ///
    /// `afk` will help Discord determine where to send notifications.
    pub async fn set_presence(
        &mut self,
        game: Option<Game>,
        status: OnlineStatus,
        afk: bool,
    ) -> Result<()> {
        let status = match status {
            OnlineStatus::Offline => OnlineStatus::Invisible,
            other => other,
        };
        let game = match game {
            Some(Game {
                kind: GameType::Streaming,
                url: Some(url),
                name,
            }) => json! {{ "type": GameType::Streaming, "url": url, "name": name }},
            Some(game) => json! {{ "name": game.name, "type": GameType::Playing }},
            None => json!(null),
        };

        let presence_update = json! {{
            "op": 3,
            "d": {
                "afk": afk,
                "since": 0,
                "status": status,
                "game": game,
            }
        }};

        self.sender.send(presence_update).await?;

        Ok(())
    }

    /// Get a handle to the voice connection for a server.
    ///
    /// Pass `None` to get the handle for group and one-on-one calls.
    #[cfg(feature = "voice")]
    pub fn voice(&mut self, server_id: Option<ServerId>) -> &mut VoiceConnection {
        let Connection {
            ref mut voice_handles,
            user_id,
            ref keepalive_channel,
            ..
        } = *self;
        voice_handles.entry(server_id).or_insert_with(|| {
            VoiceConnection::__new(server_id, user_id, keepalive_channel.clone())
        })
    }

    /// Drop the voice connection for a server, forgetting all settings.
    ///
    /// Calling `.voice(server_id).disconnect()` will disconnect from voice but retain the mute
    /// and deaf status, audio source, and audio receiver.
    ///
    /// Pass `None` to drop the connection for group and one-on-one calls.
    #[cfg(feature = "voice")]
    pub fn drop_voice(&mut self, server_id: Option<ServerId>) {
        self.voice_handles.remove(&server_id);
    }

    /// Receive an event over the websocket, blocking until one is available.
    pub async fn recv_event(&mut self) -> Result<Event> {
        loop {
            match self.receiver.recv_json(GatewayEvent::decode) {
                Err(Error::WebSocket(err)) => {
                    warn!("Websocket error, reconnecting: {:?}", err);
                    // Try resuming if we haven't received an InvalidateSession
                    if let Some(session_id) = self.session_id.clone() {
                        match self.resume(session_id).await {
                            Ok(event) => return Ok(event),
                            Err(e) => debug!("Failed to resume: {:?}", e),
                        }
                    }
                    // If resuming didn't work, reconnect
                    return self.reconnect().await.map(Event::Ready);
                }
                Err(Error::Closed(num, message)) => {
                    debug!("Closure, reconnecting: {:?}: {}", num, message);
                    // Try resuming if we haven't received a 4006 or an InvalidateSession
                    if num != Some(4006) {
                        if let Some(session_id) = self.session_id.clone() {
                            match self.resume(session_id).await {
                                Ok(event) => return Ok(event),
                                Err(e) => debug!("Failed to resume: {:?}", e),
                            }
                        }
                    }
                    // If resuming didn't work, reconnect
                    return self.reconnect().await.map(Event::Ready);
                }
                Err(error) => return Err(error),
                Ok(GatewayEvent::Hello(interval)) => {
                    debug!("Mysterious late-game hello: {}", interval);
                }
                Ok(GatewayEvent::Dispatch(sequence, event)) => {
                    self.last_sequence = sequence;
                    let _ = self.keepalive_channel.send(Status::Sequence(sequence));
                    #[cfg(feature = "voice")]
                    {
                        if let Event::VoiceStateUpdate(server_id, ref voice_state) = event {
                            self.voice(server_id).__update_state(voice_state);
                        }
                        if let Event::VoiceServerUpdate {
                            server_id,
                            ref endpoint,
                            ref token,
                            ..
                        } = event
                        {
                            self.voice(server_id).__update_server(endpoint, token);
                        }
                    }
                    return Ok(event);
                }
                Ok(GatewayEvent::Heartbeat(sequence)) => {
                    debug!("Heartbeat received with seq {}", sequence);
                    let map = json! {{
                        "op": 1,
                        "d": sequence,
                    }};
                    let _ = self.keepalive_channel.send(Status::SendMessage(map));
                }
                Ok(GatewayEvent::HeartbeatAck) => {}
                Ok(GatewayEvent::Reconnect) => {
                    return self.reconnect().await.map(Event::Ready);
                }
                Ok(GatewayEvent::InvalidateSession) => {
                    debug!("Session invalidated, reidentifying");
                    self.session_id = None;
                    let _ = self
                        .keepalive_channel
                        .send(Status::SendMessage(self.identify.clone()));
                }
            }
        }
    }

    /// Reconnect after receiving an OP7 RECONNECT,
    /// and replace the current connection.
    ///
    /// This method *does not try to resume*, instead re-identifying
    /// and wasting network bandwidth by refetching all visible discord state.
    async fn reconnect(&mut self) -> Result<ReadyEvent> {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        self.shutdown_heartbeat
            .take()
            .unwrap_or_else(|| unreachable!())
            .send(())
            .expect("Could not stop the keepalive task, there will be a task leak.");

        trace!("Reconnecting...");

        // Make two attempts on the current known gateway URL
        for _ in 0..2 {
            let reconnect = Connection::establish_connection(
                &self.reconnect.url,
                &self.reconnect.token,
                self.reconnect.identify.clone(),
            )
            .await;

            if let Ok((conn, ready)) = reconnect {
                mem::replace(self, conn).raw_shutdown();

                self.session_id = Some(ready.session_id.clone());

                return Ok(ready);
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }

        // If those fail, hit REST for a new endpoint
        let url = Discord::from_token_raw(self.reconnect.token.to_owned())
            .get_gateway_url()
            .await?;

        let (conn, ready) = Connection::establish_connection(
            &url,
            &self.reconnect.token,
            self.reconnect.identify.clone(),
        )
        .await?;

        mem::replace(self, conn).raw_shutdown();

        self.session_id = Some(ready.session_id.clone());

        Ok(ready)
    }

    /// Resume using our existing session *and connection*.
    /// Consider reconnecting through the `todo!()` method before resuming.
    ///
    /// https://discord.com/developers/docs/topics/gateway#resuming
    async fn resume(&mut self, session_id: String) -> Result<Event> {
        trace!("Resuming...");

        // close connection and re-establish
        let _ = self
            .shutdown_heartbeat
            .take()
            .unwrap_or_else(|| unreachable!())
            .send(());

        let url = build_gateway_url(&self.reconnect.url);

        let ws = WebSocket::connect(&url).await?;
        let (mut receiver, sender) = ws.split();

        let mut sender = JsonSink::new(sender);

        // send the resume request
        let resume = json! {{
            "op": 6,
            "d": {
                "seq": self.last_sequence,
                "token": self.reconnect.token,
                "session_id": session_id,
            }
        }};

        sender.send(resume).await?;

        let first_event;
        loop {
            match receiver.recv_json(GatewayEvent::decode)? {
                GatewayEvent::Hello(interval) => {
                    let _ = self
                        .keepalive_channel
                        .send(Status::ChangeInterval(interval));
                }
                GatewayEvent::Dispatch(seq, event) => {
                    if let Event::Resumed { .. } = event {
                        trace!("Resumed successfully");
                    }
                    if let Event::Ready(ReadyEvent { ref session_id, .. }) = event {
                        self.session_id = Some(session_id.clone());
                    }
                    self.last_sequence = seq;
                    first_event = event;
                    break;
                }
                GatewayEvent::InvalidateSession => {
                    debug!("Session invalidated in resume, reidentifying");
                    sender.send_json(&self.identify)?;
                }
                other => {
                    debug!("Unexpected event: {:?}", other);
                    return Err(Error::Protocol("Unexpected event during resume"));
                }
            }
        }

        // switch everything to the new connection
        self.receiver = receiver;
        let _ = self.keepalive_channel.send(Status::ChangeSender(sender));
        Ok(first_event)
    }

    /// Cleanly shut down the websocket connection. Optional.
    pub fn shutdown(mut self) -> Result<()> {
        todo!()
    }

    // called from shutdown() and drop()
    fn inner_shutdown(&mut self) -> Result<()> {
        todo!()
    }

    // called when we want to drop the connection with no fanfare
    fn raw_shutdown(mut self) {
        use std::io::Write;
        {
            let stream = self.receiver.get_mut().get_mut();
            let _ = stream.flush();
            let _ = stream.shutdown(::std::net::Shutdown::Both);
        }
        ::std::mem::forget(self); // don't call inner_shutdown()
    }

    /// Requests a download of online member lists.
    ///
    /// It is recommended to avoid calling this method until the online member list
    /// is actually needed, especially for large servers, in order to save bandwidth
    /// and memory.
    ///
    /// Can be used with `State::all_servers`.
    pub async fn sync_servers(&mut self, servers: &[ServerId]) -> Result<()> {
        let msg = json! {{
            "op": 12,
            "d": servers,
        }};

        self.sender.send(msg).await?;

        Ok(())
    }

    /// Request a synchronize of active calls for the specified channels.
    ///
    /// Can be used with `State::all_private_channels`.
    pub async fn sync_calls(&mut self, channels: &[ChannelId]) -> Result<()> {
        for &channel in channels {
            let msg = json! {{
                "op": 13,
                "d": { "channel_id": channel }
            }};

            self.sender.feed(msg).await?;
        }

        self.sender.flush().await?;

        Ok(())
    }

    /// Requests a download of all member information for large servers.
    ///
    /// The members lists are cleared on call, and then refilled as chunks are received. When
    /// `unknown_members()` returns 0, the download has completed.
    pub fn download_all_members(&mut self, state: &mut crate::State) {
        if state.unknown_members() == 0 {
            return;
        }
        let servers = state.__download_members();
        let msg = json! {{
            "op": 8,
            "d": {
                "guild_id": servers,
                "query": "",
                "limit": 0,
            }
        }};
        let _ = self.keepalive_channel.send(Status::SendMessage(msg));
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Swallow errors
        let _ = self.inner_shutdown();
    }
}

#[inline]
fn build_gateway_url(base: &str) -> String {
    format!("{}?v={}", base, GATEWAY_VERSION)
}

/// Spawns a future that sends heartbeats at the given interval.
///
/// # Stopping
///
/// Stopping execution of the `heartbeat` task, either because the connection
/// has been lost, or the application is shutting down can be done via
/// `todo!()`
async fn heartbeat(
    interval: Duration,
    mut sink: SharedSink<JsonSink, Value>,
    mut sequence: mpsc::Receiver<u64>,
    mut shutdown: oneshot::Receiver<()>,
) {
    let mut interval = tokio::time::interval(interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut shutdown = Pin::new(&mut shutdown);

    let mut last_sequence = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = &mut shutdown => break
        };

        // receive the latest sequence number
        while let Ok(num) = sequence.try_recv() {
            last_sequence = num;
        }

        let map = json! {{
            "op": 1,
            "d": last_sequence
        }};

        match sink.send(map).await {
            Err(e) => warn!("Error sending gateway keeaplive: {:?}", e),
            _ => {}
        }
    }
}

/// Instructions for how to reconnect.
///
/// Contains the Gateway URL and the login payload.
#[derive(Debug)]
struct ReconnectData {
    /// The URL of the Discord Gateway.
    pub url: String,
    /// The token used to sign in to Discord.
    pub token: String,

    /// The complete identify payload used when logging in.
    pub identify: Value,
}
