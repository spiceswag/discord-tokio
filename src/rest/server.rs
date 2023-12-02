//! Interact with Servers (`guilds`) and their channels.
//!
//! For sending messages on any channel use the `MessageExt` extension trait.
//!
//! Other text-based channels such as Direct Messages, group chats
//! and `relationships` can be found in the `AtMeExt` extension trait.

use reqwest::Method;
use serde_json::json;

use crate::{
    builders::{EditMember, EditRole, EditServer},
    error::{Result, StatusChecks},
    model::{
        Ban, ChannelId, ChannelType, Emoji, EmojiId, Image, Invite, ManagedInvite, Member,
        Permissions, Role, RoleId, Server, ServerChannel, ServerId, ServerPreview, ServerPrune,
        UserId,
    },
    resolve_invite,
};

use super::Discord;

/// Discord Rest API methods for working with servers (guilds) and their channels
///
/// This trait is not meant to be implemented by any type
/// except the [`Discord`] Rest API client provided by this crate.
pub trait ServerExt {
    /// Get the list of servers this user knows about.
    async fn get_servers(&self) -> Result<Vec<ServerPreview>>;

    /// Gets a specific server.
    async fn get_server(&self, server_id: ServerId) -> Result<Server>;

    /// Gets the list of a specific server's members.
    async fn get_server_members(
        &self,
        server_id: ServerId,
        limit: Option<u32>,
        after: Option<u32>,
    ) -> Result<Vec<Member>>;

    /// Get the list of channels in a server.
    async fn get_server_channels(&self, server: ServerId) -> Result<Vec<ServerChannel>>;

    /// Create a channel on a server.
    ///
    /// This method only accepts channel types that
    /// are supported on servers, such as `Text` and `Voice`.
    async fn create_server_channel(
        &self,
        server: ServerId,
        name: &str,
        kind: ChannelType,
    ) -> Result<ServerChannel>;

    /// Create a new server with the given name.
    ///
    /// This method may only be used by user accounts.
    async fn create_server(&self, name: &str, region: &str, icon: Option<&str>) -> Result<Server>;

    /// Edit a server's information. See `EditServer` for the editable fields.
    ///
    /// ```ignore
    /// // Rename a server
    /// discord.edit_server(server_id, |server| server.name("My Cool Server")).await;
    /// // Edit many properties at once
    /// discord.edit_server(server_id, |server| server
    ///     .name("My Cool Server")
    ///     .icon(Some("data:image/jpg;base64,..."))
    ///     .afk_timeout(300)
    ///     .region("us-south")
    /// ).await;
    /// ```
    async fn edit_server<F: FnOnce(EditServer) -> EditServer>(
        &self,
        server_id: ServerId,
        f: F,
    ) -> Result<Server>;

    /// Delete the given server. Only available to the server owner.
    async fn delete_server(&self, server: ServerId) -> Result<Server>;

    /// Leave the given server.
    async fn leave_server(&self, server: ServerId) -> Result<Server>;

    /// Get the ban list for the given server.
    async fn get_bans(&self, server: ServerId) -> Result<Vec<Ban>>;

    /// Ban a user from the server, optionally deleting their recent messages.
    ///
    /// Zero may be passed for `delete_message_days` if no deletion is desired.
    async fn add_ban(&self, server: ServerId, user: UserId, delete_message_days: u32)
        -> Result<()>;

    /// Unban a user from the server.
    async fn remove_ban(&self, server: ServerId, user: UserId) -> Result<()>;

    /// Kick a member from a server.
    async fn kick_member(&self, server: ServerId, user: UserId) -> Result<()>;

    /// Extract information from an invite.
    ///
    /// The invite should either be a URL of the form `http://discord.gg/CODE`,
    /// or a string containing just the `CODE`.
    async fn get_invite(&self, invite: &str) -> Result<Invite>;

    /// Get the active invites for a server.
    async fn get_server_invites(&self, server: ServerId) -> Result<Vec<ManagedInvite>>;

    /// Get the active invites for a channel.
    async fn get_channel_invites(&self, channel: ChannelId) -> Result<Vec<ManagedInvite>>;

    /// Accept an invite. See `get_invite` for details.
    async fn accept_invite(&self, invite: &str) -> Result<Invite>;

    /// Create an invite to a channel.
    ///
    /// Passing 0 for `max_age` or `max_uses` means no limit. `max_age` should
    /// be specified in seconds.
    async fn create_invite(
        &self,
        channel: ChannelId,
        max_age: u64,
        max_uses: u64,
        temporary: bool,
    ) -> Result<ManagedInvite>;

    /// Delete an invite. See `get_invite` for details.
    async fn delete_invite(&self, invite: &str) -> Result<Invite>;

