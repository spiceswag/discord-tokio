//! Data models for resources returned from the Discord REST API.
//!
//! The name `frozen` comes from the fact that data modeled
//! in this module rarely changes, like server settings, channel configurations,
//! and so on, so they are accessible from the REST API.
//!
//! Resources that do not fit into the above description i.e.
//! instances of them are managed by an active gateway connection,
//! are defined in the sister module `live`.

use std::{borrow::Cow, fmt};

use bitflags::bitflags;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::serial::Eq;

// IDs

macro_rules! snowflake {
    ($(#[$attr:meta] $name:ident;)*) => {
        $(
            #[$attr]
            ///
            /// Identifiers can be debug-printed using the `{:?}` specifier, or their
            /// raw number value printed using the `{}` specifier.
            /// Some identifiers have `mention()` methods as well.
            #[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
            #[derive(Serialize, Deserialize)]
            pub struct $name(#[serde(deserialize_with = "crate::serial::deserialize_id")] pub u64);

            impl $name {
                /// Get the creation date of the object referred to by this ID.
                ///
                /// Discord generates identifiers using a scheme based on [Twitter Snowflake]
                /// (https://github.com/twitter/snowflake/tree/b3f6a3c6ca8e1b6847baa6ff42bf72201e2c2231#snowflake).
                pub fn creation_date(&self) -> DateTime<Utc> {
                    let naive = NaiveDateTime::from_timestamp((1420070400 + (self.0 >> 22) / 1000) as i64, 0);
                    DateTime::from_utc(naive, Utc)
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        )*
    }
}

snowflake! {
    /// Bots are identified sometimes by their application ID
    ApplicationId;
    /// An identifier for a User
    UserId;
    /// An identifier for a Server
    ServerId;
    /// An identifier for a Channel
    ChannelId;
    /// An identifier for a Message
    MessageId;
    /// An identifier for a Role
    RoleId;
    /// An identifier for an Emoji
    EmojiId;
    /// An identifier for a sticker
    StickerId;
    /// An identifier for a standard sticker pack.
    StickerPackId;
    /// An identifier for a scheduled server event
    EventId;
}

// Users

/// Frozen user information, accessible without being friends with that user.
///
/// Users in Discord are generally considered the base entity.
/// Users can spawn across the entire platform, be members of guilds,
/// participate in text and voice chat, and much more.
///
/// # User Vs bot considerations
///  
/// Users are separated by a distinction of "bot" vs "normal".
/// Although they are similar, bot users are automated users that are "owned" by another user.
/// Unlike normal users, bot users do not have a limitation on the number of Guilds they can be a part of.
///
/// https://discord.com/developers/docs/resources/user#user-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// The user's ID
    pub id: UserId,

    /// The user's username, not unique across the platform.
    #[serde(rename = "username")]
    pub name: String,
    /// The user's Discord-tag
    #[serde(deserialize_with = "crate::serial::deserialize_discrim")]
    pub discriminator: u16,

    /// The user's avatar hash
    pub avatar: Option<String>,

    /// Whether the user belongs to an OAuth2 application
    #[serde(default)]
    pub bot: bool,
}

impl User {
    /// Return a `Mention` which will ping this user.
    #[inline(always)]
    pub fn mention(&self) -> Mention {
        self.id.mention()
    }

    /// Returns the formatted URL of the user's icon.
    ///
    /// Returns None if the user does not have an avatar.
    pub fn avatar_url(&self) -> Option<String> {
        self.avatar
            .as_ref()
            .map(|avatar_hash| format!(cdn_concat!("/avatars/{}/{}.jpg"), self.id, avatar_hash))
    }
}

/// Information about the logged-in user
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct CurrentUser {
    pub id: UserId,
    pub username: String,
    #[serde(deserialize_with = "crate::serial::deserialize_discrim")]
    pub discriminator: u16,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub verified: bool,
    #[serde(default)]
    pub bot: bool,
    pub mfa_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct CurrentUserPatch {
    /// The ID of the current user
    pub id: Option<UserId>,

    /// The non unique username of the current user
    pub username: Option<String>,
    /// The Discord-tag of the current user
    #[serde(deserialize_with = "crate::serial::deserialize_discrim_opt")]
    pub discriminator: Option<u16>,

    /// The current user's avatar hash
    pub avatar: Option<String>,

    /// The current user's email address, if not a bot account
    pub email: Option<String>,
    /// If the email set for the user has been verified
    pub verified: Option<bool>,
    /// Does the current user has multi factor authentication enabled
    pub mfa_enabled: Option<bool>,

    /// If the current user is a bot or not
    #[serde(default)]
    pub bot: Option<bool>,
}

impl CurrentUser {
    pub fn update_from(&mut self, patch: &CurrentUserPatch) {
        update_field(&mut self.id, &patch.id);
        update_field(&mut self.username, &patch.username);
        update_field(&mut self.discriminator, &patch.discriminator);
        update_field_opt(&mut self.avatar, &patch.avatar);
        update_field_opt(&mut self.email, &patch.email);
        update_field(&mut self.verified, &patch.verified);
        update_field(&mut self.bot, &patch.bot);
        update_field(&mut self.mfa_enabled, &patch.mfa_enabled);
    }
}

/// Information on a friendship relationship this user has with another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: UserId,
    #[serde(rename = "type")]
    pub kind: RelationshipType,
    pub user: User,
}

/// A type of relationship this user has with another.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum RelationshipType {
    /// A user has sent a friend request which was consequently ignored.
    Ignored = 0,
    /// The two users are friends.
    Friends = 1,
    /// One user has blocked the other.
    Blocked = 2,
    /// One user has sent this user a friend request.
    IncomingRequest = 3,
    /// One user has sent another a friend request.
    OutgoingRequest = 4,
}

/// Flags for who may add a user as a friend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendSourceFlags {
    /// Anybody can add this user as a friend.
    #[serde(default)]
    pub all: bool,

    /// Only friends of friends my add this user.
    #[serde(default)]
    pub mutual_friends: bool,

    /// Only people in a server with this user may add them.
    #[serde(default)]
    #[serde(rename = "mutual_guilds")]
    pub mutual_servers: bool,
}

// Random **NECESSARY** utilities

fn update_field<T: Clone>(item: &mut T, patch: &Option<T>) {
    if let Some(value) = patch.clone() {
        *item = value;
    }
}

fn update_field_opt<T: Clone>(item: &mut Option<T>, patch: &Option<T>) {
    if let Some(value) = patch.clone() {
        *item = Some(value);
    }
}

// Servers

/// Complete information about a server obtainable only through joining it.
///
/// This information is not likely to change therefore its
/// defined here, in the `frozen` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// The ID of the server.
    pub id: ServerId,
    /// The name of the server (2-100 characters).
    pub name: String,
    /// The icon hash of the server.
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub icon: Option<String>,

    /// The banner image (splash) hash of the server.
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub splash: Option<String>,

    /// The hash of the banner image (splash) displayed
    /// in the public server discovery provided by Discord.
    ///
    /// Only present for guilds with the "DISCOVERABLE" feature
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub discovery_splash: Option<String>,

    /// True if the requesting user is the owner of the guild.
    pub owner: bool,
    /// The owner of the guild
    pub owner_id: UserId,

    /// Total permissions for the user in the guild
    /// (excludes channel and category overwrites and implicit permissions)
    pub permissions: Permissions,

    /// Voice region id for the guild (deprecated)
    #[deprecated(note = "this field is replaced by a dedicated field on each voice channel")]
    pub region: String,

    /// Voice AFK timeout in seconds,
    /// after which the user will be moved to the AFK channel.
    pub afk_timeout: u64,
    /// The ID of the AFK voice channel.
    pub afk_channel_id: Option<ChannelId>,

    /// True if the server widget is enabled
    pub widget_enabled: bool,
    /// The channel ID that the widget will generate an invite to, or `None` if set to no invite.
    pub widget_channel_id: Option<ChannelId>,

    /// User verification level to be able to use the server
    pub verification_level: VerificationLevel,

    /// Default message notification level.
    ///
    /// https://discord.com/developers/docs/resources/guild#guild-object-default-message-notification-level
    pub default_message_notifications: u8,
    /// Explicit content filter level.
    ///
    /// https://discord.com/developers/docs/resources/guild#guild-object-explicit-content-filter-level
    pub explicit_content_filter: u8,

    /// A list of all roles in the server.
    pub roles: Vec<Role>,

    /// A list of custom emojis in the server.
    pub emojis: Vec<Emoji>,

    /// Array of server features enabled for this server.
    pub features: Vec<ServerFeature>,

    /// Required multi factor authentication level.
    ///
    /// https://discord.com/developers/docs/resources/guild#guild-object-mfa-level
    pub mfa_level: u8,

    // pub application_id: ApplicationId
    /// The id of the channel where server notices
    /// such as welcome messages and boost events are posted.
    pub system_channel_id: Option<ChannelId>,
    /// System channel flags.
    ///
    /// https://discord.com/developers/docs/resources/guild#guild-object-system-channel-flags
    pub system_channel_flags: u64,

    /// The ID of the channel where Community servers can display rules and/or guidelines.
    pub rules_channel_id: Option<ChannelId>,

    /// The maximum number of presences for the server
    /// (null is always returned, apart from the largest of servers)
    pub max_presences: Option<u64>,
    /// The maximum number of members for the server
    pub max_members: Option<u64>,

    /// The vanity url code for the server
    pub vanity_url_code: Option<String>,

    /// The description of a server
    pub description: Option<String>,
    /// The banner image (splash) hash of the server.
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub banner: Option<String>,

    /// Server boost level.
    #[serde(rename = "premium_tier")]
    pub boost_tier: u8,
    /// The number of boosts this server currently has
    #[serde(rename = "premium_subscription_count")]
    pub boost_subscription_count: Option<u64>,

    /// The preferred locale of a Community server;
    /// used in server discovery and notices from Discord,
    /// and sent in interactions; defaults to "en-US".
    pub preferred_locale: String,

    /// The id of the channel where admins and moderators
    /// of Community guilds receive notices from Discord.
    pub public_updates_channel_id: Option<ChannelId>,

    /// The maximum amount of users in a voice turned video channel.
    pub max_video_channel_users: u64,
    /// The maximum amount of users in a stage video channel.
    pub max_stage_video_channel_users: u64,

    /// Approximate number of members in this guild,
    /// returned from the `GET` `/guilds/<id>` and `/users/@me/guilds`
    /// endpoints when `with_counts` is `true`.
    pub approximate_member_count: u64,

    /// Approximate number of non-offline members in this guild,
    /// returned from the `GET` `/guilds/<id>` and `/users/@me/guilds`
    /// endpoints when `with_counts` is `true`.
    pub approximate_presence_count: u64,

    /// The welcome screen of a Community guild, shown to new members,
    /// returned in an Invite's server object.
    pub welcome_screen: WelcomeScreen,

    /// The server's self assigned NSFW rating.
    #[serde(rename = "nsfw_level")]
    pub nsfw: NsfwLevel,

    /// Custom server stickers
    pub stickers: Option<Vec<() /* Sticker */>>,

    /// Whether the guild has the boost progress bar enabled.
    #[serde(rename = "premium_progress_bar_enabled")]
    pub boost_progress_bar_enabled: bool,

    /// The ID of the channel where admins and moderators
    /// of Community guilds receive safety alerts from Discord.
    pub safety_alerts_channel_id: Option<ChannelId>,
}

