//! Data models for `live` resources, i.e. those that are accessible,
//! and are maintained by an active gateway connection.
//!
//! Types in this category include `LiveServer`, `Presences` and so on.

use bitflags::bitflags;
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use tracing::{error, warn};

use super::{
    ChannelCategory, ChannelId, ChannelType, Emoji, EmojiId, MessageId, PermissionOverwrite,
    Permissions, Role, RoleId, Server, ServerChannel, ServerId, User, UserId, VerificationLevel,
};

// Live Server

/// Live server information is provided and maintained actively by the gateway.
///
/// It is suitable for querying the constantly changing parts of a server,
/// such as its member, and those members' presences, but also static (`frozen`)
/// information, such as its owner.
#[derive(Debug, Clone)]
pub struct LiveServer {
    pub id: ServerId,
    pub name: String,
    pub owner_id: UserId,
    pub application_id: Option<u64>,
    pub voice_states: Vec<VoiceState>,
    pub roles: Vec<Role>,
    pub region: String,
    pub presences: Vec<Presence>,
    pub member_count: u64,
    pub members: Vec<Member>,
    pub joined_at: String,
    pub icon: Option<String>,
    pub large: bool,
    pub channels: Vec<ServerChannel>,
    pub categories: Vec<ChannelCategory>,
    pub afk_timeout: u64,
    pub afk_channel_id: Option<ChannelId>,
    pub system_channel_id: Option<ChannelId>,
    pub verification_level: VerificationLevel,
    pub emojis: Vec<Emoji>,
    pub features: Vec<String>,
    pub splash: Option<String>,
    pub default_message_notifications: u64,
    pub mfa_level: u64,
    pub explicit_content_filter: u64,
}

impl LiveServer {
    /// Returns the formatted URL of the server's icon.
    ///
    /// Returns None if the server does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/icons/{}/{}.jpg"), self.id, icon))
    }

    /// Calculate the effective permissions for a specific user in a specific
    /// channel on this server.
    pub fn permissions_for(&self, channel: ChannelId, user: UserId) -> Permissions {
        // Owner has all permissions
        if user == self.owner_id {
            return Permissions::all();
        }

        // OR together all the user's roles
        let everyone = match self.roles.iter().find(|r| r.id == self.id.everyone()) {
            Some(r) => r,
            None => {
                error!(
                    "Missing @everyone role in permissions lookup on {} ({})",
                    self.name, self.id
                );
                return Permissions::empty();
            }
        };

        // Permissions acquired through granted roles
        let mut role_permissions = everyone.permissions;

        let member = match self.members.iter().find(|u| u.user.unwrap().id == user) {
            Some(u) => u,
            None => return everyone.permissions,
        };
        for &role in &member.roles {
            if let Some(role) = self.roles.iter().find(|r| r.id == role) {
                role_permissions |= role.permissions;
            } else {
                warn!(
                    "perms: {:?} on {:?} has non-existent role {:?}",
                    member.user.unwrap().id,
                    self.id,
                    role
                );
            }
        }

        // Administrators have all permissions in any channel
        if role_permissions.contains(Permissions::ADMINISTRATOR) {
            return Permissions::all();
        }

        let mut text_channel = false;
        if let Some(channel) = self.channels.iter().find(|c| c.id() == &channel) {
            text_channel = channel.kind() == ChannelType::Text;
            // Apply role overwrites, denied then allowed
            for overwrite in channel.permission_overwrites() {
                if let PermissionOverwrite::Role {
                    id, allow, deny, ..
                } = overwrite
                {
                    // if the member has this role, or it is the @everyone role
                    if member.roles.contains(id) || id.0 == self.id.0 {
                        role_permissions = (role_permissions & !*deny) | *allow;
                    }
                }
            }
            // Apply member overwrites, denied then allowed
            for overwrite in channel.permission_overwrites() {
                if let PermissionOverwrite::Member {
                    id, allow, deny, ..
                } = overwrite
                {
                    if &user == id {
                        role_permissions = (role_permissions & !*deny) | *allow;
                    }
                }
            }
        } else {
            warn!("perms: {:?} does not contain {:?}", self.id, channel);
        }

        // Default channel is always readable
        if channel.0 == self.id.0 {
            role_permissions |= Permissions::READ_MESSAGES;
        }

        // calculate implicit permissions

        // No SEND_MESSAGES => no message-sending-related actions
        if !role_permissions.contains(Permissions::SEND_MESSAGES) {
            role_permissions &= !(Permissions::SEND_TTS_MESSAGES
                | Permissions::MENTION_EVERYONE
                | Permissions::EMBED_LINKS
                | Permissions::ATTACH_FILES);
        }

        // No READ_MESSAGES => no channel actions
        if !role_permissions.contains(Permissions::READ_MESSAGES) {
            role_permissions &= Permissions::KICK_MEMBERS
                | Permissions::BAN_MEMBERS
                | Permissions::ADMINISTRATOR
                | Permissions::MANAGE_SERVER
                | Permissions::CHANGE_NICKNAMES
                | Permissions::MANAGE_NICKNAMES;
        }

        // Text channel => no voice actions
        if text_channel {
            role_permissions &= !(Permissions::VOICE_CONNECT
                | Permissions::VOICE_SPEAK
                | Permissions::VOICE_MUTE_MEMBERS
                | Permissions::VOICE_DEAFEN_MEMBERS
                | Permissions::VOICE_MOVE_MEMBERS
                | Permissions::VOICE_USE_VOICE_ACTIVITY);
        }
        role_permissions
    }
}

