//! Voice communication module.
//!
//! A `VoiceConnection` for a server is obtained from a `Connection`. It can then be used to
//! join a channel, change mute/deaf status, and play and receive audio.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::UdpSocket;
use std::sync::mpsc;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use opus;
use serde_json;
use sodiumoxide::crypto::secretbox as crypto;
use websocket::client::{Client, Sender};
use websocket::stream::WebSocketStream;

use crate::model::*;
use crate::{Error, Result};

/// An active or inactive voice connection, obtained from `Connection::voice`.
#[derive(Debug)]
pub struct VoiceConnection {
    // primary WS send control
    server_id: Option<ServerId>, // None for group and private calls
    user_id: UserId,
    main_ws: mpsc::Sender<crate::internal::Status>,
    channel_id: Option<ChannelId>,
    mute: bool,
    deaf: bool,

    // main WS receive control
    session_id: Option<String>,
    endpoint_token: Option<(String, String)>,

    // voice thread (voice WS + UDP) control
    sender: mpsc::Sender<Status>,
}

/// A readable audio source.
pub trait AudioSource: Send {
    /// Called each frame to determine if the audio source is stereo.
    ///
    /// This value should change infrequently; changing it will reset the encoder state.
    fn is_stereo(&mut self) -> bool;

    /// Called each frame when more audio is required.
    ///
    /// Samples should be supplied at 48000Hz, and if `is_stereo` returned true, the channels
    /// should be interleaved, left first.
    ///
    /// The result should normally be `Some(N)`, where `N` is the number of samples written to the
    /// buffer. The rest of the buffer is zero-filled; the whole buffer must be filled each call
    /// to avoid audio interruptions.
    ///
    /// If `Some(0)` is returned, no audio will be sent this frame, but the audio source will
    /// remain active. If `None` is returned, the audio source is considered to have ended, and
    /// `read_frame` will not be called again.
    fn read_frame(&mut self, buffer: &mut [i16]) -> Option<usize>;
}

/// A receiver for incoming audio.
pub trait AudioReceiver: Send {
    /// Called when a user's currently-speaking state has updated.
    ///
    /// This method is the only way to know the `ssrc` to `user_id` mapping, but is unreliable and
    /// only a hint for when users are actually speaking, due both to latency differences and that
    /// it is possible for a user to leave `speaking` true even when they are not sending audio.
    fn speaking_update(&mut self, ssrc: u32, user_id: UserId, speaking: bool);

    /// Called when a voice packet is received.
    ///
    /// The sequence number increases by one per packet sent, and can be used to reorder packets
    /// if they have been received out of order. The timestamp increases at 48000Hz (typically by
    /// 960 per 20ms frame). If `stereo` is true, the length of the `data` slice is doubled and
    /// samples have been interleaved. The typical length of `data` is 960 or 1920 for a 20ms frame,
    /// but may be larger or smaller in some situations.
    fn voice_packet(
        &mut self,
        ssrc: u32,
        sequence: u16,
        timestamp: u32,
        stereo: bool,
        data: &[i16],
    );
}

impl VoiceConnection {
    #[doc(hidden)]
    pub fn __new(
        server_id: Option<ServerId>,
        user_id: UserId,
        main_ws: mpsc::Sender<crate::internal::Status>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        start_voice_thread(server_id, rx);
        VoiceConnection {
            server_id: server_id,
            user_id: user_id,
            main_ws: main_ws,
            channel_id: None,
            mute: false,
            deaf: false,
            session_id: None,
            endpoint_token: None,
            sender: tx,
        }
    }

    /// Connect to the specified voice channel. Any previous channel on this server will be
    /// disconnected from.
    #[inline]
    pub fn connect(&mut self, channel_id: ChannelId) {
        self.channel_id = Some(channel_id);
        self.send_connect();
    }

    /// Disconnect from the current voice channel, if any.
    #[inline]
    pub fn disconnect(&mut self) {
        self.channel_id = None;
        self.send_connect();
    }

    /// Set the mute status of the voice connection.
    ///
    /// Note that enabling mute client-side is cosmetic and does not prevent the sending of audio;
    /// to fully mute, you must manually silence the audio source.
    #[inline]
    pub fn set_mute(&mut self, mute: bool) {
        self.mute = mute;
        if self.channel_id.is_some() {
            self.send_connect()
        }
    }