impl Server {
    /// Returns the formatted URL of the server's icon.
    ///
    /// Returns `None` if the server does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/icons/{}/{}.jpg"), self.id, icon))
    }

    /// Returns the formatted URL of the server's banner.
    ///
    /// Returns `None` if the server does not have an banner.
    pub fn banner_url(&self) -> Option<String> {
        self.banner
            .as_ref()
            .map(|banner| format!(cdn_concat!("/icons/{}/{}.jpg"), self.id, banner))
    }
}

/// The welcome screen shown to new users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeScreen {
    /// The server description shown in the welcome screen.
    pub description: Option<String>,
    /// The channels shown in the welcome screen, up to 5.
    pub welcome_channels: Vec<WelcomeChannels>,
}

/// One of the channels shown on the welcome screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeChannels {
    /// The channel's ID.
    pub channel_id: ChannelId,
    /// The description shown for the channel.
    pub description: String,
    /// The emoji id, if the emoji is custom.
    pub emoji_id: EmojiId,
    /// The emoji name if custom, the unicode character if standard,
    /// or null if no emoji is set.
    pub emoji_name: Option<String>,
}

/// Basic information about a Discord server.
/// Viewable without needing to be a member if
/// the guild is in the official public server discovery.
///
/// https://discord.com/developers/docs/resources/guild#guild-preview-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPreview {
    /// The ID of the server.
    pub id: ServerId,
    /// The name of the server (2-100 characters).
    pub name: String,
    /// The icon hash of the server.
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub icon: Option<String>,

    /// The banner image (splash) hash of the server.
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    #[serde(rename = "splash")]
    pub banner: Option<String>,

    /// The hash of the banner image (splash) displayed
    /// in the public server discovery provided by Discord.
    ///
    /// Only present for guilds with the "DISCOVERABLE" feature
    ///
    /// https://discord.com/developers/docs/reference#image-formatting
    pub discovery_splash: Option<String>,

    /// Custom server emojis.
    pub emojis: Vec<Emoji>,

    /// A list of enabled server features.
    pub features: Vec<ServerFeature>,

    /// Approximate number of members in this server.
    pub approximate_member_count: u64,

    /// Approximate number of online members in this server.
    pub approximate_presence_count: u64,

    /// The description for the server.
    pub description: Option<String>,
}