/// A server which may be unavailable
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PossibleServer<T> {
    /// An online server, for which more information is available
    Online(T),
    /// An offline server, the ID of which is known
    Offline { id: ServerId, unavailable: bool },
}

impl PossibleServer<LiveServer> {
    pub fn id(&self) -> ServerId {
        match *self {
            PossibleServer::Offline { id, .. } => id,
            PossibleServer::Online(ref ls) => ls.id,
        }
    }
}

impl PossibleServer<Server> {
    pub fn id(&self) -> ServerId {
        match *self {
            PossibleServer::Offline { id, .. } => id,
            PossibleServer::Online(ref ls) => ls.id,
        }
    }
}

// Member

/// Information about a member of a server
///
/// https://discord.com/developers/docs/resources/guild#guild-member-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    /// The user this member structure concerns.
    ///
    /// This field is set to `None` when received as part of a message event.
    pub user: Option<User>,

    /// The user's server nickname.
    pub nick: Option<String>,
    /// The user's server specific avatar.
    pub avatar: Option<String>,

    /// The roles granted to the user
    pub roles: Vec<RoleId>,

    /// When the user joined this server.
    pub joined_at: DateTime<FixedOffset>,

    /// If the user has muted themselves in VC
    pub mute: bool,
    /// If the user has deafened themselves in VC
    pub deaf: bool,

    /// Server member flags
    pub flags: MemberFlags,

    /// When the user started boosting this server.
    #[serde(rename = "premium_since")]
    pub boosting_since: Option<DateTime<FixedOffset>>,
}

impl Member {
    /// Get this member's nickname if present or their username otherwise.
    pub fn display_name(&self) -> Option<&str> {
        if let Some(name) = self.nick.as_ref() {
            Some(name)
        } else {
            self.user.as_ref().map(|member| member.name.as_str())
        }
    }
}

bitflags! {
    /// Odd member information.
    ///
    /// https://discord.com/developers/docs/resources/guild#guild-member-object-guild-member-flags
    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    struct MemberFlags: u8 {
        /// Member has left and rejoined the server
        const DID_REJOIN = 1 << 0;
        /// Member has completed onboarding
        const COMPLETED_ONBOARDING = 1 << 1;
        /// Member is exempt from server verification requirements
        const BYPASSES_VERIFICATION = 1 << 2;
        /// Member has started onboarding
        const STARTED_ONBOARDING = 1 << 3;
    }
}

// Presence

/// A members's online status
#[derive(Debug, Clone)]
pub struct Presence {
    /// The user this ID belongs to
    pub user_id: UserId,

    /// The status of this user
    pub status: OnlineStatus,
    /// The last time the presence was updated
    pub last_modified: Option<u64>,

    /// user's current activities
    pub activities: Vec<Activity>,
}