    /// Creates a custom emoji in a server.
    ///
    /// Requires that the logged in account be a user
    /// and have the `ADMINISTRATOR` or `MANAGE_EMOJIS` permission.
    ///
    /// `read_image` may be used to build an `image` string.
    async fn create_emoji(&self, server: ServerId, name: &str, image: Image) -> Result<Emoji>;

    /// Edits a server's emoji.
    ///
    /// Requires that the logged in account be a user and have the
    /// `ADMINISTRATOR` or `MANAGE_EMOJIS` permission.
    async fn edit_emoji(&self, server: ServerId, emoji: EmojiId, name: &str) -> Result<Emoji>;

    /// Delete an emoji in a server.
    ///
    /// Requires that the logged in account be a user and have the
    /// `ADMINISTRATOR` or `MANAGE_EMOJIS` permission.
    async fn delete_emoji(&self, server: ServerId, emoji: EmojiId) -> Result<()>;

    /// Retrieve a member object for a server given the member's user id.
    async fn get_member(&self, server: ServerId, user: UserId) -> Result<Member>;

    /// Edit the list of roles assigned to a member of a server.
    async fn edit_member_roles(
        &self,
        server: ServerId,
        user: UserId,
        roles: &[RoleId],
    ) -> Result<()>;

    /// Add a role to a member of a server.
    async fn add_member_role(&self, server: ServerId, user: UserId, role: RoleId) -> Result<()>;

    /// Remove a role for a member of a server.
    async fn remove_member_role(&self, server: ServerId, user: UserId, role: RoleId) -> Result<()>;

    /// Edit member information, including roles, nickname, and voice state.
    ///
    /// See the `EditMember` struct for the editable fields.
    async fn edit_member<F: FnOnce(EditMember) -> EditMember>(
        &self,
        server: ServerId,
        user: UserId,
        f: F,
    ) -> Result<()>;

    /// Change the server nickname of another user.
    ///
    /// Shorthand for an `edit_member` invocation.
    async fn edit_nickname(&self, server: ServerId, member: UserId, nick: &str) -> Result<()>;

    /// Change the nickname of the current user in a server.
    async fn edit_own_nickname(&self, server: ServerId, nick: &str) -> Result<()>;

    /// Retrieve the list of roles for a server.
    async fn get_roles(&self, server: ServerId) -> Result<Vec<Role>>;

    /// Create a new role on a server.
    async fn create_role(
        &self,
        server: ServerId,
        name: Option<&str>,
        permissions: Option<Permissions>,
        color: Option<u64>,
        hoist: Option<bool>,
        mentionable: Option<bool>,
    ) -> Result<Role>;

    /// Create a new role on a server.
    async fn create_role_from_builder<F: FnOnce(EditRole) -> EditRole>(
        &self,
        server: ServerId,
        f: F,
    ) -> Result<Role>;

    /// Modify a role on a server.
    async fn edit_role<F: FnOnce(EditRole) -> EditRole>(
        &self,
        server: ServerId,
        role: RoleId,
        f: F,
    ) -> Result<Role>;

    /// Reorder the roles on a server.
    async fn reorder_roles(&self, server: ServerId, roles: &[(RoleId, usize)])
        -> Result<Vec<Role>>;

    /// Remove specified role from a server.
    async fn delete_role(&self, server: ServerId, role: RoleId) -> Result<()>;

    /// Start a prune operation, kicking members who have been inactive for the
    /// specified number of days. Members with a role assigned will never be
    /// pruned.
    async fn begin_server_prune(&self, server: ServerId, days: u16) -> Result<ServerPrune>;

    /// Get the number of members who have been inactive for the specified
    /// number of days and would be pruned by a prune operation. Members with a
    /// role assigned will never be pruned.
    async fn get_server_prune_count(&self, server: ServerId, days: u16) -> Result<ServerPrune>;
}

impl ServerExt for Discord {
    async fn get_servers(&self) -> Result<Vec<ServerPreview>> {
        let servers = self
            .empty_request("/users/@me/guilds", Method::GET)
            .await?
            .json()
            .await?;

        Ok(servers)
    }