    /// Set the deaf status of the voice connection. Does not affect mute status.
    #[inline]
    pub fn set_deaf(&mut self, deaf: bool) {
        self.deaf = deaf;
        if self.channel_id.is_some() {
            self.send_connect()
        }
    }

    /// Get the current channel of this voice connection, if any.
    #[inline]
    pub fn current_channel(&self) -> Option<ChannelId> {
        self.channel_id
    }

    /// Send the connect/disconnect command over the main websocket
    fn send_connect(&self) {
        let _ = self
            .main_ws
            .send(crate::internal::Status::SendMessage(json! {{
                "op": 4,
                "d": {
                    "guild_id": self.server_id,
                    "channel_id": self.channel_id,
                    "self_mute": self.mute,
                    "self_deaf": self.deaf,
                }
            }}));
    }

    #[doc(hidden)]
    pub fn __update_state(&mut self, voice_state: &VoiceState) {
        if voice_state.user_id == self.user_id {
            self.channel_id = voice_state.channel_id;
            if voice_state.channel_id.is_some() {
                let session_id = voice_state.session_id.clone();
                if let Some((endpoint, token)) = self.endpoint_token.take() {
                    self.internal_connect(session_id, endpoint, token);
                } else {
                    self.session_id = Some(session_id);
                }
            } else {
                self.internal_disconnect();
            }
        }
    }

    #[doc(hidden)]
    pub fn __update_server(&mut self, endpoint: &Option<String>, token: &str) {
        if let Some(endpoint) = endpoint.clone() {
            let token = token.to_string();
            // nb: .take() is not used; in the event of server transfer, only this is called
            if let Some(session_id) = self.session_id.clone() {
                self.internal_connect(session_id, endpoint, token);
            } else {
                self.endpoint_token = Some((endpoint, token));
            }
        } else {
            self.internal_disconnect();
        }
    }

    /// Play from the given audio source.
    #[inline]
    pub fn play(&mut self, source: Box<dyn AudioSource>) {
        self.thread_send(Status::SetSource(Some(source)));
    }

    /// Stop the currently playing audio source.
    #[inline]
    pub fn stop(&mut self) {
        self.thread_send(Status::SetSource(None));
    }

    /// Set the receiver to which incoming voice will be sent.
    #[inline]
    pub fn set_receiver(&mut self, receiver: Box<dyn AudioReceiver>) {
        self.thread_send(Status::SetReceiver(Some(receiver)));
    }

    /// Clear the voice receiver, discarding incoming voice.
    #[inline]
    pub fn clear_receiver(&mut self) {
        self.thread_send(Status::SetReceiver(None));
    }

    fn thread_send(&mut self, status: Status) {
        match self.sender.send(status) {
            Ok(()) => {}
            Err(mpsc::SendError(status)) => {
                // voice thread has crashed... start it over again
                let (tx, rx) = mpsc::channel();
                self.sender = tx;
                self.sender.send(status).unwrap(); // should be infallible
                debug!("Restarting crashed voice thread...");
                start_voice_thread(self.server_id, rx);
                self.send_connect();
            }
        }
    }

    #[inline]
    fn internal_disconnect(&mut self) {
        self.thread_send(Status::Disconnect);
    }

    #[inline]
    fn internal_connect(&mut self, session_id: String, endpoint: String, token: String) {
        let user_id = self.user_id;
        let server_id = match (&self.server_id, &self.channel_id) {
            (&Some(ServerId(id)), _) | (&None, &Some(ChannelId(id))) => id,
            _ => {
                error!("no server_id or channel_id in internal_connect");
                return;
            }
        };
        self.thread_send(Status::Connect(ConnStartInfo {
            server_id: server_id,
            user_id: user_id,
            session_id: session_id,
            endpoint: endpoint,
            token: token,
        }));
    }
}

impl Drop for VoiceConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Create an audio source based on a `pcm_s16le` input stream.
///
/// The input data should be in signed 16-bit little-endian PCM input stream at 48000Hz. If
/// `stereo` is true, the channels should be interleaved, left first.
pub fn create_pcm_source<R: Read + Send + 'static>(stereo: bool, read: R) -> Box<dyn AudioSource> {
    Box::new(PcmSource(stereo, read))
}

