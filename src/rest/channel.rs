use reqwest::Method;
use serde_json::json;

use crate::{
    builders::EditChannel,
    error::{Error, Result, StatusChecks},
    model::{
        Channel, ChannelId, MessageId, PermissionOverwrite, PermissionOverwriteId, ServerId,
        UserId, VoiceRegion,
    },
};

use super::Discord;

/// Discord REST API methods for modifying channels and threads.
pub trait ChannelExt {
    /// Get information about a channel.
    async fn get_channel(&self, channel: ChannelId) -> Result<Channel>;

    /// Edit a channel's details. See `EditChannel` for the editable fields.
    ///
    /// ```ignore
    /// // Edit a channel's name and topic
    /// discord.edit_channel(channel_id, "general", |ch|
    ///     ch.topic("Welcome to the general chat!")
    /// ).await;
    /// ```
    async fn edit_channel<F: FnOnce(EditChannel) -> EditChannel>(
        &self,
        channel_id: ChannelId,
        f: F,
    ) -> Result<Channel>;

    /// Delete a channel, or close a private message.
    ///
    /// Requires the `MANAGE_CHANNELS` permission for the server,
    /// or `MANAGE_THREADS` if the channel is a thread.
    ///
    /// Deleting a server channel cannot be undone.
    /// Use this with caution, as it is impossible to undo this action when performed on a server channel.
    /// In contrast, when used with a private message, it is possible to undo the action by opening a private message with the recipient again.
    async fn delete_channel(&self, channel: ChannelId) -> Result<Channel>;

    /// Create permissions for a `Channel` for a `Member` or `Role`.
    ///
    /// # Examples
    ///
    /// An example of creating channel role permissions for a `Member`:
    ///
    /// ```ignore
    /// use discord::model::{PermissionOverwriteType, permissions};
    ///
    /// // Assuming that a `Discord` instance, member, and channel have already
    /// // been defined previously.
    /// let target = PermissionOverwrite {
    ///     kind: PermissionOverwriteType::Member(member.user.id),
    ///     allow: permissions::VOICE_CONNECT | permissions::VOICE_SPEAK,
    ///     deny: permissions::VOICE_MUTE_MEMBERS | permissions::VOICE_MOVE_MEMBERS,
    /// };
    /// let result = discord.create_permission(channel.id, target).await;
    /// ```
    ///
    /// The same can similarly be accomplished for a `Role`:
    ///
    /// ```ignore
    /// use discord::model::{PermissionOverwriteType, permissions};
    ///
    /// // Assuming that a `Discord` instance, role, and channel have already
    /// // been defined previously.
    /// let target = PermissionOverwrite {
    ///	    kind: PermissionOverwriteType::Role(role.id),
    ///	    allow: permissions::VOICE_CONNECT | permissions::VOICE_SPEAK,
    ///	    deny: permissions::VOICE_MUTE_MEMBERS | permissions::VOICE_MOVE_MEMBERS,
    ///	};
    /// let result = discord.create_permission(channel.id, target).await;
    /// ```
    async fn create_permission(
        &self,
        channel: ChannelId,
        permission: PermissionOverwrite,
    ) -> Result<()>;

    /// Delete a `Member` or `Role`'s permissions for a `Channel`.
    ///
    /// # Examples
    ///
    /// Delete a `Member`'s permissions for a `Channel`:
    ///
    /// ```ignore
    /// use discord::model::PermissionOverwriteType;
    ///
    /// // Assuming that a `Discord` instance, channel, and member have already
    /// // been previously defined.
    /// let target = member.user.id.0;
    /// let response = discord.delete_permission(channel.id, target).await;
    /// ```
    ///
    /// The same can be accomplished for a `Role` similarly:
    ///
    /// ```ignore
    /// use discord::model::PermissionOverwriteType;
    ///
    /// // Assuming that a `Discord` instance, channel, and role have already
    /// // been previously defined.
    /// let target = role.id.0;
    /// let response = discord.delete_permission(channel.id, target).await;
    /// ```
    async fn delete_permission(
        &self,
        channel: ChannelId,
        overwrite: PermissionOverwriteId,
    ) -> Result<()>;