impl ServerPreview {
    /// Returns the formatted URL of the server's icon.
    ///
    /// Returns None if the server does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/icons/{}/{}.jpg"), self.id, icon))
    }

    /// Returns the formatted URL of the server's banner.
    ///
    /// Returns None if the server does not have an banner.
    pub fn banner_url(&self) -> Option<String> {
        self.banner
            .as_ref()
            .map(|icon| format!(cdn_concat!("/splashes/{}/{}.jpg"), self.id, icon))
    }

    /// Returns the formatted URL of the server's discovery banner.
    ///
    /// Returns None if the server does not have an discovery banner.
    pub fn discovery_splash_url(&self) -> Option<String> {
        self.discovery_splash
            .as_ref()
            .map(|icon| format!(cdn_concat!("/discovery-splashes/{}/{}.jpg"), self.id, icon))
    }
}

/// An enabled feature for a guild.
/// Most commonly allocated for feature roll-outs.
///
/// https://discord.com/developers/docs/resources/guild#guild-object-guild-features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerFeature {
    // server cosmetics (nitro boost or not)
    /// Server has access to set a server banner image
    Banner,
    /// Server has access to set an animated server banner image
    AnimatedBanner,
    /// Server has access to set an animated server icon
    AnimatedIcon,
    /// Server has access to set an invite splash background
    InviteSplash,

    // discord gives servers badges
    /// Server is verified by discord
    Verified,
    /// Server can allocate a custom permanent `https://discord.gg` invite code
    VanityUrl,
    /// Server is partnered with discord
    Partnered,

    /// Server has been set as a support server for an app on the App Directory
    DeveloperSupportServer,

    // discord recommends servers to you
    /// Server can enable welcome screen, Membership Screening,
    /// stage channels and discovery, and receives community updates.
    Community,
    /// Server is able to be discovered in the directory
    Discoverable,
    /// Server is able to be featured in the directory
    Featurable,

    // discord patreon wannabe
    /// Server has enabled monetization
    #[serde(rename = "CREATOR_MONETIZABLE_PROVISIONAL")]
    CreatorMonetization,
    /// Server has enabled the role subscription promo page
    CreatorStorePage,

    // discord creator economy
    /// Server has role subscriptions that can be purchased
    #[serde(rename = "ROLE_SUBSCRIPTIONS_AVAILABLE_FOR_PURCHASE")]
    RoleSubscriptionsPurchasable,
    /// Server has enabled role subscriptions
    RoleSubscriptionsEnabled,
    /// Server has enabled ticketed events
    TicketedEventsAvailable,

    // nitro boost features
    /// Server has increased custom sticker slots
    MoreStickers,
    /// Server is able to set role icons
    RoleIcons,
    /// Server has access to set 384Kbps bitrate in voice (previously VIP voice servers)
    VipRegions,

    // server join utilities
    /// Server has paused invites, preventing new users from joining
    InvitesDisabled,
    /// Server can be previewed before joining via Membership Screening or the directory
    PreviewEnabled,
    /// Server has enabled the welcome screen
    WelcomeScreenEnabled,
    /// Server has enabled Membership Screening
    #[serde(rename = "MEMBER_VERIFICATION_GATE_ENABLED")]
    MembershipScreening,

    // random roll-outs
    /// Server has access to create announcement channels
    News,
    /// Server has set up auto moderation rules
    AutoModeration,
    /// Server is using the **old** permissions configuration behavior
    ApplicationCommandPermissionsV2,

    // random settings
    /// Server has disabled alerts for join raids in the configured safety alerts channel
    RaidAlertsDisabled,
}

/// A condition that new users must satisfy before posting in text channels
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum VerificationLevel {
    /// No verification is needed
    None = 0,
    /// Must have a verified email on their Discord account
    Low = 1,
    /// Must also be registered on Discord for longer than 5 minutes
    Medium = 2,
    /// Must also be a member of this server for longer than 10 minutes
    High = 3,
    /// Must have a verified phone on their Discord account
    Phone = 4,
}

/// A server's NSFW rating.
///
/// https://discord.com/developers/docs/resources/guild#guild-object-guild-nsfw-level
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum NsfwLevel {
    #[default]
    Default = 0,
    Explicit = 1,
    Safe = 2,
    AgeRestricted = 3,
}

/// A banning of a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ban {
    reason: Option<String>,
    user: User,
}

/// Representation of the number of member that would be pruned by a server
/// prune operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPrune {
    pub pruned: u64,
}

impl ServerId {
    /// Get the ID of the server's `@everyone` role.
    ///
    /// ```ignore
    /// let mention_everyone = server.id.everyone().mention();
    /// ````
    pub fn everyone(&self) -> RoleId {
        RoleId(self.0)
    }
}

// Roles

/// Roles represent a set of permissions attached to a group of users.
/// Roles have names, colors, and can be "pinned" to the side bar,
/// causing their members to be listed separately.
///
/// Roles can have separate permission profiles for the global context (server) and channel context.
/// The `@everyone` role has the same ID as the guild it belongs to.
///
/// https://discord.com/developers/docs/topics/permissions#role-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// The ID of the role.
    pub id: RoleId,
    /// The name of the role.
    pub name: String,

    /// Color in `0xRRGGBB` form
    pub color: u64,

    /// Hash of the role icon iaage.
    pub icon: Option<String>,

    /// The unicode icon of the emoji.
    pub unicode_emoji: Option<String>,

    /// If this role is pinned in the user listing
    pub hoist: bool,

    /// If this role belongs to a bot user and is managed by their application.
    pub managed: bool,

    /// Position of this role.
    pub position: i64,

    /// Whether this role can be mentioned.
    #[serde(default)] // default to false
    pub mentionable: bool,

    /// The permissions granted by this role.
    pub permissions: Permissions,

    /// Other flags.
    pub flags: RoleFlags,
    // nah
    // pub tags: (),
}

impl Role {
    /// Return a `Mention` which will ping members of this role.
    #[inline(always)]
    pub fn mention(&self) -> Mention {
        self.id.mention()
    }

    /// Returns the formatted URL of the role's icon.
    ///
    /// Returns `None` if the role does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/role-icons/{}/{}.jpg"), self.id, icon))
    }
}

bitflags! {
    /// Additional role flags.
    #[derive(Default, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct RoleFlags: u8 {
        /// The role is selectable in the onboarding prompt.
        const AVAILABLE_IN_PROMPT = 1;
    }
}

// Permissions