/// A user's online presence status
///
/// https://discord.com/developers/docs/topics/gateway-events#update-presence-status-types
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum OnlineStatus {
    /// The user will not receive notifications.
    #[serde(rename = "dnd")]
    DoNotDisturb,
    /// The user appears offline.
    #[serde(rename = "invisible")]
    Invisible,
    /// The user is presumed to not be online.
    #[serde(rename = "offline")]
    Offline,
    /// The user is online and active within discord.
    #[serde(rename = "online")]
    Online,
    /// The user will not receive notifications on a given device.
    #[serde(rename = "idle")]
    Idle,
}

/// User's activity
/// https://discord.com/developers/docs/topics/gateway#activity-object
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    /// The activity's name.
    pub name: String,

    /// 0 - Game, 1 - Streaming, 2 - Listening, 4 - Custom
    /// https://discord.com/developers/docs/topics/gateway#activity-object-activity-types
    #[serde(rename = "type")]
    pub kind: ActivityType,

    /// Stream url, is validated as a URL
    /// when type is [`ActivityType::Streaming`], but may always be set.
    pub url: Option<String>,

    /// Timestamp of when the activity was added to the user's session.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,

    /// Application ID for the game
    pub application_id: Option<String>,

    /// What the player is currently doing.
    pub details: Option<String>,
    /// User's current party status, or text used for a custom status.
    pub state: Option<String>,
    /// The emoji used for a custom status
    pub emoji: Option<ActivityEmoji>,

    pub party: (),   /* ActivityParty */
    pub assets: (),  /* ActivityAssets */
    pub secrets: (), /* ActivitySecrets */

    /// Whether or not the activity is an instanced game session.
    pub instance: bool,

    /// Activity flags `OR`d together, describes what the payload includes.
    pub flags: ActivityFlags,

    /// Custom buttons shown in the Rich Presence (max 2).
    pub buttons: Option<[(); 2]>,
}

bitflags! {
    /// Informational flags about an activity and what can be done with it.
    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct ActivityFlags: u16 {
        const INSTANCE = 1 << 0;
        const JOIN = 1 << 1;
        const SPECTATE = 1 << 2;
        const JOIN_REQUEST = 1 << 3;
        const SYNC = 1 << 4;
        const PLAY = 1 << 5;
        const PARTY_PRIVACY_FRIENDS = 1 << 6;
        const PARTY_PRIVACY_VOICE_CHANNEL = 1 << 7;
        const EMBEDDED = 1 << 8;
    }
}

/// A type of game being played.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ActivityType {
    /// The user is playing a game
    Playing = 0,
    /// The user is streaming somewhere.
    /// This game type is accompanied by a purple play button icon in the user's presence.
    Streaming = 1,
    /// The user is listening to music on spotify or youtube music.
    Listening = 2,
    /// The user has set a custom text status.
    Custom = 4,
    /// The user is competing.
    ///
    /// This activity type goes basically unused
    /// except as flavor text for bot presences.
    Competing = 5,
}

/// the emoji used for a custom status
/// https://discord.com/developers/docs/topics/gateway#activity-object-activity-emoji
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEmoji {
    pub name: String,
    pub id: Option<EmojiId>,
    pub animated: Option<bool>,
}

// Messages

/// Message transmitted over a text channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub content: String,
    // carry on if nonce is absent or for some reason not a string
    #[serde(deserialize_with = "crate::serial::ignore_errors")]
    #[serde(default)]
    pub nonce: Option<String>,
    pub tts: bool,
    pub timestamp: DateTime<FixedOffset>,
    pub edited_timestamp: Option<DateTime<FixedOffset>>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub kind: MessageType,

    pub author: User,
    pub mention_everyone: bool,
    pub mentions: Vec<User>,
    pub mention_roles: Vec<RoleId>,
    #[serde(default)]
    pub reactions: Vec<MessageReaction>,

    pub attachments: Vec<Attachment>,
    /// Follows OEmbed standard
    pub embeds: Vec<Value>,
    pub referenced_message: Option<Box<Message>>,

    pub flags: MessageFlags,
}

