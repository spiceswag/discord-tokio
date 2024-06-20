//! Events returned by the gateway.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::serial::Eq;

use super::{
    Activity, Attachment, Call, Channel, ChannelId, CurrentUser, CurrentUserPatch, Emoji,
    FriendSourceFlags, LiveServer, Member, Message, MessageId, MessageType, OnlineStatus,
    PossibleServer, Presence, Relationship, RelationshipType, Role, RoleId, Server, ServerId,
    SingleReaction, Tutorial, UnreadMessages, User, UserId, UserServerSettings, UserSettings,
    VoiceState,
};

/// A JSON payload message sent to the gateway.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SentMessage {
    /// Used to trigger the initial handshake with the gateway.
    Identify {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<2>,

        /// The payload sent with this message.
        #[serde(rename = "d")]
        payload: IdentifyPayload,
    },

    /// Used to replay missed events when a disconnected client resumes.
    Resume {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<6>,

        /// The payload sent with this message.
        #[serde(rename = "d")]
        payload: ResumePayload,
    },

    /// Used to maintain an active gateway connection.
    ///
    /// Must be sent every `heartbeat_interval` milliseconds after the Opcode 10 Hello payload is received.
    /// The inner d key is the last sequence number (the field labeled `s`) received by the client. If you have not yet received one, send `None`.
    Heartbeat {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<1>,

        /// The last event sequence number received by the client (the field labeled `s` on dispatch messages).
        /// If one has not yet been received, send `None`.
        #[serde(rename = "d")]
        last_sequence: Option<u64>,
    },

    /// Used to request all members for a guild or a list of guilds.
    /// If a client wishes to receive all members, they need to explicitly request them via this operation.
    ///
    /// The server will send Guild Members Chunk events in response with up to
    /// 1000 members per chunk until all members that match the request have been sent.
    ///
    /// Discord restricts returned members through intents:
    /// - `GUILD_PRESENCES` intent is required to set `presences: true`. Otherwise, it will always be false.
    /// - `GUILD_MEMBERS` intent is required to request the entire member list —- `(query=‘’, limit=0<=n)`
    /// - You will be limited to requesting 1 `guild_id` per request (this seems to only apply to bots).
    /// - Requesting a prefix (query parameter) will return a maximum of 100 members.
    /// - Requesting `user_ids` will continue to be limited to returning 100 members
    ///
    /// # Ready event
    ///
    /// When initially connecting, if you don't have the `GUILD_PRESENCES` Gateway Intent, or if the guild is over 75k members,
    /// it will only send members who are in voice, plus the member for you (the connecting user).
    ///
    /// Otherwise, if a guild has over large_threshold members (value in the Gateway `Identify`), it will only send members who are online,
    /// have a role, have a nickname, or are in a voice channel, and if it has under large_threshold members, it will send all members.  
    RequestGuildMembers {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<8>,

        /// The request parameters.
        #[serde(rename = "d")]
        payload: RequestGuildMembersPayload,
    },

    /// Sent when a client wants to join, move, or disconnect from a voice channel.
    UpdateVoiceState {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<4>,

        /// The update payload.
        #[serde(rename = "d")]
        payload: UpdateVoiceStatePayload,
    },

    /// Sent by the client to indicate a presence or status update.
    UpdatePresence {
        /// The opcode behind this event type.
        #[doc(hidden)]
        op: Eq<3>,

        /// The update payload.
        #[serde(rename = "d")]
        payload: UpdatePresencePayload,
    },
}

/// The payload sent along with the `Identify` message (opcode 2).
#[derive(Debug, Clone, Serialize)]
pub struct IdentifyPayload {
    /// Authentication token.
    pub token: String,
    /// A tuple of the two values `(shard_id, num_shards)`, used for guild sharding.
    pub shard: Option<(u8, u8)>,
    /// Gateway Intents you wish to receive.
    pub intents: (),

    /// Whether this connection supports compression of packets
    pub compress: Option<bool>,
    /// Value between 50 and 250, total number of members where the gateway will stop sending offline members in the guild member list.
    pub large_threshold: Option<u64>,