bitflags! {
    /// Set of permissions assignable to a Role or PermissionOverwrite
    #[derive(Default, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct Permissions: u64 {
        const CREATE_INVITE = 1;
        const KICK_MEMBERS = 1 << 1;
        const BAN_MEMBERS = 1 << 2;
        /// Grant all permissions, bypassing channel-specific permissions
        const ADMINISTRATOR = 1 << 3;
        /// Modify roles below their own
        const MANAGE_ROLES = 1 << 28;
        /// Create channels or edit existing ones
        const MANAGE_CHANNELS = 1 << 4;
        /// Change the server's name or move regions
        const MANAGE_SERVER = 1 << 5;
        /// Change their own nickname
        const CHANGE_NICKNAMES = 1 << 26;
        /// Change the nickname of other users
        const MANAGE_NICKNAMES = 1 << 27;
        /// Manage the emojis in a a server.
        const MANAGE_EMOJIS = 1 << 30;
        /// Manage channel webhooks
        const MANAGE_WEBHOOKS = 1 << 29;

        const READ_MESSAGES = 1 << 10;
        const SEND_MESSAGES = 1 << 11;
        /// Send text-to-speech messages to those focused on the channel
        const SEND_TTS_MESSAGES = 1 << 12;
        /// Delete messages by other users
        const MANAGE_MESSAGES = 1 << 13;
        const EMBED_LINKS = 1 << 14;
        const ATTACH_FILES = 1 << 15;
        const READ_HISTORY = 1 << 16;
        /// Trigger a push notification for an entire channel with "@everyone"
        const MENTION_EVERYONE = 1 << 17;
        /// Use emojis from other servers
        const EXTERNAL_EMOJIS = 1 << 18;
        /// Add emoji reactions to messages
        const ADD_REACTIONS = 1 << 6;

        const VOICE_CONNECT = 1 << 20;
        const VOICE_SPEAK = 1 << 21;
        const VOICE_MUTE_MEMBERS = 1 << 22;
        const VOICE_DEAFEN_MEMBERS = 1 << 23;
        /// Move users out of this channel into another
        const VOICE_MOVE_MEMBERS = 1 << 24;
        /// When denied, members must use push-to-talk
        const VOICE_USE_VOICE_ACTIVITY = 1 << 25;
    }
}

/// A channel-specific permission overwrite for a role or member.
///
/// https://discord.com/developers/docs/resources/channel#overwrite-object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionOverwrite {
    /// A permission overwrite targeting users with a given role.
    Role {
        /// The ID of the role this overwrite is about.
        id: RoleId,

        /// Permissions to allow for this role.
        #[serde(default)]
        allow: Permissions,
        /// Permissions to deny for this role.
        #[serde(default)]
        deny: Permissions,

        #[serde(rename = "type")]
        kind: Eq<0>,
    },

    /// A permission overwrite targeting a specific user.
    Member {
        /// The ID of the member this overwrite is about.
        id: UserId,

        /// Permissions to allow for this role.
        #[serde(default)]
        allow: Permissions,
        /// Permissions to deny for this role.
        #[serde(default)]
        deny: Permissions,

        #[serde(rename = "type")]
        kind: Eq<1>,
    },
}

/// The ID of a permission overwrite entity.
#[derive(Debug, Clone, Serialize)]
pub enum PermissionOverwriteId {
    /// The permission overwrite concerns a member.
    Member(UserId),
    /// The permission overwrite concerns a role.
    Role(RoleId),
}

// Channels

/// A private or public channel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Channel {
    /// Text channel to another user
    DirectMessage(DirectMessage),
    /// A group channel separate from a server
    Group(Group),
    /// Voice or text channel within a server
    Server(ServerChannel),
}

/// Private text channel to another user.
///
/// https://discord.com/developers/docs/resources/channel#channel-object
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectMessage {
    /// The ID of the DM
    pub id: ChannelId,

    /// The ID of the last message sent.
    pub last_message_id: Option<MessageId>,

    /// When the last pinned message was pinned.
    ///  
    /// This may be null in events such as GUILD_CREATE when a message is not pinned.
    pub last_pin_timestamp: Option<DateTime<FixedOffset>>,

    /// The peer at the other side of the DM
    #[serde(rename = "recipients")]
    pub recipient: [User; 1],

    #[serde(rename = "type")]
    _type: Eq<1>,
}

/// A group channel, potentially including other users, separate from a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// The ID of the group chat
    pub id: ChannelId,

    /// The hash of the custom icon.
    pub icon: Option<String>,

    /// The ID of the last message sent.
    pub last_message_id: Option<MessageId>,
    /// When the last pinned message was pinned.
    ///
    /// This may be null in events such as `GUILD_CREATE` when a message is not pinned.
    pub last_pin_timestamp: Option<DateTime<FixedOffset>>,

    /// The name of the group chat.
    pub name: Option<String>,
    /// The ID of the creator of the group chat.
    pub owner_id: UserId,

    /// Application ID of the group DM creator if it is bot-created
    pub application_id: Option<ApplicationId>,
    /// Whether the channel is managed by an application via the `gdm.join` OAuth2 scope.
    pub managed: Option<bool>,

    /// The members of the group chat.
    pub recipients: Vec<User>,

    #[serde(rename = "type")]
    _type: Eq<3>,
}

impl Group {
    /// Get this group's name, building a default if needed
    pub fn name(&self) -> Cow<str> {
        match self.name {
            Some(ref name) => Cow::Borrowed(name),
            None => {
                if self.recipients.is_empty() {
                    return Cow::Borrowed("Empty Group");
                }

                let mut result = self
                    .recipients
                    .iter()
                    .map(|user| user.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ");
                Cow::Owned(result)
            }
        }
    }

    /// Returns the formatted URL of the group's icon.
    ///
    /// Returns None if the group does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/channel-icons/{}/{}.jpg"), self.id, icon))
    }
}

/// A channel that can be found in a server.
///
/// This type is meant to be used primarily when deserializing
/// channels received from the rest API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerChannel {
    /// A text channel in a server.
    Text {
        /// The text channel data of the channel.
        #[serde(flatten)]
        channel: TextChannel,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<0>,
    },

    /// A voice channel in a server.
    Voice {
        /// The voice channel info.
        #[serde(flatten)]
        channel: VoiceChannel,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<2>,
    },

    /// A channel category in a server.
    Category {
        /// The category itself.
        #[serde(flatten)]
        category: ChannelCategory,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<4>,
    },

    /// An announcement channel in a server.
    Announcement {
        /// The announcement channel in question.
        #[serde(flatten)]
        channel: AnnouncementChannel,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<5>,
    },

    /// A temporary sub-channel within an [AnnouncementChannel].
    AnnouncementThread {
        /// The thread in question.
        #[serde(flatten)]
        thread: Thread,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<10>,
    },

    /// A temporary sub-channel within a [TextChannel] or GUILD_FORUM channel
    PublicThread {
        /// The thread in question.
        #[serde(flatten)]
        thread: Thread,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<11>,
    },

    /// A temporary sub-channel within a [TextChannel] channel
    /// that is only viewable by those invited and those with the `MANAGE_THREADS` permission.
    PrivateThread {
        /// The thread in question.
        #[serde(flatten)]
        thread: Thread,

        #[doc(hidden)]
        kind: Eq<12>,
    },
}