/// The type of a message
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize_repr, Deserialize_repr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum MessageType {
    /// A regular, text-based message
    Default = 0,

    /// A recipient was added to the group
    #[serde(rename = "RECIPIENT_ADD")]
    GroupRecipientAdded = 1,
    /// A recipient was removed from the group
    #[serde(rename = "RECIPIENT_REMOVE")]
    GroupRecipientRemoved = 2,

    /// A group call was created
    #[serde(rename = "CALL")]
    GroupCall = 3,
    /// A group name was updated
    GroupNameChange = 4,
    /// A group icon was updated
    GroupIconChange = 5,

    /// A message was pinned
    #[serde(rename = "CHANNEL_PINNED_MESSAGE")]
    MessagePinned = 6,

    /// A user joined a server and a welcome message was generated
    #[serde(rename = "USER_JOIN")]
    UserJoined = 7,

    /// Server has been boosted.
    #[serde(rename = "GUILD_BOOST")]
    ServerBoost = 8,
    /// Server has been boosted and just reached level 1 boost.
    #[serde(rename = "GUILD_BOOST_TIER_1")]
    ServerBoostTier1 = 9,
    /// Server has been boosted and just reached level 2 boost.
    #[serde(rename = "GUILD_BOOST_TIER_2")]
    ServerBoostTier2 = 10,
    /// Server has been boosted and just reached level 3 boost.
    #[serde(rename = "GUILD_BOOST_TIER_3")]
    ServerBoostTier3 = 11,

    ChannelFollowAdd = 12,
    /// The server is no longer in the discovery directory.
    GuildDiscoveryDisqualified = 14,
    /// The server has met the criteria for entering the discovery directory again.
    GuildDiscoveryRequalified = 15,
    /// The server has dropped from the criteria for server discovery
    /// and the first warning has been sent.
    GuildDiscoveryGracePeriodInitialWarning = 16,
    /// The server has dropped from the criteria for server discovery
    /// and the last warning has been sent.
    GuildDiscoveryGracePeriodFinalWarning = 17,
    /// A user started a thread
    ThreadCreated = 18,
    // Replies only have type `19` in API v8. In v6, they are still type `0`.
    /// A reply message.
    Reply = 19,
    /// A bot has responded to a command.
    ChatInputCommand = 20,
    /// A message that a standalone thread has been
    /// started without a message in the main channel.
    ThreadStarterMessage = 21,
    GuildInviteReminder = 22,
    ContextMenuCommand = 23,
    /// The auto moderation tool has taken an action.
    AutoModerationAction = 24,
    /// A member has bought a role subscription.
    RoleSubscriptionPurchase = 25,
    InteractionPremiumUpsell = 26,
    /// A stage session has been started.
    StageStart = 27,
    /// A stage session has ended.
    StageEnd = 28,
    /// A member has been promoted to speaker on a stage session.
    StageSpeaker = 29,
    /// The topic for the stage session has been set.
    StageTopic = 31,
    GuildApplicationPremiumSubscription = 32,
}

bitflags! {
    /// Sets of flags that may be set on a message.
    ///
    /// See https://discord.com/developers/docs/resources/channel#message-object-message-flags
    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct MessageFlags: u16 {
        const CROSSPOSTED = 1 << 0;
        const IS_CROSSPOST = 1 << 1;
        const SUPPRESS_EMBEDS = 1 << 2;
        const SOURCE_MESSAGE_DELETED = 1 << 3;
        const URGENT = 1 << 4;
    }
}

/// File upload attached to a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    /// Short filename for the attachment
    pub filename: String,
    /// Shorter URL with message and attachment id
    pub url: String,
    /// Longer URL with large hash
    pub proxy_url: String,
    /// Size of the file in bytes
    pub size: u64,
    /// Width if the file is an image
    pub width: Option<u64>,
    /// Height if the file is an image
    pub height: Option<u64>,
}

impl Attachment {
    /// Get the dimensions of the attachment if it is an image.
    pub fn dimensions(&self) -> Option<(u64, u64)> {
        if let (&Some(w), &Some(h)) = (&self.width, &self.height) {
            Some((w, h))
        } else {
            None
        }
    }
}

// Message reactions

/// A full single reaction interaction.
///
/// Contains no information about the bulk of
/// the users who may have reacted with an emoji
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub user_id: UserId,
    pub emoji: ReactionEmoji,
}