struct PcmSource<R: Read + Send>(bool, R);

impl<R: Read + Send> AudioSource for PcmSource<R> {
    fn is_stereo(&mut self) -> bool {
        self.0
    }
    fn read_frame(&mut self, buffer: &mut [i16]) -> Option<usize> {
        let mut i = 0;
        for outval in buffer.iter_mut() {
            match self.1.read_i16::<LittleEndian>() {
                Ok(val) => *outval = val,
                Err(_) => break,
            };
            i += 1;
        }
        // When wrapping `Read`, we consider reading 0 samples to be EOF.
        if i == 0 {
            None
        } else {
            Some(i)
        }
    }
}

/// Use `ffmpeg` to open an audio file as a PCM stream.
///
/// Requires `ffmpeg` to be on the path and executable. If `ffprobe` is available and indicates
/// that the input file is stereo, the returned audio source will be stereo.
pub fn open_ffmpeg_stream<P: AsRef<::std::ffi::OsStr>>(path: P) -> Result<Box<dyn AudioSource>> {
    use std::process::{Command, Stdio};
    let path = path.as_ref();
    let stereo = check_stereo(path).unwrap_or(false);
    let child = Command::new("ffmpeg")
        .arg("-i")
        .arg(path)
        .args(&[
            "-f",
            "s16le",
            "-ac",
            if stereo { "2" } else { "1" },
            "-ar",
            "48000",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(create_pcm_source(stereo, ProcessStream(child)))
}

fn check_stereo(path: &::std::ffi::OsStr) -> Result<bool> {
    use std::process::{Command, Stdio};
    let output = Command::new("ffprobe")
        .args(&["-v", "quiet", "-of", "json", "-show_streams", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .output()?;
    let json: serde_json::Value = serde_json::from_reader(&output.stdout[..])?;
    let streams = json
        .as_object()
        .and_then(|m| m.get("streams"))
        .and_then(|v| v.as_array())
        .ok_or(Error::Other(""))?;
    Ok(streams.iter().any(|stream| {
        stream
            .as_object()
            .and_then(|m| m.get("channels").and_then(|v| v.as_i64()))
            == Some(2)
    }))
}

/// A stream that reads from a child's stdout and kills it on drop.
struct ProcessStream(::std::process::Child);

impl Read for ProcessStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.stdout.as_mut().expect("missing stdout").read(buf)
    }
}

impl Drop for ProcessStream {
    fn drop(&mut self) {
        // If we can't kill it, it's dead already or out of our hands
        let _ = self.0.kill();
        // To avoid zombie processes, we must also wait on it
        let _ = self.0.wait();
    }
}

/// Use `youtube-dl` and `ffmpeg` to stream from an internet source.
///
/// Requires both `youtube-dl` and `ffmpeg` to be on the path and executable.
/// On Windows, this means the `.exe` version of `youtube-dl` must be used.
///
/// The audio download is streamed rather than downloaded in full; this may be desireable for
/// longer audios but can introduce occasional brief interruptions.
pub fn open_ytdl_stream(url: &str) -> Result<Box<dyn AudioSource>> {
    use std::process::{Command, Stdio};
    let output = Command::new("youtube-dl")
        .args(&[
            "-f",
            "webm[abr>0]/bestaudio/best",
            "--no-playlist",
            "--print-json",
            "--skip-download",
            url,
        ])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(Error::Command("youtube-dl", output));
    }

    let json: serde_json::Value = serde_json::from_reader(&output.stdout[..])?;
    let map = match json.as_object() {
        Some(map) => map,
        None => return Err(Error::Other("youtube-dl output could not be read")),
    };
    let url = match map.get("url").and_then(serde_json::Value::as_str) {
        Some(url) => url,
        None => {
            return Err(Error::Other(
                "youtube-dl output's \"url\" could not be read",
            ))
        }
    };
    open_ffmpeg_stream(url)
}

enum Status {
    SetSource(Option<Box<dyn AudioSource>>),
    SetReceiver(Option<Box<dyn AudioReceiver>>),
    Connect(ConnStartInfo),
    Disconnect,
}

fn start_voice_thread(server_id: Option<ServerId>, rx: mpsc::Receiver<Status>) {
    let name = match server_id {
        Some(ServerId(id)) => format!("discord voice (server {})", id),
        None => "discord voice (private/groups)".to_owned(),
    };
    ::std::thread::Builder::new()
        .name(name)
        .spawn(move || voice_thread(rx))
        .expect("Failed to start voice thread");
}

fn voice_thread(channel: mpsc::Receiver<Status>) {
    let mut audio_source = None;
    let mut receiver = None;
    let mut connection = None;
    let mut audio_timer = crate::Timer::new(20);

    // start the main loop
    'outer: loop {
        // Check on the signalling channel
        loop {
            match channel.try_recv() {
                Ok(Status::SetSource(s)) => audio_source = s,
                Ok(Status::SetReceiver(r)) => receiver = r,
                Ok(Status::Connect(info)) => {
                    connection = InternalConnection::new(info)
                        .map_err(|e| error!("Error connecting to voice: {:?}", e))
                        .ok();
                }
                Ok(Status::Disconnect) => connection = None,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'outer,
            }
        }

        // Update the voice connection, transmitting and receiving data as needed
        let mut error = false;
        if let Some(connection) = connection.as_mut() {
            // update() will sleep using audio_timer as needed
            if let Err(e) = connection.update(&mut audio_source, &mut receiver, &mut audio_timer) {
                error!("Error in voice connection: {:?}", e);
                error = true;
            }
        } else {
            // no connection, so we sleep ourselves
            audio_timer.sleep_until_tick();
        }
        if error {
            connection = None;
        }
    }
}

