use reqwest::Method;
use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{
    builders::SendMessage,
    error::{Error, Result, StatusChecks},
    model::{ChannelId, Message, MessageId, ReactionEmoji, User, UserId},
};

use super::Discord;

/// Discord REST API methods for sending, editing, pining and otherwise interact with sent messages.
pub trait MessageExt {
    /// Get a single message by ID from a given channel.
    async fn get_message(&self, channel: ChannelId, message: MessageId) -> Result<Message>;

    /// Get messages in the backlog for a given channel.
    ///
    /// The `what` argument should be one of the options in the `GetMessages`
    /// enum, and will determine which messages will be returned. A message
    /// limit can also be specified, and defaults to 50. More recent messages
    /// will appear first in the list.
    async fn get_messages(
        &self,
        channel: ChannelId,
        what: GetMessages,
        limit: Option<u64>,
    ) -> Result<Vec<Message>>;

    /// Gets a list of the pinned messages for a given channel.
    async fn get_pins(&self, channel: ChannelId) -> Result<Vec<Message>>;

    /// Pin the given message to the given channel.
    ///
    /// Requires that the logged in user has the `MANAGE_MESSAGES` permission.
    async fn pin_message(&self, channel: ChannelId, message: MessageId) -> Result<()>;

    /// Removes the given message from being pinned to the given channel.
    ///
    /// Requires that the logged in user has the `MANAGE_MESSAGES` permission.
    async fn unpin_message(&self, channel: ChannelId, message: MessageId) -> Result<()>;

    /// Build and send a message to a given channel.
    async fn send_message<F: FnOnce(SendMessage) -> SendMessage>(
        &self,
        channel: ChannelId,
        builder: F,
    ) -> Result<Message>;

    /// Edit a previously posted message by building a new one.
    ///
    /// Requires that either the message was posted by this user, or this user
    /// has permission to manage other members' messages.
    ///
    /// Not all fields can be edited; see the [docs] for more.
    /// [docs]: https://discord.com/developers/docs/resources/channel#edit-message
    async fn edit_message<F: FnOnce(SendMessage) -> SendMessage>(
        &self,
        channel: ChannelId,
        message: MessageId,
        builder: F,
    ) -> Result<Message>;

    /// Send a message to a given channel.
    ///
    /// The `nonce` will be returned in the result and also transmitted to other
    /// clients. The empty string is a good default if you don't care.
    async fn send_text_message(
        &self,
        channel: ChannelId,
        text: &str,
        nonce: &str,
    ) -> Result<Message>;

    /// Edit the text portion of a previously posted message.
    ///
    /// Requires that either the message was posted by this user, or this user
    /// has permission to manage other members' messages.
    async fn edit_text_message(
        &self,
        channel: ChannelId,
        message: MessageId,
        text: &str,
    ) -> Result<Message>;

    /// Send a message with file attachments to a given channel.
    ///
    /// The filenames will be replaced with "file" if equal to `Some("")` or `None`.
    async fn send_message_with_files<R, F>(
        &self,
        channel: ChannelId,
        message: F,
        files: Vec<(Option<&str>, &mut R)>,
    ) -> Result<Message>
    where
        R: AsyncRead + Unpin,
        F: FnOnce(SendMessage) -> SendMessage;

    /// Send a message with a file attachment to a given channel.
    ///
    /// The filename will be replaced with "file" if equal to `Some("")` or `None`.
    async fn send_message_with_file<R, F>(
        &self,
        message: ChannelId,
        builder: F,
        file: &mut R,
        file_name: Option<&str>,
    ) -> Result<Message>
    where
        R: AsyncRead + Unpin,
        F: FnOnce(SendMessage) -> SendMessage;

    /// Delete a previously posted message.
    ///
    /// Requires that either the message was posted by this user, or this user
    /// has permission to manage other members' messages.
    async fn delete_message(&self, channel: ChannelId, message: MessageId) -> Result<()>;