impl ServerChannel {
    /// Access the ID of the channel this `enum` represents.
    pub fn id(&self) -> &ChannelId {
        match self {
            Self::Text { channel, .. } => &channel.id,
            Self::Voice { channel, .. } => &channel.id,
            Self::Announcement { channel, .. } => &channel.id,
            Self::Category { category, .. } => &category.id,
            Self::AnnouncementThread { thread, .. } => &thread.id,
            Self::PublicThread { thread, .. } => &thread.id,
            Self::PrivateThread { thread, .. } => &thread.id,
        }
    }

    /// Get the type of the channel that is stored in the enum.
    pub fn kind(&self) -> ChannelType {
        match self {
            Self::Text { .. } => ChannelType::Text,
            Self::Voice { .. } => ChannelType::Voice,
            Self::Announcement { .. } => ChannelType::Announcement,
            Self::Category { .. } => ChannelType::Category,
            Self::AnnouncementThread { .. } => ChannelType::AnnouncementThread,
            Self::PublicThread { .. } => ChannelType::PublicThread,
            Self::PrivateThread { .. } => ChannelType::PrivateThread,
        }
    }

    /// Get the type of the channel that is stored in the enum.
    pub fn permission_overwrites(&self) -> &[PermissionOverwrite] {
        match self {
            Self::Text { channel, .. } => channel.permission_overwrites.as_ref(),
            Self::Voice { channel, .. } => channel.permission_overwrites.as_ref(),
            Self::Announcement { channel, .. } => channel.permission_overwrites.as_ref(),
            Self::Category { category, .. } => category.permission_overwrites.as_ref(),

            // what now
            Self::AnnouncementThread { thread, .. } => &[],
            Self::PublicThread { thread, .. } => &[],
            Self::PrivateThread { thread, .. } => &[],
        }
    }
}

/// A textual channel of a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChannel {
    /// The ID of the channel, unique across the server.
    pub id: ChannelId,

    /// The ID of the server this channel belongs to.
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The name of the channel.
    pub name: String,

    /// The order of the channel in relation to others.
    ///
    /// This value is only useful with access to the rest of the channels.
    pub position: i32,

    /// ID of the parent category for a channel (each parent category can contain up to 50 channels).
    #[serde(rename = "parent_id")]
    pub category_id: Option<ChannelId>,

    /// Permission overwrites for members or whole roles.
    pub permission_overwrites: Vec<PermissionOverwrite>,

    /// Amount of seconds a user has to wait before sending another message (0-21600).
    ///
    /// Bots, as well as users with the permission `MANAGE_MESSAGES` or `MANAGE_CHANNEL`, are unaffected.
    ///
    /// This rate limit also applies to thread creation.
    /// Users can send one message and create one thread during each `user_rate_limit` interval.
    #[serde(rename = "rate_limit_per_user")]
    pub user_rate_limit: Option<u16>,

    /// If the channel is marked as Not Safe For Work
    #[serde(default)]
    pub nsfw: bool,

    /// The topic of the channel (0-1024 characters).
    pub topic: Option<String>,

    /// The ID of the last message sent (may not point to an existing or valid message or thread).
    #[serde(rename = "last_message_id")]
    pub last_message: Option<MessageId>,

    /// When the last pinned message was pinned.
    /// This may be null in events such as `GUILD_CREATE` when a message is not pinned.
    pub last_pin_timestamp: Option<DateTime<FixedOffset>>,

    /// Default duration, copied onto newly created threads, in minutes,
    /// threads will stop showing in the channel list after the specified period of inactivity, can be set to: 60, 1440, 4320, 10080.
    pub default_auto_archive_duration: Option<u16>,
}

/// A voice channel of a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceChannel {
    /// The ID of the channel, unique across the server.
    pub id: ChannelId,

    /// The ID of the server this channel belongs to.
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The name of the channel.
    pub name: String,

    /// ID of the parent category for a channel (each parent category can contain up to 50 channels).
    #[serde(rename = "parent_id")]
    pub category_id: Option<ChannelId>,

    /// The order of the channel in relation to others.
    ///
    /// This value is only useful with access to the rest of the channels.
    pub position: i32,

    /// Permission overwrites for members or whole roles.
    pub permission_overwrites: Vec<PermissionOverwrite>,

    /// The bitrate (in bits) of the channel.
    pub bitrate: u32,

    /// The user limit on the channel for non streaming users.
    /// When a user starts streaming the limit is clamped down by discord.
    pub user_limit: u16,

    /// Voice region ID for the voice channel, automatic when set to `None`.
    pub rtc_region: Option<String>,

    /// The camera video quality mode of the voice channel.
    #[serde(default)]
    pub video_quality_mode: VideoQuality,

    /// The ID of the last message sent (may not point to an existing or valid message or thread).
    #[serde(rename = "last_message_id")]
    pub last_message: Option<MessageId>,

    /// Amount of seconds a user has to wait before sending another message (0-21600).
    ///
    /// Bots, as well as users with the permission `MANAGE_MESSAGES` or `MANAGE_CHANNEL`, are unaffected.
    ///
    /// This rate limit also applies to thread creation.
    /// Users can send one message and create one thread during each `user_rate_limit` interval.
    #[serde(rename = "rate_limit_per_user")]
    pub user_rate_limit: Option<u16>,
}

/// The video quality to be used for streaming users inside of a voice channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize_repr, Deserialize_repr, Default)]
#[repr(u8)]
pub enum VideoQuality {
    /// Discord chooses the quality for "optimal performance"
    #[default]
    Auto = 1,
    /// Full HD, also known as 720p
    Full = 2,
}

/// A category (channel) that contains up to 50 other channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCategory {
    /// The ID of the pseudo-channel
    pub id: ChannelId,

    /// The ID of the server this category is found it
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The name of the category
    pub name: String,

    /// Permission overwrites for members or whole roles.
    pub permission_overwrites: Vec<PermissionOverwrite>,

    /// The NSFW rating for the channel
    #[serde(default)]
    pub nsfw: bool,

    /// The sorting position this category occupies
    pub position: i64,
}