    /// System fingerprinting information for discord analytics.
    #[serde(rename = "properties")]
    pub fingerprint: IdentifyConnection,
}

/// A connection fingerprint of sorts, including information about the bot's environment.
///
/// This is useful for discord to collect, because bots don't include normal user agent strings.
/// Oddly enough, this is still collected for regular users.
#[derive(Debug, Clone, Serialize)]
pub struct IdentifyConnection {
    /// The current operating system.
    pub os: String,

    /// For bot users, this is set as the current library.
    /// For non-bot users, this is a description of the current client program.
    pub browser: String,

    /// For bot users, this is set as the current library.
    /// For non-bot users, this is a description of the current device.
    pub device: String,
}

/// The payload sent along with the `Resume` message (opcode 6).
#[derive(Debug, Clone, Serialize)]
pub struct ResumePayload {
    /// The token of the authenticating user.
    token: String,

    /// The session ID sent by the gateway during the failed connection.
    session_id: String,

    /// The number of the last sequence number received
    #[serde(rename = "seq")]
    last_sequence: u64,
}

/// The request payload sent along with the `RequestGuildMembers` message (opcode 8).
#[derive(Debug, Clone, Serialize)]
pub struct RequestGuildMembersPayload {
    /// ID of the server to get members for.
    #[serde(rename = "guild_id")]
    pub server_id: ServerId,

    /// String that username starts with, or an empty string to return all members.
    /// This field is mandatory, except when the `user_ids` field is set.
    #[serde(rename = "query")]
    pub username_query: Option<String>,

    /// Maximum number of members to send matching the query; a limit of 0
    /// can be used with an empty string query to return all members.
    pub limit: u32,

    /// Used to specify if we want the presences of the matched members.
    pub presences: bool,

    /// Used to specify which users you wish to fetch.
    /// This field may be specified instead of `username_query`.
    pub user_ids: Option<Vec<UserId>>,

    /// A nonce value to identify the Guild Members Chunk response.
    pub nonce: String,
}

/// The request payload sent along with the `UpdateVoiceState` message (opcode 4).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateVoiceStatePayload {
    /// ID of the guild to change the state of.
    pub guild_id: ServerId,
    /// ID of the voice channel the client wants to join (`None` if disconnecting).
    pub channel_id: Option<ChannelId>,

    /// Whether the client is muted.
    pub self_mute: bool,
    /// Whether the client is deafened.
    pub self_deaf: bool,
}

/// The new user presence that will be attached to the gateway's user.
#[derive(Debug, Clone, Serialize)]
pub struct UpdatePresencePayload {
    /// The user's activities
    pub activities: Vec<Activity>,
    /// The user's new status
    pub status: OnlineStatus,

    /// Unix time (in milliseconds) of when the client went idle, or `None` if the client is not idle.
    pub since: Option<u64>,

    /// Whether or not the client is away from keyboard.
    pub afk: bool,
}

/// A JSON payload message received over the gateway, of any purpose, not just event dispatching.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ReceivedMessage {
    /// An event was sent by the gateway.
    Dispatch {
        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<0>,

        /// The received dispatch.
        #[serde(flatten)]
        dispatch: DispatchPayload,
    },

    /// The gateway asks the bot to reconnect to the gateway.
    Reconnect {
        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<7>,
    },

    /// The current gateway session is invalid.
    InvalidSession {
        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<9>,
    },

    /// The first message sent to the client.
    Hello {
        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<10>,

        #[serde(rename = "d")]
        payload: HelloPayload,
    },

    /// Sent in response to receiving a heartbeat to acknowledge that it has been received.
    HeartbeatAck {
        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<11>,
    },
}

/// The data (`d`) field of a discord gateway `Hello` event.
#[derive(Debug, Clone, Deserialize)]
pub struct HelloPayload {
    /// Interval (in milliseconds) an app should heartbeat with.
    pub heartbeat_interval: u64,
}