    async fn get_server(&self, server_id: ServerId) -> Result<Server> {
        let server = self
            .empty_request(&format!("/guilds/{server_id}"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(server)
    }

    async fn get_server_members(
        &self,
        server_id: ServerId,
        limit: Option<u32>,
        after: Option<u32>,
    ) -> Result<Vec<Member>> {
        let limit = limit.unwrap_or(1);
        let after = after.unwrap_or(0);

        let members = self
            .empty_request(
                &format!(
                    "/guilds/{server_id}/members?limit={}&after={}",
                    limit, after
                ),
                Method::GET,
            )
            .await?
            .json()
            .await?;

        Ok(members)
    }

    async fn get_server_channels(&self, server: ServerId) -> Result<Vec<ServerChannel>> {
        let channels = self
            .empty_request(&format!("/guilds/{server}/channels"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(channels)
    }

    async fn create_server_channel(
        &self,
        server: ServerId,
        name: &str,
        kind: ChannelType,
    ) -> Result<ServerChannel> {
        let map = json! {{
            "name": name,
            "type": kind as u8,
        }};

        let channel = self
            .request(&format!("/guilds/{server}/channels"), Method::POST, |req| {
                req.json(&map)
            })
            .await?
            .json::<ServerChannel>()
            .await?;

        Ok(channel)
    }

    async fn create_server(&self, name: &str, region: &str, icon: Option<&str>) -> Result<Server> {
        let map = json! {{
            "name": name,
            "region": region,
            "icon": icon,
        }};

        let server = self
            .request("/guilds", Method::POST, |req| req.json(&map))
            .await?
            .json()
            .await?;

        Ok(server)
    }

    async fn edit_server<F: FnOnce(EditServer) -> EditServer>(
        &self,
        server_id: ServerId,
        f: F,
    ) -> Result<Server> {
        let map = EditServer::build(f);

        let server = self
            .request(&format!("/guilds/{server_id}"), Method::PATCH, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(server)
    }

    async fn delete_server(&self, server: ServerId) -> Result<Server> {
        let server = self
            .empty_request(&format!("/guilds/{server}"), Method::DELETE)
            .await?
            .json()
            .await?;

        Ok(server)
    }

    async fn leave_server(&self, server: ServerId) -> Result<Server> {
        let server = self
            .empty_request(&format!("/users/@me/guilds/{server}"), Method::DELETE)
            .await?
            .json()
            .await?;

        Ok(server)
    }

    async fn get_bans(&self, server: ServerId) -> Result<Vec<Ban>> {
        let bans = self
            .empty_request(&format!("/guilds/{server}/bans"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(bans)
    }

    async fn add_ban(
        &self,
        server: ServerId,
        user: UserId,
        delete_message_days: u32,
    ) -> Result<()> {
        self.empty_request(
            &format!(
                "/guilds/{server}/bans/{user}?delete_message_days={}",
                delete_message_days
            ),
            Method::DELETE,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn remove_ban(&self, server: ServerId, user: UserId) -> Result<()> {
        self.empty_request(&format!("/guilds/{server}/bans/{user}"), Method::DELETE)
            .await?
            .insure_no_content()
            .await
    }

    async fn kick_member(&self, server: ServerId, user: UserId) -> Result<()> {
        self.empty_request(&format!("/guilds/{server}/members/{user}"), Method::DELETE)
            .await?
            .insure_no_content()
            .await
    }

    async fn get_invite(&self, invite: &str) -> Result<Invite> {
        let invite = resolve_invite(invite);

        let invite = self
            .empty_request(&format!("/invite/{invite}"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(invite)
    }

    async fn get_server_invites(&self, server: ServerId) -> Result<Vec<ManagedInvite>> {
        let invites = self
            .empty_request(&format!("/guilds/{server}/invites"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(invites)
    }

    async fn get_channel_invites(&self, channel: ChannelId) -> Result<Vec<ManagedInvite>> {
        let invites = self
            .empty_request(&format!("/channels/{channel}/invites"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(invites)
    }

    async fn accept_invite(&self, invite: &str) -> Result<Invite> {
        let invite = resolve_invite(invite);
        let invite = self
            .empty_request(&format!("/invite/{invite}"), Method::POST)
            .await?
            .json()
            .await?;

        Ok(invite)
    }

    async fn create_invite(
        &self,
        channel: ChannelId,
        max_age: u64,
        max_uses: u64,
        temporary: bool,
    ) -> Result<ManagedInvite> {
        let map = json! {{
            "validate": null,
            "max_age": max_age,
            "max_uses": max_uses,
            "temporary": temporary,
        }};

        let invite = self
            .request(
                &format!("/channels/{channel}/invites"),
                Method::POST,
                |req| req.json(&map),
            )
            .await?
            .json()
            .await?;

        Ok(invite)
    }

    async fn delete_invite(&self, invite: &str) -> Result<Invite> {
        let invite = resolve_invite(invite);
        let invite = self
            .empty_request(&format!("/invite/{invite}"), Method::DELETE)
            .await?
            .json()
            .await?;

        Ok(invite)
    }

    async fn create_emoji(&self, server: ServerId, name: &str, image: Image) -> Result<Emoji> {
        let map = json! {{
            "name": name,
            "image": image.data,
        }};

        let emoji = self
            .request(&format!("/guilds/{server}/emojis"), Method::POST, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(emoji)
    }

    async fn edit_emoji(&self, server: ServerId, emoji: EmojiId, name: &str) -> Result<Emoji> {
        let map = json! {{
            "name": name
        }};

        let emoji = self
            .request(
                &format!("/guilds/{server}/emojis/{emoji}"),
                Method::PATCH,
                |req| req.json(&map),
            )
            .await?
            .json()
            .await?;

        Ok(emoji)
    }

    async fn delete_emoji(&self, server: ServerId, emoji: EmojiId) -> Result<()> {
        self.empty_request(&format!("/guilds/{server}/emojis/{emoji}"), Method::DELETE)
            .await?
            .insure_no_content()
            .await
    }

    async fn get_member(&self, server: ServerId, user: UserId) -> Result<Member> {
        let member = self
            .empty_request(&format!("/guilds/{server}/members/{user}"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(member)
    }

    async fn edit_member_roles(
        &self,
        server: ServerId,
        user: UserId,
        roles: &[RoleId],
    ) -> Result<()> {
        self.edit_member(server, user, |m| m.roles(roles)).await
    }

    async fn add_member_role(&self, server: ServerId, user: UserId, role: RoleId) -> Result<()> {
        self.empty_request(
            &format!("/guilds/{server}/members/{user}/roles/{role}"),
            Method::PUT,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn remove_member_role(&self, server: ServerId, user: UserId, role: RoleId) -> Result<()> {
        self.empty_request(
            &format!("/guilds/{server}/members/{user}/roles/{role}"),
            Method::DELETE,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn edit_member<F: FnOnce(EditMember) -> EditMember>(
        &self,
        server: ServerId,
        user: UserId,
        f: F,
    ) -> Result<()> {
        let map = EditMember::build(f);

        self.request(
            &format!("/guilds/{server}/members/{user}"),
            Method::PATCH,
            |req| req.json(&map),
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn edit_nickname(&self, server: ServerId, member: UserId, nick: &str) -> Result<()> {
        self.edit_member(server, member, |member| member.nickname(nick))
            .await
    }

    async fn edit_own_nickname(&self, server: ServerId, nick: &str) -> Result<()> {
        let map = json! {{ "nick": nick }};

        self.request(
            &format!("/guilds/{server}/members/@me/nick"),
            Method::PATCH,
            |req| req.json(&map),
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn get_roles(&self, server: ServerId) -> Result<Vec<Role>> {
        let roles = self
            .empty_request(&format!("/guilds/{server}/roles"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(roles)
    }

    async fn create_role(
        &self,
        server: ServerId,
        name: Option<&str>,
        permissions: Option<Permissions>,
        color: Option<u64>,
        hoist: Option<bool>,
        mentionable: Option<bool>,
    ) -> Result<Role> {
        let map = json! {{
            "name": name,
            "permissions": permissions,
            "color": color,
            "hoist": hoist,
            "mentionable": mentionable,
        }};

        let role = self
            .request(&format!("/guilds/{server}/roles"), Method::POST, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(role)
    }

    async fn create_role_from_builder<F: FnOnce(EditRole) -> EditRole>(
        &self,
        server: ServerId,
        f: F,
    ) -> Result<Role> {
        let map = EditRole::build(f);

        let role = self
            .request(&format!("/guilds/{server}/roles"), Method::POST, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(role)
    }

    async fn edit_role<F: FnOnce(EditRole) -> EditRole>(
        &self,
        server: ServerId,
        role: RoleId,
        f: F,
    ) -> Result<Role> {
        let map = EditRole::build(f);

        let role = self
            .request(
                &format!("/guilds/{server}/roles/{role}"),
                Method::PATCH,
                |req| req.json(&map),
            )
            .await?
            .json()
            .await?;

        Ok(role)
    }

    async fn reorder_roles(
        &self,
        server: ServerId,
        roles: &[(RoleId, usize)],
    ) -> Result<Vec<Role>> {
        let map: serde_json::Value = roles
            .iter()
            .map(|&(id, pos)| {
                json! {{
                    "id": id,
                    "position": pos
                }}
            })
            .collect();

        let roles = self
            .request(&format!("/guilds/{server}/roles"), Method::PATCH, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(roles)
    }

    async fn delete_role(&self, server: ServerId, role: RoleId) -> Result<()> {
        self.empty_request(&format!("/guilds/{server}/roles/{role}"), Method::DELETE)
            .await?
            .insure_no_content()
            .await
    }

    async fn begin_server_prune(&self, server: ServerId, days: u16) -> Result<ServerPrune> {
        let map = json! {{ "days": days }};

        let prune = self
            .request(&format!("/guilds/{server}/prune"), Method::POST, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(prune)
    }

    async fn get_server_prune_count(&self, server: ServerId, days: u16) -> Result<ServerPrune> {
        let map = json! {{ "days": days }};

        let prune = self
            .request(&format!("/guilds/{server}/prune"), Method::GET, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(prune)
    }
}