/// An announcement channel is a text based channel with the ability
/// to broadcast messages to subscribers in other unrelated servers.
///
/// Announcement channels are equivalent to text channels otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncementChannel {
    /// The ID of the channel, unique across the server.
    pub id: ChannelId,

    /// The ID of the server this channel belongs to.
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The name of the channel.
    pub name: String,

    /// The order of the channel in relation to others.
    ///
    /// This value is only useful with access to the rest of the channels.
    pub position: i32,

    /// ID of the parent category for a channel (each parent category can contain up to 50 channels).
    #[serde(rename = "parent_id")]
    pub category_id: Option<ChannelId>,

    /// Permission overwrites for members or whole roles.
    pub permission_overwrites: Vec<PermissionOverwrite>,

    /// Amount of seconds a user has to wait before sending another message (0-21600).
    ///
    /// Bots, as well as users with the permission `MANAGE_MESSAGES` or `MANAGE_CHANNEL`, are unaffected.
    ///
    /// This rate limit also applies to thread creation.
    /// Users can send one message and create one thread during each `user_rate_limit` interval.
    #[serde(rename = "rate_limit_per_user")]
    pub user_rate_limit: Option<u16>,

    /// If the channel is marked as Not Safe For Work
    #[serde(default)]
    pub nsfw: bool,

    /// The topic of the channel (0-1024 characters).
    pub topic: Option<String>,

    /// The ID of the last message sent (may not point to an existing or valid message or thread).
    #[serde(rename = "last_message_id")]
    pub last_message: Option<MessageId>,

    /// When the last pinned message was pinned.
    /// This may be null in events such as `GUILD_CREATE` when a message is not pinned.
    pub last_pin_timestamp: Option<DateTime<FixedOffset>>,

    /// Default duration, copied onto newly created threads, in minutes,
    /// threads will stop showing in the channel list after the specified period of inactivity, can be set to: 60, 1440, 4320, 10080.
    pub default_auto_archive_duration: Option<u16>,
}

/// A thread within a channel.
///
/// Threads can be thought of as temporary sub-channels inside an existing channel,
/// to help better organize conversation in a busy channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// The ID of the channel, unique across the server.
    pub id: ChannelId,

    /// The ID of the channel that owns this thread.
    #[serde(rename = "parent_id")]
    pub channel_id: ChannelId,

    /// The ID of the server this channel belongs to.
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,

    /// The name of the thread.
    pub name: String,

    /// The ID of the user that created this thread.
    #[serde(rename = "owner_id")]
    pub owner: UserId,

    /// The ID of the last message sent (may not point to an existing or valid message or thread).
    #[serde(rename = "last_message_id")]
    pub last_message: Option<MessageId>,

    /// The number of messages (not including the initial message or deleted messages) in the thread.
    pub message_count: u64,
    /// An approximate count of users in a thread, stops counting at 50.
    pub member_count: u64,

    /// Amount of seconds a user has to wait before sending another message (0-21600).
    ///
    /// Bots, as well as users with the permission `MANAGE_MESSAGES` or `MANAGE_CHANNEL`, are unaffected.
    #[serde(rename = "rate_limit_per_user")]
    pub user_rate_limit: Option<u16>,

    /// Number of messages ever sent in a thread,
    /// it's similar to message_count on message creation,
    /// but will not decrement the number when a message is deleted.
    #[serde(rename = "total_message_sent")]
    pub total_messages: u64,

    /// Additional data about a thread
    #[serde(rename = "thread_metadata")]
    pub thread_info: ThreadInfo,
}

/// Additional info about a thread channel.
///
/// This type is recycled for all types of thread,
/// be it public, private or in an announcement channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadInfo {
    /// Whether the thread is archived.
    pub archived: bool,
    /// whether the thread is locked; when a thread is locked, only users with `MANAGE_THREADS` can unarchive it.
    pub locked: bool,

    /// The thread will stop showing in the channel list after `auto_archive_duration`
    /// minutes of inactivity, can be set to: `60`, `1440`, `4320`, `10080`.
    pub auto_archive_duration: u16,

    /// Timestamp when the thread's archive status was last changed, used for calculating recent activity
    pub archive_timestamp: DateTime<FixedOffset>,

    /// Whether non-moderators can add other non-moderators to a thread; only available on private threads.
    pub invitable: Option<bool>,

    /// timestamp when the thread was created.
    #[serde(rename = "create_timestamp")]
    pub creation_timestamp: DateTime<FixedOffset>,
}

/// The type of a channel.
///
/// https://discord.com/developers/docs/resources/channel#channel-object-channel-types
#[derive(Debug, Clone, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ChannelType {
    /// A private channel with only one other person
    #[serde(rename = "DM")]
    DirectMessage = 1,
    /// A group channel, separate from a server
    #[serde(rename = "GROUP_DM")]
    Group = 3,

    /// A text channel in a server
    #[serde(rename = "GUILD_TEXT")]
    Text = 0,
    /// A voice channel
    #[serde(rename = "GUILD_VOICE")]
    Voice = 2,

    /// A channel category in a server
    #[serde(rename = "GUILD_CATEGORY")]
    Category = 4,

    /// A channel that users can follow and cross-post into their own server (formerly news channels)
    #[serde(rename = "GUILD_ANNOUNCEMENT")]
    Announcement = 5,

    /// A temporary sub-channel within an announcement channel
    #[serde(rename = "ANNOUNCEMENT_THREAD")]
    AnnouncementThread = 10,
    /// A temporary sub-channel within a group channel
    #[serde(rename = "PUBLIC_THREAD")]
    PublicThread = 11,
    /// A temporary sub-channel within a group channel, limited to those who are invited or have MANAGE_THREADS
    #[serde(rename = "PRIVATE_THREAD")]
    PrivateThread = 12,

    /// A voice channel for hosting events
    #[serde(rename = "PRIVATE_THREAD")]
    Stage = 13,

    /// A channel which contains a list of servers
    #[serde(rename = "GUILD_DIRECTORY")]
    Directory = 14,

    ///	A channel which exclusively contains threads
    #[serde(rename = "GUILD_FORUM")]
    Forum = 15,
    /// Channel that can only contain threads, similar to [`Forum`] channels
    ///
    /// It's currently a Work In Progress as stated by discord,
    /// and usage of it in ways
    ///
    /// [`Forum`]: crate::model::ChannelType::Forum
    #[serde(rename = "GUILD_MEDIA")]
    MediaForum = 16,
}

// Mentions

/// A mention targeted at a specific user, channel, or other entity.
///
/// A mention can be constructed by calling `.mention()` on a mentionable item
/// or an ID type which refers to it, and can be formatted into a string using
/// the `format!` macro:
///
/// ```ignore
/// let message = format!("Hey, {}, ping!", user.mention());
/// ```
///
/// If a `String` is required, call `mention.to_string()`.
pub struct Mention {
    prefix: &'static str,
    id: u64,
}

impl fmt::Display for Mention {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.prefix)?;
        fmt::Display::fmt(&self.id, f)?;
        fmt::Write::write_char(f, '>')
    }
}

impl UserId {
    /// Return a `Mention` which will ping this user.
    #[inline(always)]
    pub fn mention(&self) -> Mention {
        Mention {
            prefix: "<@",
            id: self.0,
        }
    }
}

impl RoleId {
    /// Return a `Mention` which will ping members of this role.
    #[inline(always)]
    pub fn mention(&self) -> Mention {
        Mention {
            prefix: "<@&",
            id: self.0,
        }
    }
}

impl ChannelId {
    /// Return a `Mention` which will link to this channel.
    #[inline(always)]
    pub fn mention(&self) -> Mention {
        Mention {
            prefix: "<#",
            id: self.0,
        }
    }
}

