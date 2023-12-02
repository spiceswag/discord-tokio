//! Client library for the [Discord](https://discord.com) API.
//!
//! The Discord API can be divided into three main components: the RESTful API
//! to which calls can be made to take actions, a websocket-based permanent
//! connection over which state updates are received, and the voice calling
//! system.
//!
//! Log in to Discord with `Discord::new`, `new_cache`, or `from_bot_token` as appropriate.
//! The resulting value can be used to make REST API calls to post messages and manipulate Discord
//! state. Calling `connect()` will open a websocket connection, through which events can be
//! received. These two channels are enough to write a simple chatbot which can
//! read and respond to messages.
//!
//! For more in-depth tracking of Discord state, a `State` can be seeded with
//! the `ReadyEvent` obtained when opening a `Connection` and kept updated with
//! the events received over it.
//!
#![cfg_attr(
    not(feature = "voice"),
    doc = "*<b>NOTE</b>: The library has been compiled without voice support.*"
)]
//! To join voice servers, call `Connection::voice` to get a `VoiceConnection` and use `connect`
//! to join a channel, then `play` and `stop` to control playback. Manipulating deaf/mute state
//! and receiving audio are also possible.
//!
//! For examples, see the `examples` directory in the source tree.

#![warn(missing_docs)]
#![allow(deprecated)]

use std::collections::BTreeMap;

type Object = serde_json::Map<String, serde_json::Value>;

mod connection;
mod error;
mod io;
mod ratelimit;
mod state;

mod rest;
pub use rest::*;

macro_rules! cdn_concat {
    ($e:expr) => {
        // Out of everything, only the CDN still uses the old domain.
        concat!("https://cdn.discordapp.com", $e)
    };
}

/// Struct and enum definitions of values in the Discord model.
pub mod model {
    mod event;
    pub use self::event::*;

    mod frozen;
    pub use frozen::*;

    mod live;
    pub use live::*;
}

// #[cfg(feature = "voice")]
// pub mod voice;

#[macro_use]
mod serial;
pub mod builders;

pub use error::{Error, Result};
pub use state::{ChannelRef, State};
use tracing::debug;

use crate::error::CheckStatus;
use crate::model::*;

use ratelimit::RateLimits;
use reqwest::{header, Method};

const API_BASE: &'static str = "https://discord.com/api/v6";

const STATUS_BASE: &'static str = "https://status.discord.com/api/v2";
macro_rules! status_concat {
    ($e:expr) => {
        concat!("https://status.discord.com/api/v2", $e)
    };
}

/// Client for the Discord REST API.
///
/// Log in to the API with a user's email and password using `new()`. Call
/// `connect()` to create a `Connection` on which to receive events. If desired,
/// use `logout()` to invalidate the token when done. Other methods manipulate
/// the Discord REST API.
pub struct Discord {
    rate_limits: RateLimits,
    client: reqwest::Client,
    token: String,
}

fn tls_client() -> reqwest::Client {
    reqwest::Client::builder()
        .https_only(true)
        .build()
        .expect("Couldn't build HTTPS reqwest client")
}

impl Discord {
    /// Make a request while having rate limits, retries, and authorization taken care of.
    async fn request<F: FnMut(reqwest::RequestBuilder) -> reqwest::RequestBuilder>(
        &self,
        url: &str,
        method: Method,
        mut f: F,
    ) -> Result<reqwest::Response> {
        self.rate_limits.pre_check(url);

        let builder = self.client.request(
            method,
            &format!(
                "{API_BASE}{}{}",
                if url.starts_with('/') { "" } else { "/" },
                url
            ),
        );
        let mut f2 =
            || f(builder.try_clone().unwrap()).header(header::AUTHORIZATION, self.token.clone());

        let result = retry(&mut f2).await;
        if let Ok(response) = result.as_ref() {
            if self.rate_limits.post_update(url, response).await {
                // we were rate limited, we have slept, it is time to retry
                // the request once. if it fails the second time, give up
                debug!("Retrying after having been ratelimited");
                let result = retry(&mut f2).await;
                if let Ok(response) = result.as_ref() {
                    self.rate_limits.post_update(url, response);
                }
                return result.map_err(Error::Reqwest);
            }
        }
        result.check_status().await
    }

    /// Make a request while having rate limits, retries, and authorization taken care of.
    ///
    /// Now comes in body free flavor.
    async fn empty_request(&self, url: &str, method: Method) -> Result<reqwest::Response> {
        self.request(url, method, |req| req).await
    }

    /// Retrieves information about the application and the owner.
    pub async fn get_application_info(&self) -> Result<ApplicationInfo> {
        Ok(self
            .empty_request("/oath/applications/@me", Method::GET)
            .await?
            .json()
            .await?)
    }