struct ConnStartInfo {
    // may have originally been a ServerId or ChannelId
    server_id: u64,
    user_id: UserId,
    endpoint: String,
    session_id: String,
    token: String,
}

struct InternalConnection {
    sender: Sender<WebSocketStream>,
    receive_chan: mpsc::Receiver<RecvStatus>,
    ws_close: mpsc::Sender<()>,
    udp_close: mpsc::Sender<()>,
    encryption_key: crypto::Key,
    udp: UdpSocket,
    destination: ::std::net::SocketAddr,
    ssrc: u32,
    sequence: u16,
    timestamp: u32,
    speaking: bool,
    silence_frames: u8,
    decoder_map: HashMap<(u32, opus::Channels), opus::Decoder>,
    encoder: opus::Encoder,
    encoder_stereo: bool,
    keepalive_timer: crate::Timer,
    audio_keepalive_timer: crate::Timer,
    ws_thread: Option<::std::thread::JoinHandle<()>>,
    udp_thread: Option<::std::thread::JoinHandle<()>>,
}

const SAMPLE_RATE: u32 = 48000;
const HEADER_LEN: usize = 12;

impl InternalConnection {
    fn new(info: ConnStartInfo) -> Result<InternalConnection> {
        let ConnStartInfo {
            server_id,
            user_id,
            mut endpoint,
            session_id,
            token,
        } = info;

        // prepare the URL: drop the :80 and prepend wss://
        if endpoint.ends_with(":80") {
            let len = endpoint.len();
            endpoint.truncate(len - 3);
        }
        // establish the websocket connection
        // v=4 as described at https://discord.com/developers/docs/topics/voice-connections#voice-gateway-versioning-gateway-versions
        let url = match ::websocket::client::request::Url::parse(&format!("wss://{}?v=4", endpoint))
        {
            Ok(url) => url,
            Err(_) => return Err(Error::Other("Invalid endpoint URL")),
        };
        let response = Client::connect(url)?.send()?;
        response.validate()?;
        let (mut sender, mut receiver) = response.begin().split();

        // send the handshake
        let map = json! {{
            "op": 0,
            "d": {
                "server_id": server_id,
                "user_id": user_id,
                "session_id": session_id,
                "token": token,
            }
        }};
        sender.send_json(&map)?;

        let mut interval = 10_000; // crappy guess in case we fail to receive one
        let (port, ssrc, modes, ip) = loop {
            match receiver.recv_json(VoiceEvent::decode)? {
                VoiceEvent::Hello { heartbeat_interval } => {
                    interval = heartbeat_interval;
                }
                VoiceEvent::VoiceReady {
                    port,
                    ssrc,
                    modes,
                    ip,
                } => {
                    break (port, ssrc, modes, ip);
                }
                other => {
                    debug!("Unexpected voice msg: {:?}", other);
                    return Err(Error::Protocol("Unexpected message setting up voice"));
                }
            }
        };
        if !modes.iter().any(|s| s == "xsalsa20_poly1305") {
            return Err(Error::Protocol(
                "Voice mode \"xsalsa20_poly1305\" unavailable",
            ));
        }

        // bind a UDP socket and send the ssrc value in a packet as identification
        let destination = {
            use std::net::ToSocketAddrs;
            (ip.as_ref().map(|ip| &ip[..]).unwrap_or(&endpoint[..]), port)
                .to_socket_addrs()?
                .next()
                .ok_or(Error::Other("Failed to resolve voice hostname"))?
        };
        let udp = UdpSocket::bind("0.0.0.0:0")?;
        debug!("local addr = {:?}", udp.local_addr());
        {
            // https://discord.com/developers/docs/topics/voice-connections#ip-discovery
            let mut bytes = [0; 2 + 2 + 4 + 64 + 2];
            let mut msg = &mut bytes[..];
            msg.write_u16::<BigEndian>(0x1)?;
            msg.write_u16::<BigEndian>(70)?;
            msg.write_u32::<BigEndian>(ssrc)?;
            debug!("sending {:x?} to {:?}", bytes, destination);
            udp.send_to(&bytes, destination)?;
        }

        {
            // receive the response to the identification to get port and address info
            let mut bytes = [0; 256];
            let (len, _) = udp.recv_from(&mut bytes)?;
            let mut msg = &bytes[..len];
            assert_eq!(0x2, msg.read_u16::<BigEndian>()?);
            assert_eq!(70, msg.read_u16::<BigEndian>()?);
            assert_eq!(ssrc, msg.read_u32::<BigEndian>()?);
            let (addr, mut msg) = msg.split_at(64);
            let addr = &addr[..addr.iter().position(|&x| x == 0).unwrap()];
            let port_number = msg.read_u16::<BigEndian>()?;

            // send the acknowledgement websocket message
            let map = json! {{
                "op": 1,
                "d": {
                    "protocol": "udp",
                    "data": {
                        "address": addr,
                        "port": port_number,
                        "mode": "xsalsa20_poly1305",
                    }
                }
            }};
            sender.send_json(&map)?;
        }

        // discard websocket messages until we get the Ready
        let encryption_key;
        loop {
            match receiver.recv_json(VoiceEvent::decode)? {
                VoiceEvent::Hello { heartbeat_interval } => {
                    // Not hit in usual operation; just for coverage.
                    interval = heartbeat_interval;
                }
                VoiceEvent::SessionDescription { mode, secret_key } => {
                    encryption_key =
                        crypto::Key::from_slice(&secret_key).expect("failed to create key");
                    if mode != "xsalsa20_poly1305" {
                        return Err(Error::Protocol(
                            "Voice mode in Ready was not \"xsalsa20_poly1305\"",
                        ));
                    }
                    break;
                }
                VoiceEvent::Unknown(op, value) => {
                    debug!("Unknown message type: {}/{:?}", op, value)
                }
                _ => {}
            }
        }

        // start two child threads: one for the voice websocket and another for UDP voice packets
        let thread = ::std::thread::current();
        let thread_name = thread.name().unwrap_or("discord voice");

        let (udp_sender_close, udp_reader_close) = mpsc::channel();
        let (ws_sender_close, ws_reader_close) = mpsc::channel();
        let (receive_chan, ws_thread, udp_thread) = {
            let (tx1, rx) = mpsc::channel();
            let tx2 = tx1.clone();
            let udp_clone = udp.try_clone()?;
            let ws_thread = Some(
                ::std::thread::Builder::new()
                    .name(format!("{} (WS reader)", thread_name))
                    .spawn(move || {
                        {
                            match *receiver.get_mut().get_mut() {
                                WebSocketStream::Tcp(ref inner) => {
                                    inner.set_nonblocking(true).unwrap()
                                }
                                WebSocketStream::Ssl(ref inner) => inner
                                    .lock()
                                    .unwrap()
                                    .get_ref()
                                    .set_nonblocking(true)
                                    .unwrap(),
                            };
                        }
                        loop {
                            while let Ok(msg) = receiver.recv_json(VoiceEvent::decode) {
                                match tx1.send(RecvStatus::Websocket(msg)) {
                                    Ok(()) => {}
                                    Err(_) => return,
                                }
                            }
                            if let Ok(_) = ws_reader_close.try_recv() {
                                return;
                            }
                            ::std::thread::sleep(::std::time::Duration::from_millis(25));
                        }
                    })?,
            );
            let udp_thread = Some(
                ::std::thread::Builder::new()
                    .name(format!("{} (UDP reader)", thread_name))
                    .spawn(move || {
                        udp_clone
                            .set_read_timeout(Some(::std::time::Duration::from_millis(100)))
                            .unwrap();
                        let mut buffer = [0; 512];
                        loop {
                            if let Ok((len, _)) = udp_clone.recv_from(&mut buffer) {
                                match tx2.send(RecvStatus::Udp(buffer[..len].to_vec())) {
                                    Ok(()) => {}
                                    Err(_) => return,
                                }
                            } else if let Ok(_) = udp_reader_close.try_recv() {
                                return;
                            }
                        }
                    })?,
            );
            (rx, ws_thread, udp_thread)
        };

        info!("Voice connected to {} ({})", endpoint, destination);
        Ok(InternalConnection {
            sender: sender,
            receive_chan: receive_chan,
            ws_close: ws_sender_close,
            udp_close: udp_sender_close,

            encryption_key: encryption_key,
            udp: udp,
            destination: destination,

            ssrc: ssrc,
            sequence: 0,
            timestamp: 0,
            speaking: false,
            silence_frames: 0,

            decoder_map: HashMap::new(),
            encoder: opus::Encoder::new(
                SAMPLE_RATE,
                opus::Channels::Mono,
                opus::Application::Audio,
            )?,
            encoder_stereo: false,
            keepalive_timer: crate::Timer::new(interval),
            // after 5 minutes of us sending nothing, Discord will stop sending voice data to us
            audio_keepalive_timer: crate::Timer::new(4 * 60 * 1000),

            ws_thread: ws_thread,
            udp_thread: udp_thread,
        })
    }

