//! Events returned by the gateway.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::serial::Eq;

use super::{
    Attachment, Call, Channel, ChannelId, CurrentUser, CurrentUserPatch, Emoji, FriendSourceFlags,
    LiveServer, Member, Message, MessageId, MessageType, PossibleServer, Presence, Relationship,
    RelationshipType, Role, RoleId, Server, ServerId, SingleReaction, Tutorial, UnreadMessages,
    User, UserId, UserServerSettings, UserSettings, VoiceState,
};

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
    /// The precense list of the user's friends should be replaced entirely
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
    pub trace: Vec<Option<String>>,
}

/// An event received over the gateway, of any purpose.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ReceivedMessage {
    /// An event was sent by the gateway.
    Dispatch {
        /// The received dispatch.
        #[serde(flatten)]
        dispatch: DispatchPayload,

        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<0>,
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
        #[serde(rename = "d")]
        payload: HelloPayload,

        /// The parsed opcode from the event.
        #[doc(hidden)]
        op: Eq<10>,
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