#[test]
fn mention_test() {
    assert_eq!(UserId(1234).mention().to_string(), "<@1234>");
    assert_eq!(RoleId(1234).mention().to_string(), "<@&1234>");
    assert_eq!(ChannelId(1234).mention().to_string(), "<#1234>");
}

// Emoji

/// A custom emoji uploaded to a discord server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Emoji {
    pub id: EmojiId,
    pub name: String,
    pub managed: bool,
    pub require_colons: bool,
    pub animated: bool,
    pub roles: Vec<RoleId>,
}

impl Emoji {
    /// The CDN URL that points to the image or GIF that is shown for the emoji.
    pub fn image_url(&self) -> String {
        format!(
            cdn_concat!("/emojis/{}.{}"),
            self.id,
            if !self.animated { "png" } else { "gif" }
        )
    }
}

// Stickers

/// A sticker that can be sent in messages.
///
/// https://discord.com/developers/docs/resources/sticker#sticker-resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sticker {
    /// ID of the sticker
    pub id: StickerId,

    /// Name of the sticker
    pub name: String,
    /// Description of the sticker
    pub description: Option<String>,

    /// Autocomplete/suggestion tags for the sticker (max 200 characters).
    pub tags: String,

    /// How the sticker image is stored.
    pub format: StickerFormat,

    /// Where the sticker is from.
    #[serde(flatten)]
    pub kind: StickerType,
}

/// Where the sticker is defined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StickerType {
    Standard {
        /// For standard stickers, ID of the pack the sticker is from.
        pack_id: StickerPackId,

        /// The standard sticker's sort order within its pack
        #[serde(rename = "sort_value")]
        sort: i32,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<1>,
    },
    Server {
        /// The server this sticker is from.
        #[serde(rename = "guild_id")]
        server_id: ServerId,

        /// Whether this server sticker can be used, may be false due to loss of Server Boosts
        available: bool,

        /// The user that uploaded the sticker.
        #[serde(rename = "user")]
        uploader: User,

        #[doc(hidden)]
        #[serde(rename = "type")]
        kind: Eq<2>,
    },
}

/// How the sticker image is stored in discord.
///
/// https://discord.com/developers/docs/resources/sticker#sticker-object-sticker-format-types
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum StickerFormat {
    Png = 1,
    APng,
    Lottie,
    Gif,
}

/// The smallest amount of data required to render a sticker. A partial sticker object.
///
/// https://discord.com/developers/docs/resources/sticker#sticker-item-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerItem {
    /// ID of the sticker
    pub id: StickerId,
    /// Name of the sticker
    pub name: String,

    /// The format of the sticker image.
    #[serde(rename = "format_type")]
    pub format: StickerFormat,
}

// Application

/// Information about the current application and the owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationInfo {
    pub id: ApplicationId,
    pub description: String,
    pub icon: Option<String>,
    pub name: String,
    pub rpc_origins: Vec<String>,
    pub bot_public: bool,
    pub bot_require_code_grant: bool,

    pub owner: User,
}

// User account settings

/// User settings usually used to influence client behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub detect_platform_accounts: bool,
    pub developer_mode: bool,
    pub enable_tts_command: bool,
    pub inline_attachment_media: bool,
    pub inline_embed_media: bool,
    pub locale: String,
    pub message_display_compact: bool,
    pub render_embeds: bool,
    pub server_positions: Vec<ServerId>,
    pub show_current_game: bool,
    pub status: String,
    pub theme: String,
    pub convert_emoticons: bool,
    pub friend_source_flags: FriendSourceFlags,
    /// Servers whose members cannot private message this user.
    pub restricted_servers: Vec<ServerId>,
}

/// User settings which influence per-server notification behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserServerSettings {
    #[serde(rename = "guild_id")]
    pub server_id: Option<ServerId>,
    pub message_notifications: NotificationLevel,
    pub mobile_push: bool,
    pub muted: bool,
    pub suppress_everyone: bool,
    pub channel_overrides: Vec<ChannelOverride>,
}

/// Notification level for a channel or server
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum NotificationLevel {
    /// All messages trigger a notification
    All = 0,
    /// Only @mentions trigger a notification
    Mentions = 1,
    /// No messages, even @mentions, trigger a notification
    Nothing = 2,
    /// Follow the parent's notification level
    Parent = 3,
}

/// A channel-specific notification settings override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverride {
    pub channel_id: ChannelId,
    pub message_notifications: NotificationLevel,
    pub muted: bool,
}

/// Progress through the Discord tutorial
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tutorial {
    pub indicators_suppressed: bool,
    pub indicators_confirmed: Vec<String>,
}

// Discord Status

/// Discord status maintenance message.
///
/// This can be either for active maintenances or scheduled maintenances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maintenance {
    pub description: String,
    pub id: String,
    pub name: String,
    pub start: String,
    pub stop: String,
}

/// An incident retrieved from the Discord status page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub impact: String,
    pub monitoring_at: Option<String>,
    pub name: String,
    pub page_id: String,
    #[serde(rename = "shortlink")]
    pub short_link: String,
    pub status: String,

    pub incident_updates: Vec<IncidentUpdate>,

    pub created_at: String,
    pub resolved_at: Option<String>,
    pub updated_at: String,
}

/// An update to an incident from the Discord status page. This will typically
/// state what new information has been discovered about an incident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentUpdate {
    pub body: String,
    pub id: String,
    pub incident_id: String,
    pub status: String,

    pub affected_components: Vec<Value>,

    pub created_at: String,
    pub display_at: String,
    pub updated_at: String,
}

// Invites

/// Information about an invite, as viewed from a recipient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    /// The unique code of the invite.
    pub code: String,

    /// The server you will be added to.
    #[serde(rename = "guild")]
    pub server: Option<InviteServer>,

    /// The channel you're being invited to
    pub channel: Option<InviteChannel>,

    /// The creator of the invite.
    pub inviter: Option<User>,

    /// If the invite points to a voice channel,
    /// this describes if the user should be directed
    /// into another user's stream or activity.
    #[serde(rename = "target_type")]
    pub invite_target: Option<InviteTargetType>,
    /// The user whose stream or activity will be joined.
    #[serde(rename = "target_user")]
    pub join_target: Option<User>,

    /// Approximate count of the members in the server.
    pub approximate_member_count: Option<u64>,
    /// Approximate count of the online members in the server.
    #[serde(rename = "approximate_presence_count")]
    pub approximate_online_count: Option<u64>,

    /// When the invite expires.
    pub expires_at: Option<DateTime<FixedOffset>>,

    /// Server scheduled event data.
    #[serde(rename = "guild_scheduled_event")]
    pub scheduled_event: Option<ScheduledEvent>,
}

