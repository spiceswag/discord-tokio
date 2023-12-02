use reqwest::Method;
use serde_json::json;

use crate::{
    builders::{EditProfile, EditUserProfile},
    error::{Error, Result, StatusChecks},
    model::{CurrentUser, DirectMessage, User, UserId},
    Object,
};

use super::Discord;

/// Discord REST API methods for working with the options
/// of the current user, their DMs and relationships.
pub trait UserExt {
    /// Get information about a user.
    /// https://discord.com/developers/docs/resources/user#get-user
    async fn get_user(&self, user: UserId) -> Result<User>;

    /// Get the logged-in user's profile.
    async fn get_current_user(&self) -> Result<CurrentUser>;

    /// Edit the logged-in bot or user's profile. See `EditProfile` for editable fields.
    ///
    /// Usable for bot and user accounts. Only allows updating the username and
    /// avatar.
    async fn edit_profile<F: FnOnce(EditProfile) -> EditProfile>(
        &self,
        f: F,
    ) -> Result<CurrentUser>;

    /// Edit the logged-in non-bot user's profile. See `EditUserProfile` for editable fields.
    ///
    /// Usable only for user (non-bot) accounts. Requires mutable access in order
    /// to keep the login token up to date in the event of a password change.
    async fn edit_user_profile<F: FnOnce(EditUserProfile) -> EditUserProfile>(
        &mut self,
        f: F,
    ) -> Result<CurrentUser>;

    /// Create a DM channel with the given user,
    /// or return the existing one if it exists.
    async fn create_dm(&self, recipient: UserId) -> Result<DirectMessage>;

    /// Sets a note for the user that is readable only to the currently logged
    /// in user.
    ///
    /// This endpoint is only available for users, and does not work for bots.
    async fn edit_note(&self, user: UserId, note: &str) -> Result<()>;
}

impl UserExt for Discord {
    async fn get_user(&self, user: UserId) -> Result<User> {
        let user = self
            .empty_request(&format!("/users/{user}"), Method::GET)
            .await?
            .json()
            .await?;

        Ok(user)
    }

    async fn get_current_user(&self) -> Result<CurrentUser> {
        let user = self
            .empty_request("/users/@me", Method::GET)
            .await?
            .json()
            .await?;

        Ok(user)
    }

    async fn edit_profile<F: FnOnce(EditProfile) -> EditProfile>(
        &self,
        f: F,
    ) -> Result<CurrentUser> {
        // First, get the current profile, so that providing username and avatar is optional.
        let user: CurrentUser = self
            .empty_request("/users/@me", Method::GET)
            .await?
            .json()
            .await?;

        let mut map = Object::new();
        map.insert("username".into(), json!(user.username));
        map.insert("avatar".into(), json!(user.avatar));

        // Then, send the profile patch.
        let map = EditProfile::apply(f, map);

        let user = self
            .request("/user/@me", Method::PATCH, |req| req.json(&map))
            .await?
            .json()
            .await?;

        Ok(user)
    }

    async fn edit_user_profile<F: FnOnce(EditUserProfile) -> EditUserProfile>(
        &mut self,
        f: F,
    ) -> Result<CurrentUser> {
        // First, get the current profile, so that providing username and avatar is optional.
        let user: CurrentUser = self
            .empty_request("/users/@me", Method::GET)
            .await?
            .json()
            .await?;
        if user.bot {
            return Err(Error::Other(
                "Cannot call edit_user_profile on a bot account",
            ));
        }
        let mut map = Object::new();
        map.insert("username".into(), json!(user.username));
        map.insert("avatar".into(), json!(user.avatar));
        if let Some(email) = user.email.as_ref() {
            map.insert("email".into(), email.as_str().into());
        }

        // Then, send the profile patch.
        let map = EditUserProfile::apply(f, map);

        let mut json: Object = self
            .request("/user/@me", Method::PATCH, |req| req.json(&map))
            .await?
            .json()
            .await?;

        // If a token was included in the response, switch to it. Important because if the
        // password was changed, the old token is invalidated.
        if let Some(serde_json::Value::String(token)) = json.remove("token") {
            self.token = token;
        }

        Ok(serde_json::from_value(serde_json::to_value(json)?)?)
    }

    async fn create_dm(&self, recipient: UserId) -> Result<DirectMessage> {
        let map = json! {{ "recipient_id": recipient }};

        let channel = self
            .request("/user/@me/channels", Method::POST, |req| req.json(&map))
            .await?
            .json()
            .await?;

        Ok(channel)
    }

    async fn edit_note(&self, user: UserId, note: &str) -> Result<()> {
        let map = json! {{ "note": note }};

        self.request(&format!("/user/@me/notes/{user}"), Method::PUT, |req| {
            req.json(&map)
        })
        .await?
        .insure_no_content()
        .await
    }
}