/// A dispatch event (opcode 0) received from the discord gateway.
/// This structure is to be used in conjunction with `#[serde(flatten)]`
#[derive(Debug, Clone, Deserialize)]
pub struct DispatchPayload {
    /// The event that occurred.
    #[serde(flatten)]
    pub event: Event,

    /// The sequence number of the event.
    #[serde(rename = "s")]
    pub sequence: u64,
}

/// Event received over a websocket connection.
///
/// When deserialized as part of a `struct` use `#[serde(flatten)]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "t", content = "d")]
pub enum Event {
    /// The first event in a connection, containing the initial state.
    ///
    /// May also be received at a later time in the event of a reconnect.
    Ready(ReadyEvent),
    /// The connection has successfully resumed after a disconnect.
    Resumed {
        /// The trace of discord gateway servers involved in serving this connection.
        #[serde(rename = "_trace")]
        trace: Vec<Option<String>>,
    },

    /// Update to the logged-in user's information
    UserUpdate(CurrentUserPatch),
    /// Update to a note that the logged-in user has set for another user.
    UserNoteUpdate(UserId, String),
    /// Update to the logged-in user's preferences or client settings
    UserSettingsUpdate {
        detect_platform_accounts: Option<bool>,
        developer_mode: Option<bool>,
        enable_tts_command: Option<bool>,
        inline_attachment_media: Option<bool>,
        inline_embed_media: Option<bool>,
        locale: Option<String>,
        message_display_compact: Option<bool>,
        render_embeds: Option<bool>,
        server_positions: Option<Vec<ServerId>>,
        show_current_game: Option<bool>,
        status: Option<String>,
        theme: Option<String>,
        convert_emoticons: Option<bool>,
        friend_source_flags: Option<FriendSourceFlags>,
    },
    /// Update to the logged-in user's server-specific notification settings
    UserServerSettingsUpdate(UserServerSettings),
    /// A member's voice state has changed
    VoiceStateUpdate(Option<ServerId>, VoiceState),
    /// Voice server information is available
    VoiceServerUpdate {
        server_id: Option<ServerId>,
        channel_id: Option<ChannelId>,
        endpoint: Option<String>,
        token: String,
    },
    /// A new group call has been created
    CallCreate(Call),
    /// A group call has been updated
    CallUpdate {
        channel_id: ChannelId,
        message_id: MessageId,
        region: String,
        ringing: Vec<UserId>,
    },
    /// A group call has been deleted (the call ended)
    CallDelete(ChannelId),
    /// A user has been added to a group
    ChannelRecipientAdd(ChannelId, User),
    /// A user has been removed from a group
    ChannelRecipientRemove(ChannelId, User),

    /// A user is typing; considered to last 5 seconds
    TypingStart {
        channel_id: ChannelId,
        user_id: UserId,
        timestamp: DateTime<Utc>,
    },
    /// A member's presence state (or username or avatar) has changed
    /// https://discord.com/developers/docs/topics/gateway#presence-update
    PresenceUpdate {
        presence: Presence,
        server_id: Option<ServerId>,
        roles: Option<Vec<RoleId>>,
    },
    /// The presence list of the user's friends should be replaced entirely
    PresencesReplace(Vec<Presence>),
    RelationshipAdd(Relationship),
    RelationshipRemove(UserId, RelationshipType),