    fn update(
        &mut self,
        source: &mut Option<Box<dyn AudioSource>>,
        receiver: &mut Option<Box<dyn AudioReceiver>>,
        audio_timer: &mut crate::Timer,
    ) -> Result<()> {
        let mut audio_buffer = [0i16; 960 * 2]; // 20 ms, stereo
        let mut packet = [0u8; 512]; // 256 forces opus to reduce bitrate for some packets
        let mut nonce = crypto::Nonce([0; 24]);

        // Check for received voice data
        if let Some(receiver) = receiver.as_mut() {
            while let Ok(status) = self.receive_chan.try_recv() {
                match status {
                    RecvStatus::Websocket(VoiceEvent::SpeakingUpdate {
                        user_id,
                        ssrc,
                        speaking,
                    }) => {
                        receiver.speaking_update(ssrc, user_id, speaking);
                    }
                    RecvStatus::Websocket(_) => {}
                    RecvStatus::Udp(packet) => {
                        let mut handle = &packet[2..];
                        let sequence = handle.read_u16::<BigEndian>()?;
                        let timestamp = handle.read_u32::<BigEndian>()?;
                        let ssrc = handle.read_u32::<BigEndian>()?;
                        nonce.0[..HEADER_LEN].clone_from_slice(&packet[..HEADER_LEN]);
                        if let Ok(decrypted) =
                            crypto::open(&packet[HEADER_LEN..], &nonce, &self.encryption_key)
                        {
                            let channels = opus::packet::get_nb_channels(&decrypted)?;
                            let len = self
                                .decoder_map
                                .entry((ssrc, channels))
                                .or_insert_with(|| {
                                    opus::Decoder::new(SAMPLE_RATE, channels).unwrap()
                                })
                                .decode(&decrypted, &mut audio_buffer, false)?;
                            let stereo = channels == opus::Channels::Stereo;
                            receiver.voice_packet(
                                ssrc,
                                sequence,
                                timestamp,
                                stereo,
                                &audio_buffer[..if stereo { len * 2 } else { len }],
                            );
                        }
                    }
                }
            }
        } else {
            // if there's no receiver, discard incoming events
            while let Ok(_) = self.receive_chan.try_recv() {}
        }

        // Send the voice websocket keepalive if needed
        if self.keepalive_timer.check_tick() {
            let map = json! {{
                "op": 3,
                "d": serde_json::Value::Null,
            }};
            self.sender.send_json(&map)?;
        }

        // Send the UDP keepalive if needed
        if self.audio_keepalive_timer.check_tick() {
            let mut bytes = [0; 4];
            (&mut bytes[..]).write_u32::<BigEndian>(self.ssrc)?;
            self.udp.send_to(&bytes, self.destination)?;
        }

        // read the audio from the source
        let mut clear_source = false;
        let len = if let Some(source) = source.as_mut() {
            let stereo = source.is_stereo();
            if stereo != self.encoder_stereo {
                let channels = if stereo {
                    opus::Channels::Stereo
                } else {
                    opus::Channels::Mono
                };
                self.encoder = opus::Encoder::new(SAMPLE_RATE, channels, opus::Application::Audio)?;
                self.encoder_stereo = stereo;
            }
            let buffer_len = if stereo { 960 * 2 } else { 960 };
            match source.read_frame(&mut audio_buffer[..buffer_len]) {
                Some(len) => len,
                None => {
                    clear_source = true;
                    0
                }
            }
        } else {
            0
        };
        if clear_source {
            *source = None;
        }
        if len == 0 {
            // stop speaking, don't send any audio
            self.set_speaking(false)?;
            if self.silence_frames > 0 {
                // send a few frames of silence; could be optimized to be pre-encoded
                self.silence_frames -= 1;
                for value in &mut audio_buffer[..] {
                    *value = 0;
                }
            } else {
                audio_timer.sleep_until_tick();
                return Ok(());
            }
        } else {
            self.silence_frames = 5;
            // zero-fill the rest of the buffer
            for value in &mut audio_buffer[len..] {
                *value = 0;
            }
        }
        self.set_speaking(true)?;

        // prepare the packet header
        {
            let mut cursor = &mut packet[..HEADER_LEN];
            cursor.write_all(&[0x80, 0x78])?;
            cursor.write_u16::<BigEndian>(self.sequence)?;
            cursor.write_u32::<BigEndian>(self.timestamp)?;
            cursor.write_u32::<BigEndian>(self.ssrc)?;
            debug_assert!(cursor.is_empty());
        }
        nonce.0[..HEADER_LEN].clone_from_slice(&packet[..HEADER_LEN]);

        // encode the audio data
        let extent = packet.len() - 16; // leave 16 bytes for encryption overhead
        let buffer_len = if self.encoder_stereo { 960 * 2 } else { 960 };
        let len = self
            .encoder
            .encode(&audio_buffer[..buffer_len], &mut packet[HEADER_LEN..extent])?;
        let crypted = crypto::seal(
            &packet[HEADER_LEN..HEADER_LEN + len],
            &nonce,
            &self.encryption_key,
        );
        packet[HEADER_LEN..HEADER_LEN + crypted.len()].clone_from_slice(&crypted);

        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(960);

        // wait until the right time, then transmit the packet
        audio_timer.sleep_until_tick();
        self.udp
            .send_to(&packet[..HEADER_LEN + crypted.len()], self.destination)?;
        self.audio_keepalive_timer.defer();
        Ok(())
    }

    fn set_speaking(&mut self, speaking: bool) -> Result<()> {
        if self.speaking == speaking {
            return Ok(());
        }
        self.speaking = speaking;
        let map = json! {{
            "op": 5,
            "d": {
                "speaking": speaking,
                "delay": 0,
            }
        }};
        self.sender.send_json(&map)
    }
}

impl Drop for InternalConnection {
    fn drop(&mut self) {
        // Shutdown both internal threads
        let _ = self.udp_close.send(());
        let _ = self.udp_thread.take().unwrap().join();
        let _ = self.ws_close.send(());
        let _ = self.ws_thread.take().unwrap().join();
        info!("Voice disconnected");
    }
}

enum RecvStatus {
    Websocket(VoiceEvent),
    Udp(Vec<u8>),
}