    /// Retrieves the number of guild shards Discord suggests to use based on
    /// the number of guilds.
    ///
    /// This endpoint is only available for bots.
    pub async fn suggested_shard_count(&self) -> Result<u64> {
        let mut response = self
            .empty_request("/gateway/bot", Method::GET)
            .await?
            .json::<Object>()
            .await?;

        match response.remove("shards") {
            Some(value) => match value.as_u64() {
                Some(shards) => Ok(shards),
                None => Err(Error::Decode("Invalid \"shards\"", value)),
            },
            None => Err(Error::Decode(
                "suggested_shard_count missing \"shards\"",
                serde_json::Value::Object(response),
            )),
        }
    }

    /*
    /// Establish a websocket connection over which events can be received.
    ///
    /// Also returns the `ReadyEvent` sent by Discord upon establishing the
    /// connection, which contains the initial state as seen by the client.
    ///
    /// See `connect_sharded` if you want to use guild sharding.
    pub async fn connect(&self) -> Result<(Connection, ReadyEvent)> {
        self.connection_builder().await?.connect().await
    }

    /// Establish a sharded websocket connection over which events can be
    /// received.
    ///
    /// The `shard_id` is indexed at 0 while `total_shards` is indexed at 1.
    ///
    /// Also returns the `ReadyEvent` sent by Discord upon establishing the
    /// connection, which contains the initial state as seen by the client.
    ///
    /// See `connect` if you do not want to use guild sharding.
    pub async fn connect_sharded(
        &self,
        shard_id: u8,
        total_shards: u8,
    ) -> Result<(Connection, ReadyEvent)> {
        self.connection_builder()
            .await?
            .sharding(shard_id, total_shards)
            .connect()
            .await
    }

    /// Prepare to establish a websocket connection over which events can be
    /// received.
    pub async fn connection_builder(&self) -> Result<connection::ConnectionBuilder> {
        let url = self.get_gateway_url().await?;
        Ok(connection::ConnectionBuilder::new(url, &self.token))
    }
    */

    async fn get_gateway_url(&self) -> Result<String> {
        let mut response: BTreeMap<String, String> = self
            .empty_request("/gateway", Method::GET)
            .await?
            .json()
            .await?;

        match response.remove("url") {
            Some(url) => Ok(url),
            None => Err(Error::Protocol(
                "Response missing \"url\" in Discord::get_gateway_url()",
            )),
        }
    }
}

/// Read an image from a file into a string suitable for upload.
///
/// If the file's extension is `.png`, the claimed media type will be `image/png`, or `image/jpg`
/// otherwise. Note that Discord may convert the image to JPEG or another format after upload.
pub fn read_image<P: AsRef<::std::path::Path>>(path: P) -> Result<String> {
    use std::io::Read;
    let path = path.as_ref();
    let mut vec = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut vec)?;
    Ok(format!(
        "data:image/{};base64,{}",
        if path.extension() == Some("png".as_ref()) {
            "png"
        } else {
            "jpg"
        },
        base64::encode(&vec),
    ))
}

/// Retrieves the current unresolved incidents from the status page.
pub async fn get_unresolved_incidents() -> Result<Vec<Incident>> {
    let client = tls_client();
    let mut response: Object =
        retry(&mut || client.get(status_concat!("/incidents/unresolved.json")))
            .await?
            .json()
            .await?;

    match response.remove("incidents") {
        Some(incidents) => Ok(serde_json::from_value(incidents)?),
        None => Ok(vec![]),
    }
}

/// Retrieves the active maintenances from the status page.
pub async fn get_active_maintenances() -> Result<Vec<Maintenance>> {
    let client = tls_client();
    let mut response: Object =
        retry(&mut || client.get(status_concat!("/scheduled-maintenances/active.json")))
            .await
            .check_status()
            .await?
            .json()
            .await?;

    match response.remove("scheduled_maintenances") {
        Some(scheduled_maintenances) => Ok(serde_json::from_value(scheduled_maintenances)?),
        None => Ok(vec![]),
    }
}

/// Retrieves the upcoming maintenances from the status page.
pub async fn get_upcoming_maintenances() -> Result<Vec<Maintenance>> {
    let client = tls_client();
    let mut response: Object =
        retry(&mut || client.get(status_concat!("/scheduled-maintenances/upcoming.json")))
            .await
            .check_status()
            .await?
            .json()
            .await?;

    match response.remove("scheduled_maintenances") {
        Some(scheduled_maintenances) => Ok(serde_json::from_value(scheduled_maintenances)?),
        None => Ok(vec![]),
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

/// Send a request with the correct `UserAgent`, retrying it a second time if the
/// connection is aborted the first time.
async fn retry<'a, F: FnMut() -> reqwest::RequestBuilder>(
    f: &mut F,
) -> reqwest::Result<reqwest::Response> {
    // retry on a ConnectionAborted, which occurs if it's been a while since the last request
    match f().send().await {
        Err(err) if err.is_connect() => f().send().await,
        other => other,
    }
}

fn resolve_invite(invite: &str) -> &str {
    if invite.starts_with("http://discord.gg/") {
        &invite[18..]
    } else if invite.starts_with("https://discord.gg/") {
        &invite[19..]
    } else if invite.starts_with("discord.gg/") {
        &invite[11..]
    } else {
        invite
    }
}