/// Detailed information about an invite, available to server managers, hence the name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedInvite {
    /// The unique code of the invite.
    pub code: String,

    /// The server you will be added to.
    #[serde(rename = "guild")]
    pub server: Option<InviteServer>,

    /// The channel you're being invited to
    pub channel: Option<InviteChannel>,

    /// The creator of the invite.
    pub inviter: Option<User>,

    /// If the invite points to a voice channel,
    /// this describes if the user should be directed
    /// into another user's stream or activity.
    #[serde(rename = "target_type")]
    pub invite_target: Option<InviteTargetType>,
    /// The user whose stream or activity will be joined.
    #[serde(rename = "target_user")]
    pub join_target: Option<User>,

    /// Approximate count of the members in the server.
    pub approximate_member_count: Option<u64>,
    /// Approximate count of the online members in the server.
    #[serde(rename = "approximate_presence_count")]
    pub approximate_online_count: Option<u64>,

    /// When the invite expires.
    pub expires_at: Option<DateTime<FixedOffset>>,

    /// Server scheduled event data.
    #[serde(rename = "guild_scheduled_event")]
    pub scheduled_event: Option<ScheduledEvent>,

    // new fields
    /// How many times the invite has been used.
    pub uses: u64,
    /// How many times the invite can be used before deleted.
    pub max_uses: u64,

    /// Whether the invite only grants temporary membership.
    #[serde(rename = "temporary")]
    pub temporary_membership: bool,

    /// When the invite was created.
    pub created_at: DateTime<FixedOffset>,
    /// When the invite will expire, as an offset in seconds from the creation date.
    pub max_age: u64,
}

/// Defines what the joining user will see when
/// they join with an invite pointing to a voice channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum InviteTargetType {
    /// Invites a user to watch another user's live stream.
    Stream = 1,
    /// Invites a user to participate in a VC activity.
    EmbeddedApplication = 2,
}

/// A partial [Channel], that is received when resolving an invite code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteChannel {
    /// The ID of the channel you're being invited to.
    pub id: ChannelId,

    /// The name of the channel you're being invited to.
    pub name: Option<String>,

    /// The type of the channel, restricted to one of the server channel types.
    #[serde(rename = "type")]
    pub kind: ChannelType,
}

/// A partial [Server], that is received when resolving an invite code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteServer {
    /// The ID of the server.
    pub id: ServerId,

    /// The name of the server.
    pub name: String,
    /// The description of the server.
    pub description: Option<String>,

    /// The hash of the server's icon
    pub icon: Option<String>,
    /// The hash of the server's banner.
    pub banner: Option<String>,
    /// The hash of the server's invite splash,
    /// which is shown when opening an invite on the browser.
    pub splash: Option<String>,

    /// A list of rolled out features in the server.
    pub features: Vec<ServerFeature>,

    /// Conditions that must be met before new users can interact with the server.
    pub verification_level: VerificationLevel,

    /// The customizable invite code for the server, like `minecraft`.
    ///
    /// Vanity URL codes are not available to most servers.
    pub vanity_url_code: Option<String>,

    /// The self assigned NSFW rating of the server.
    #[serde(rename = "nsfw_level")]
    pub nsfw: NsfwLevel,

    #[serde(rename = "premium_subscription_count")]
    pub booster_count: u64,

    #[serde(flatten)]
    pub host: ScheduledEventHost,
}

impl InviteServer {
    /// Returns the formatted URL of the server's icon.
    ///
    /// Returns None if the server does not have an icon.
    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|icon| format!(cdn_concat!("/icons/{}/{}.jpg"), self.id, icon))
    }

    /// Returns the formatted URL of the server's banner.
    ///
    /// Returns None if the server does not have an banner.
    pub fn banner_url(&self) -> Option<String> {
        self.banner
            .as_ref()
            .map(|icon| format!(cdn_concat!("/splashes/{}/{}.jpg"), self.id, icon))
    }
}

// Server Events

/// A scheduled event inside a server.
///
/// https://discord.com/developers/docs/resources/guild-scheduled-event#guild-scheduled-event-object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEvent {
    /// The unique ID of the event
    pub id: EventId,

    /// The ID of the server the event is taking place in.
    #[serde(rename = "guild_id")]
    pub server_id: ServerId,

    /// The ID of the user who created the event.
    pub creator_id: Option<UserId>,

    /// The user details of the creator of the event.
    pub creator: Option<User>,

    /// The name of the event.
    pub name: String,
    /// The description of the event.
    pub description: Option<String>,
    /// The hash of the cover image.
    #[serde(rename = "image")]
    pub cover_image: Option<String>,

    /// When the event will approximately start.
    #[serde(rename = "scheduled_start_time")]
    pub start_time: DateTime<FixedOffset>,

    /// Details about where the event will take place.
    #[serde(flatten)]
    pub host: ScheduledEventHost,

    /// Controls who can subscribe to the event.
    #[serde(rename = "privacy_level")]
    pub privacy: ScheduledEventPrivacy,

    /// How many users will be notified when the event goes live.
    pub user_count: u64,

    /// What state is the event in.
    pub status: ScheduledEventStatus,
}

impl ScheduledEvent {
    /// Returns the formatted URL of the server's icon.
    ///
    /// Returns None if the server does not have an icon.
    pub fn cover_image_url(&self) -> Option<String> {
        self.cover_image
            .as_ref()
            .map(|icon| format!(cdn_concat!("guild-events/{}/{}.png"), self.id, icon))
    }
}

/// Where a scheduled event is held on a server.
///
/// https://discord.com/developers/docs/resources/guild-scheduled-event#guild-scheduled-event-object-guild-scheduled-event-entity-types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScheduledEventHost {
    /// Schedule an event on a stage channel in the same server.
    Stage {
        /// The stage channel the event will be hosted on.
        #[serde(rename = "channel_id")]
        channel: ChannelId,

        #[doc(hidden)]
        #[serde(rename = "entity_type")]
        _kind: Eq<1>,
    },

    /// Schedule an event on a voice channel in the same server.
    Voice {
        /// The voice channel the event will be hosted on.
        #[serde(rename = "channel_id")]
        channel: ChannelId,

        #[doc(hidden)]
        #[serde(rename = "entity_type")]
        _kind: Eq<2>,
    },

    /// Schedule an event to be hosted elsewhere.
    External {
        /// When the event is approximately going to end.
        #[serde(rename = "scheduled_end_time")]
        end_time: DateTime<FixedOffset>,

        /// Additional information about the event.
        entity_metadata: ScheduledEventMetadata,

        #[doc(hidden)]
        #[serde(rename = "entity_type")]
        _kind: Eq<3>,
    },
}

/// Additional information about an externally hosted event.
///
/// https://discord.com/developers/docs/resources/guild-scheduled-event#guild-scheduled-event-object-guild-scheduled-event-entity-metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEventMetadata {
    /// Where the external event will be hosted.
    pub location: String,
}

/// Who will be able to join a scheduled event.
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ScheduledEventPrivacy {
    ServerOnly = 2,
}

/// The scheduling status the event is in.
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ScheduledEventStatus {
    Scheduled = 1,
    Active,
    Completed,
    Canceled,
}

/// An image serialized as base64.
pub struct Image {
    pub data: String,
}