    /// Indicate typing on a channel for the next 5 seconds.
    async fn broadcast_typing(&self, channel: ChannelId) -> Result<()>;

    /// Acknowledge this message as "read" by this client.
    async fn ack_message(&self, channel: ChannelId, message: MessageId) -> Result<()>;

    /// Get the list of available voice regions for the current client.
    async fn get_voice_regions(&self) -> Result<Vec<VoiceRegion>>;

    /// Move a server member to another voice channel.
    async fn move_member_voice(
        &self,
        server: ServerId,
        user: UserId,
        channel: ChannelId,
    ) -> Result<()>;
}

impl ChannelExt for Discord {
    async fn get_channel(&self, channel: ChannelId) -> Result<Channel> {
        let channel = self
            .empty_request(&format!("/channels/{channel}"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(channel)
    }

    async fn edit_channel<F: FnOnce(EditChannel) -> EditChannel>(
        &self,
        channel_id: ChannelId,
        f: F,
    ) -> Result<Channel> {
        let channel = match self.get_channel(channel_id).await? {
            Channel::DirectMessage(_) => return Err(Error::Other("Can not edit private channels")),
            channel @ _ => channel,
        };

        let map = serde_json::from_value(serde_json::to_value(channel)?)?;
        let map = EditChannel::apply(f, map);

        let channel = self
            .request(&format!("/channels/{channel_id}"), Method::PATCH, |req| {
                req.json(&map)
            })
            .await?
            .json()
            .await?;

        Ok(channel)
    }

    async fn delete_channel(&self, channel: ChannelId) -> Result<Channel> {
        let channel = self
            .empty_request(&format!("/channels/{channel}"), Method::DELETE)
            .await?
            .json()
            .await?;

        Ok(channel)
    }

    async fn create_permission(
        &self,
        channel: ChannelId,
        permission: PermissionOverwrite,
    ) -> Result<()> {
        let id = match permission {
            PermissionOverwrite::Member { id, .. } => id.0,
            PermissionOverwrite::Role { id, .. } => id.0,
        };

        self.request(
            &format!("/channels/{channel}/permissions/{id}"),
            Method::PUT,
            |req| req.json(&permission),
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn delete_permission(
        &self,
        channel: ChannelId,
        overwrite: PermissionOverwriteId,
    ) -> Result<()> {
        let id = match overwrite {
            PermissionOverwriteId::Member(id) => id.0,
            PermissionOverwriteId::Role(id) => id.0,
        };

        self.empty_request(
            &format!("/channels/{channel}/permissions/{id}"),
            Method::DELETE,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn broadcast_typing(&self, channel: ChannelId) -> Result<()> {
        self.empty_request(&format!("/channels/{channel}/typing"), Method::POST)
            .await?
            .insure_no_content()
            .await
    }

    async fn ack_message(&self, channel: ChannelId, message: MessageId) -> Result<()> {
        self.empty_request(
            &format!("/channels/{channel}/messages/{message}/ack"),
            Method::POST,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn get_voice_regions(&self) -> Result<Vec<VoiceRegion>> {
        let reasons = self
            .empty_request("/voice/regions", Method::GET)
            .await?
            .json()
            .await?;

        Ok(reasons)
    }

    async fn move_member_voice(
        &self,
        server: ServerId,
        user: UserId,
        channel: ChannelId,
    ) -> Result<()> {
        let map = json! {{ "channel_id": channel }};

        self.request(
            &format!("/guilds/{server}/members/{user}"),
            Method::PATCH,
            |req| req.json(&map),
        )
        .await?
        .insure_no_content()
        .await
    }
}