/// Information on a reaction as available at a glance on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReaction {
    /// The amount of people that have reacted with this emoji
    pub count: u64,
    /// If the current user has placed this reaction
    pub me: bool,
    /// The emoji used to react
    pub emoji: ReactionEmoji,
}

/// Emoji information sent only from reaction events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReactionEmoji {
    /// A user reacted with a custom emoji.
    Custom {
        /// The name that is displayed for the emoji
        name: String,
        /// The ID of the emoji
        id: EmojiId,
        /// If the emoji is animated.
        #[serde(default)]
        animated: bool,
    },
    /// A user reacted with a stock unicode emoji.
    Unicode {
        /// The name is set to the emoji used
        name: String,
    },
}

// Voice States & Regions

/// A member's state within a voice channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceState {
    /// The user this voice state concerns.
    pub user_id: UserId,
    /// The server member this voice state is for
    pub member: Option<Member>,

    /// The voice channel they are connected to.
    pub channel_id: Option<ChannelId>,
    /// The server this voice state is about
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The session ID of this voice state
    pub session_id: String,

    /// The token that can be used to connect to the voice server (?)
    pub token: Option<String>,

    /// Whether this user's permission to speak has been suppressed
    pub suppress: bool,

    /// If the user has muted themselves locally
    #[serde(rename = "self_mute")]
    pub mute: bool,
    /// If the user has deafened themselves locally
    #[serde(rename = "self_deaf")]
    pub deaf: bool,

    /// If the user is broadcasting video via a webcam
    #[serde(rename = "self_video")]
    pub video: bool,
    /// If the user is streaming using the `Go Live` feature
    #[serde(rename = "self_stream")]
    #[serde(default)]
    pub streaming: bool,

    /// If the user has been muted by an administrator
    #[serde(rename = "mute")]
    pub server_mute: bool,
    /// If the user has been deafened by an administrator
    #[serde(rename = "deaf")]
    pub server_deaf: bool,

    /// The time at which the user requested to speak
    pub request_to_speak_timestamp: Option<DateTime<FixedOffset>>,
}

/// Information about an available voice region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRegion {
    /// Unique ID for this region.
    pub id: String,
    /// Human name for the voice region.
    pub name: String,

    /// True for a single server that is closest to the current user's client.
    pub optimal: bool,
    /// Whether this is a deprecated voice region (avoid switching to these).
    pub deprecated: bool,

    /// If this voice region supports 386Kbps audio.
    ///
    /// This field is not documented in the discord developer documentation.
    /// The assumed meaning of this field has not been yet confirmed by the library authors.
    pub vip: bool,

    /// Probably is a guess about the hostname of the region server by the main gateway.
    ///
    /// This field is not documented in the discord developer documentation.
    /// The assumed meaning of this field has not been yet confirmed by the library authors.
    pub sample_hostname: String,
    /// Probably is a guess about the open port of the region server by the main gateway.
    ///
    /// This field is not documented in the discord developer documentation.
    /// The assumed meaning of this field has not been yet confirmed by the library authors.
    pub sample_port: u16,
}

// Ongoing private calls

/// An active group or private call, that involves the current user.
///
/// Bots may not observe any calls, as they cannot receive any.
///
/// This field is not documented in the discord developer documentation.
/// The assumed meaning of this field has not been yet confirmed by the library authors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    /// Which group chat or DM channel the call is in.
    pub channel_id: ChannelId,

    /// The ID of the message that archives the existence.
    pub message_id: MessageId,

    /// The voice region ID that this call is hosted on.
    pub region: String,
    /// The voice states of participants who have joined.
    pub voice_states: Vec<VoiceState>,

    /// The list of participants who are currently being ringed.
    pub ringing: Vec<UserId>,

    pub unavailable: bool,
}

// Information about missed messages

/// Summary of messages since last login for an accessible channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnreadMessages {
    /// Id of the relevant channel
    pub id: ChannelId,
    /// Last seen message in this channel
    pub last_message_id: Option<MessageId>,
    /// Mentions since that message in this channel
    #[serde(default)]
    pub mention_count: u64,
}