    /// Bulk deletes a list of messages by ID from a given channel.
    ///
    /// A minimum of 2 unique messages and a maximum of 100 unique messages may
    /// be supplied, otherwise an `Error::Other` will be returned.
    ///
    /// Each MessageId *should* be unique as duplicates will be removed from the
    /// array before being sent to the Discord API.
    ///
    /// Only bots can use this endpoint. Regular user accounts can not use this
    /// endpoint under any circumstance.
    ///
    /// Requires that either the message was posted by this user, or this user
    /// has permission to manage other members' messages.
    async fn delete_messages(&self, channel: ChannelId, messages: &[MessageId]) -> Result<()>;

    /// Add a `Reaction` to a `Message`.
    ///
    /// # Examples
    /// Add an unicode emoji to a `Message`:
    ///
    /// ```ignore
    /// // Assuming that a `Discord` instance, channel, message have
    /// // already been previously defined.
    /// use discord::model::ReactionEmoji;
    ///
    /// let _ = discord.add_reaction(&channel.id, message.id, ReactionEmoji::Unicode("👌".to_string)).await;
    /// ```
    ///
    /// Add a custom emoji to a `Message`:
    ///
    /// ```ignore
    /// // Assuming that a `Discord` instance, channel, message have
    /// // already been previously defined.
    /// use discord::model::{EmojiId, ReactionEmoji};
    ///
    /// let _ = discord.add_reaction(&channel.id, message.id, ReactionEmoji::Custom {
    ///     name: "ThisIsFine",
    ///     id: EmojiId(1234)
    /// }).await;
    /// ```
    ///
    /// Requires the `ADD_REACTIONS` permission to add a new reaction.
    async fn add_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: ReactionEmoji,
    ) -> Result<()>;

    /// Delete a `Reaction` from a `Message`.
    ///
    /// # Examples
    /// Delete a `Reaction` from a `Message` (unicode emoji):
    ///
    /// ```ignore
    /// // Assuming that a `Discord` instance, channel, message, state have
    /// // already been previously defined.
    /// use discord::model::ReactionEmoji;
    ///
    /// let _ = discord.delete_reaction(&channel.id, message.id, None, ReactionEmoji::Unicode("👌".to_string())).await;
    /// ```
    ///
    /// Delete your `Reaction` from a `Message` (custom emoji):
    ///
    /// ```ignore
    /// // Assuming that a `Discord` instance, channel, message have
    /// // already been previously defined.
    /// use discord::model::ReactionEmoji;
    ///
    /// let _ = discord.delete_reaction(&channel.id, message.id, None, ReactionEmoji::Custom {
    ///	    name: "ThisIsFine",
    ///     id: EmojiId(1234)
    /// }).await;
    /// ```
    ///
    /// Delete someone else's `Reaction` from a `Message` (custom emoji):
    ///
    /// ```ignore
    /// // Assuming that a `Discord` instance, channel, message have
    /// // already been previously defined.
    /// use discord::model::{EmojiId, ReactionEmoji};
    ///
    /// let _ = discord.delete_reaction(&channel.id, message.id, Some(UserId(1234)), ReactionEmoji::Custom {
    ///     name: "ThisIsFine",
    ///     id: EmojiId(1234)
    /// }).await;
    /// ```
    ///
    /// Requires `MANAGE_MESSAGES` if deleting someone else's `Reaction`.
    async fn delete_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        user_id: Option<UserId>,
        emoji: ReactionEmoji,
    ) -> Result<()>;

    /// Get users that have reacted with a given `Emoji` in a `Message`.
    ///
    /// The default `limit` is 50. The optional value of `after` is the ID of
    /// the user to retrieve the next reactions after.
    async fn get_reactions(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: ReactionEmoji,
        limit: Option<i32>,
        after: Option<UserId>,
    ) -> Result<Vec<User>>;
}

impl MessageExt for Discord {
    async fn get_message(&self, channel: ChannelId, message: MessageId) -> Result<Message> {
        let message = self
            .empty_request(
                &format!("/channels/{channel}/messages/{message}"),
                Method::GET,
            )
            .await?
            .json()
            .await?;

        Ok(message)
    }

