//! Discord Rest API methods provided through a simple interface.
//!
//! The Discord Rest API is how applications modify the Discord state,
//! and obtain stateful Gateway connections for listening to events.
//!
//! The functionality of the [`Discord`] client is split over many
//! extension traits for the purposes of not importing dozens of methods
//! all at once.

mod channel;
pub use channel::*;

mod connect;
pub use connect::*;

mod login;
pub use login::*;

mod message;
pub use message::*;

mod server;
pub use server::*;

mod user;
pub use user::*;

use crate::{
    error::{CheckStatus, Result},
    model::{Incident, Maintenance},
    ratelimit::RateLimits,
    Object,
};

use reqwest::{Method, RequestBuilder};

/// Where the Discord API is mounted on the web.
const API_BASE: &'static str = "https://discord.com/api/v6";

/// Client for the Discord REST API.
///
/// # API families
///
/// As mentioned in the module level documentation,
/// the functionality of the [`Discord`] client is split over many
/// extension traits as to not import dozens of methods all at once,
/// instead importing only the necessary ones.
///
/// Here is a brief guide to those extension traits, in alphabetical order:
/// - `ChannelExt`: Interact with any sort of channel, in a server or outside of one.
/// - `LoginExt`: Login into the discord API from a bot or user token, or use the automated login system to generate a token.
/// - `MessageExt`: Send, edit, pin, and react to messages in channels.
/// - `ServerExt`: Create, fetch, update and delete servers, their invites and so on.
/// - `UserExt`: Fetch other users, or update the currently logged in one.
///
/// # Multiple Clients
///
/// Using multiple [`Discord`] clients is not advisable,
/// as each one of them will be tracking individual detached rate-limiting counters.
///
/// As 99.9% of operations require only immutable access (`&self`) to the client,
/// courtesy of the rate-limits being held behind a [`Mutex`][std::sync::Mutex],
/// it is best to hold the client behind an [`Rc`][std::rc::Rc] or [`Arc`][std::sync::Arc]
#[derive(Debug)]
pub struct Discord {
    /// Configured `reqwest` client for making request.
    client: reqwest::Client,
    /// The used token for making authorized requests.
    token: String,
    /// Keeping track of rate limits for this client.
    rate_limits: RateLimits,
}

impl Discord {
    /// Make a request while having rate limits and authorization taken care of.
    async fn request<F: FnOnce(RequestBuilder) -> RequestBuilder>(
        &self,
        url: &str,
        method: Method,
        builder: F,
    ) -> Result<reqwest::Response> {
        self.rate_limits.pre_check(url).await;

        let request = self.client.request(
            method,
            &format!(
                "{API_BASE}{}{}",
                if url.starts_with('/') { "" } else { "/" },
                url
            ),
        );

        let request = builder(request);

        // todo retries
        Ok(request.send().await?)
    }

    /// Make a request while having rate limits, retries, and authorization taken care of.
    ///
    /// Now comes in body free flavor.
    async fn empty_request(&self, url: &str, method: Method) -> Result<reqwest::Response> {
        self.request(url, method, |req| req).await
    }
}

const STATUS_BASE: &'static str = "https://status.discord.com/api/v2";
macro_rules! status_concat {
    ($e:expr) => {
        concat!("https://status.discord.com/api/v2", $e)
    };
}

/// Retrieves the current unresolved incidents from the status page.
pub async fn get_unresolved_incidents() -> Result<Vec<Incident>> {
    let client = tls_client();
    let mut response: Object = client
        .execute(
            client
                .get(status_concat!("/incidents/unresolved.json"))
                .build()
                .unwrap(),
        )
        .await
        .check_status()
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
    let mut response: Object = client
        .execute(
            client
                .get(status_concat!("/scheduled-maintenances/active.json"))
                .build()
                .unwrap(),
        )
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
    let mut response: Object = client
        .execute(
            client
                .get(status_concat!("/scheduled-maintenances/upcoming.json"))
                .build()
                .unwrap(),
        )
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

fn tls_client() -> reqwest::Client {
    reqwest::Client::builder()
        .https_only(true)
        .build()
        .expect("Couldn't build HTTPS reqwest client")
}
