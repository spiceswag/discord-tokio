//! Events returned by the gateway.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Utc};
use serde_json::Value;

use crate::Object;

use super::{
    Attachment, Call, Channel, ChannelId, CurrentUser, CurrentUserPatch, Emoji, FriendSourceFlags,
    LiveServer, Member, Message, MessageId, MessageType, PossibleServer, Presence, Reaction,
    Relationship, RelationshipType, Role, RoleId, Server, ServerId, Tutorial, UnreadMessages, User,
    UserId, UserServerSettings, UserSettings, VoiceState,
};

/// Event received over a websocket connection
#[derive(Debug, Clone)]
pub enum Event {
    /// The first event in a connection, containing the initial state.
    ///
    /// May also be received at a later time in the event of a reconnect.
    Ready(ReadyEvent),
    /// The connection has successfully resumed after a disconnect.
    Resumed {
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

    ReactionAdd(Reaction),
    ReactionRemove(Reaction),

    /// An event type not covered by the above
    Unknown(String, Object),
    // Any other event. Should never be used directly.
    #[doc(hidden)]
    __NonExhaustive,
}

/// The "Ready" event, containing initial state
#[derive(Debug, Clone)]
pub struct ReadyEvent {
    pub version: u64,
    pub user: CurrentUser,
    pub session_id: String,
    pub user_settings: Option<UserSettings>,
    pub unread_messages: Option<Vec<UnreadMessages>>,
    pub private_channels: Vec<Channel>,
    pub presences: Vec<Presence>,
    pub relationships: Vec<Relationship>,
    pub servers: Vec<PossibleServer<LiveServer>>,
    pub user_server_settings: Option<Vec<UserServerSettings>>,
    pub tutorial: Option<Tutorial>,
    /// The trace of servers involved in this connection.
    pub trace: Vec<Option<String>>,
    pub notes: Option<BTreeMap<UserId, Option<String>>>,
    /// The shard info for this session; the shard id used and the total number
    /// of shards.
    pub shard: Option<[u8; 2]>,
}

/// A raw gateway event,
/// containing the possibility for control messages.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum RawGatewayEvent {
    Dispatch(u64, Event),
    Heartbeat(u64),
    Reconnect,
    InvalidateSession,
    Hello(u64),
    HeartbeatAck,
}

//=================
// Voice event model
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