    MessageCreate(Message),
    /// A message has been edited, either by the user or the system
    MessageUpdate {
        id: MessageId,
        channel_id: ChannelId,
        kind: Option<MessageType>,
        content: Option<String>,
        nonce: Option<String>,
        tts: Option<bool>,
        pinned: Option<bool>,
        timestamp: Option<DateTime<FixedOffset>>,
        edited_timestamp: Option<DateTime<FixedOffset>>,
        author: Option<User>,
        mention_everyone: Option<bool>,
        mentions: Option<Vec<User>>,
        mention_roles: Option<Vec<RoleId>>,
        attachments: Option<Vec<Attachment>>,
        embeds: Option<Vec<Value>>,
    },
    /// Another logged-in device acknowledged this message
    MessageAck {
        channel_id: ChannelId,
        /// May be `None` if a private channel with no messages has closed.
        message_id: Option<MessageId>,
    },
    MessageDelete {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    MessageDeleteBulk {
        channel_id: ChannelId,
        ids: Vec<MessageId>,
    },

    ServerCreate(PossibleServer<LiveServer>),
    ServerUpdate(Server),
    ServerDelete(PossibleServer<Server>),

    ServerMemberAdd(ServerId, Member),
    /// A member's roles have changed
    ServerMemberUpdate {
        server_id: ServerId,
        roles: Vec<RoleId>,
        user: User,
        nick: Option<String>,
    },
    ServerMemberRemove(ServerId, User),
    ServerMembersChunk(ServerId, Vec<Member>),
    ServerSync {
        server_id: ServerId,
        large: bool,
        members: Vec<Member>,
        presences: Vec<Presence>,
    },

    ServerRoleCreate(ServerId, Role),
    ServerRoleUpdate(ServerId, Role),
    ServerRoleDelete(ServerId, RoleId),

    ServerBanAdd(ServerId, User),
    ServerBanRemove(ServerId, User),

    ServerIntegrationsUpdate(ServerId),
    ServerEmojisUpdate(ServerId, Vec<Emoji>),

    ChannelCreate(Channel),
    ChannelUpdate(Channel),
    ChannelDelete(Channel),
    ChannelPinsAck {
        channel_id: ChannelId,
        timestamp: DateTime<FixedOffset>,
    },
    ChannelPinsUpdate {
        channel_id: ChannelId,
        last_pin_timestamp: Option<DateTime<FixedOffset>>,
    },

    ReactionAdd(SingleReaction),
    ReactionRemove(SingleReaction),

    /// An event type not covered by the above
    #[serde(other)]
    Unknown,
}

/// The "Ready" event, containing initial state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyEvent {
    /// Active gateway version
    #[serde(rename = "v")]
    pub version: u64,

    /// Logged in user.
    pub user: CurrentUser,

    /// The ID of the current session, used for reconnecting.
    pub session_id: String,

    /// A list of servers the user is in.
    /// Servers will be eventually populated with discrete server create events.
    #[serde(rename = "guilds")]
    pub servers: Vec<PossibleServer<LiveServer>>,

    // Non-bot users
    /// For non-bot users, a list of messages that have not been acknowledged.
    #[serde(rename = "read_state")]
    pub unread_messages: Option<Vec<UnreadMessages>>,

    /// A list of users who have been blocked, or added as friends of the user.
    pub relationships: Option<Vec<Relationship>>,

    /// The account settings of the current non-bot user.
    pub user_settings: Option<UserSettings>,
    /// For non-bot users, user settings which influence per-server notification behavior.
    pub user_server_settings: Option<Vec<UserServerSettings>>,

    /// For user accounts, largely undocumented tutorial stuff.
    pub tutorial: Option<Tutorial>,

    /// For a non-bot user, a map of notes set for other users.
    pub notes: Option<BTreeMap<UserId, Option<String>>>,

    // Bot Users
    /// For bot users, the shard info for this session;
    /// the shard ID used and the total number of shards.
    pub shard: Option<(u8, u8)>,

    /// The trace of discord gateway servers involved in serving this connection.
    #[serde(rename = "_trace")]
    pub trace: Option<Vec<String>>,
}

// Voice

#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    Hello {
        // 8
        heartbeat_interval: u64,
    },
    VoiceReady {
        // 2
        port: u16,
        ssrc: u32,
        modes: Vec<String>,
        ip: Option<String>,
        // ignore heartbeat_interval: https://discord.com/developers/docs/topics/voice-connections#establishing-a-voice-websocket-connection-example-voice-ready-payload
    },
    SessionDescription {
        mode: String,
        secret_key: Vec<u8>,
    },

    SpeakingUpdate {
        user_id: UserId,
        ssrc: u32,
        speaking: bool,
    },
    KeepAlive,
    HeartbeatAck,
    Unknown(u64, Value),
}