    async fn get_messages(
        &self,
        channel: ChannelId,
        what: GetMessages,
        limit: Option<u64>,
    ) -> Result<Vec<Message>> {
        use std::fmt::Write;
        let mut url = format!("/channels/{channel}/messages?limit={}", limit.unwrap_or(50));
        match what {
            GetMessages::MostRecent => {}
            GetMessages::Before(id) => {
                let _ = write!(url, "&before={}", id);
            }
            GetMessages::After(id) => {
                let _ = write!(url, "&after={}", id);
            }
            GetMessages::Around(id) => {
                let _ = write!(url, "&around={}", id);
            }
        }

        Ok(self.empty_request(&url, Method::GET).await?.json().await?)
    }

    async fn get_pins(&self, channel: ChannelId) -> Result<Vec<Message>> {
        let messages = self
            .empty_request(&format!("/channels/{channel}/pins"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(messages)
    }

    async fn pin_message(&self, channel: ChannelId, message: MessageId) -> Result<()> {
        self.empty_request(&format!("/channels/{channel}/pins/{message}"), Method::PUT)
            .await?
            .insure_no_content()
            .await
    }

    async fn unpin_message(&self, channel: ChannelId, message: MessageId) -> Result<()> {
        self.empty_request(
            &format!("/channels/{channel}/pins/{message}"),
            Method::DELETE,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn send_message<F: FnOnce(SendMessage) -> SendMessage>(
        &self,
        channel: ChannelId,
        builder: F,
    ) -> Result<Message> {
        let map = SendMessage::build(builder);

        let message = self
            .request(
                &format!("/channels/{channel}/messages"),
                Method::POST,
                |req| req.json(&map),
            )
            .await?
            .json()
            .await?;

        Ok(message)
    }

    async fn edit_message<F: FnOnce(SendMessage) -> SendMessage>(
        &self,
        channel: ChannelId,
        message: MessageId,
        builder: F,
    ) -> Result<Message> {
        let map = SendMessage::build(builder);

        let message = self
            .request(
                &format!("/channels/{channel}/messages/{message}"),
                Method::POST,
                |req| req.json(&map),
            )
            .await?
            .json()
            .await?;

        Ok(message)
    }

    async fn send_text_message(
        &self,
        channel: ChannelId,
        text: &str,
        nonce: &str,
    ) -> Result<Message> {
        self.send_message(channel, |b| b.content(text).nonce(nonce))
            .await
    }

    async fn edit_text_message(
        &self,
        channel: ChannelId,
        message: MessageId,
        text: &str,
    ) -> Result<Message> {
        self.edit_message(channel, message, |b| b.content(text))
            .await
    }

    async fn send_message_with_files<R, F>(
        &self,
        channel: ChannelId,
        message: F,
        files: Vec<(Option<&str>, &mut R)>,
    ) -> Result<Message>
    where
        R: AsyncRead + Unpin,
        F: FnOnce(SendMessage) -> SendMessage,
    {
        let url = format!("/channels/{channel}/messages");

        let message_data = SendMessage::build(message);
        let json_part = reqwest::multipart::Part::bytes(serde_json::to_vec(&message_data)?)
            .mime_str("application/json")?;

        let multipart_form = reqwest::multipart::Form::new().part("payload_json", json_part);

        // todo: optimize to stream files as they are read from the disk
        let multipart_form = futures::future::join_all(
            files
                .into_iter()
                .map(|(file_name, file)| {
                    let file_name = match file_name {
                        Some(val) if !val.is_empty() => val,
                        _ => "file",
                    }
                    .to_owned();

                    (file_name, file)
                })
                .map(|(file_name, file)| async move {
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf).await?;

                    Ok::<(String, Vec<u8>), std::io::Error>((file_name, buf))
                }),
        )
        .await
        .into_iter()
        .collect::<std::result::Result<Vec<(String, Vec<u8>)>, std::io::Error>>()?
        .into_iter()
        .map(|(file_name, file)| reqwest::multipart::Part::bytes(file).file_name(file_name))
        .enumerate()
        .fold(multipart_form, |form, (index, part)| {
            form.part(format!("files[{}]", index), part)
        });

        let message = self
            .request(&url, Method::POST, |req| req.multipart(multipart_form))
            .await?
            .json()
            .await?;

        Ok(message)
    }

    async fn send_message_with_file<R, F>(
        &self,
        channel: ChannelId,
        builder: F,
        file: &mut R,
        file_name: Option<&str>,
    ) -> Result<Message>
    where
        R: AsyncRead + Unpin,
        F: FnOnce(SendMessage) -> SendMessage,
    {
        self.send_message_with_files(channel, builder, vec![(file_name, file)])
            .await
    }

    async fn delete_message(&self, channel: ChannelId, message: MessageId) -> Result<()> {
        self.empty_request(
            &format!("/channels/{channel}/messages/{message}"),
            Method::DELETE,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn delete_messages(&self, channel: ChannelId, messages: &[MessageId]) -> Result<()> {
        // Create a Vec of the underlying u64's of the message ids, then remove
        // duplicates in it.
        let mut ids: Vec<u64> = messages.into_iter().map(|m| m.0).collect();
        ids.sort();
        ids.dedup();

        if ids.len() < 2 {
            return Err(Error::Other("A minimum of 2 message ids must be supplied"));
        } else if ids.len() > 100 {
            return Err(Error::Other("A maximum of 100 message ids may be supplied"));
        }

        let map = json! {{ "messages": ids }};

        self.request(
            &format!("/channels/{channel}/messages/bulk_delete"),
            Method::POST,
            |req| req.json(&map),
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn add_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: ReactionEmoji,
    ) -> Result<()> {
        let emoji = match emoji {
            ReactionEmoji::Custom { name, id, .. } => format!("{}:{}", name, id.0),
            ReactionEmoji::Unicode { name } => name,
        };

        self.empty_request(
            &format!("/channels/{channel}/messages/{message}/reactions/{emoji}/@me"),
            Method::PUT,
        )
        .await?
        .insure_no_content()
        .await
    }

    async fn delete_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        user_id: Option<UserId>,
        emoji: ReactionEmoji,
    ) -> Result<()> {
        let emoji = match emoji {
            ReactionEmoji::Custom { name, id, .. } => format!("{}:{}", name, id.0),
            ReactionEmoji::Unicode { name } => name,
        };
        let endpoint = format!(
            "/channels/{}/messages/{}/reactions/{}/{}",
            channel,
            message,
            emoji,
            match user_id {
                Some(id) => id.0.to_string(),
                None => "@me".to_string(),
            }
        );

        self.empty_request(&endpoint, Method::DELETE)
            .await?
            .insure_no_content()
            .await
    }

    async fn get_reactions(
        &self,
        channel: ChannelId,
        message: MessageId,
        emoji: ReactionEmoji,
        limit: Option<i32>,
        after: Option<UserId>,
    ) -> Result<Vec<User>> {
        let emoji = match emoji {
            ReactionEmoji::Custom { name, id, .. } => format!("{}:{}", name, id.0),
            ReactionEmoji::Unicode { name } => name,
        };
        let mut endpoint = format!(
            "/channels/{}/messages/{}/reactions/{}?limit={}",
            channel,
            message,
            emoji,
            limit.unwrap_or(50)
        );

        if let Some(amount) = after {
            use std::fmt::Write;
            let _ = write!(endpoint, "&after={}", amount);
        }

        let users = self
            .empty_request(&endpoint, Method::GET)
            .await?
            .json()
            .await?;

        Ok(users)
    }
}

/// Argument to `get_messages` to specify the desired message retrieval.
pub enum GetMessages {
    /// Get the N most recent messages.
    MostRecent,
    /// Get the first N messages before the specified message.
    Before(MessageId),
    /// Get the first N messages after the specified message.
    After(MessageId),
    /// Get N/2 messages before, N/2 messages after, and the specified message.
    Around(MessageId),
}
