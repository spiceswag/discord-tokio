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

mod login;
pub use login::*;

mod message;
pub use message::*;

mod server;
pub use server::*;

mod user;
pub use user::*;

use crate::{error::Result, ratelimit::RateLimits};

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
/// Here is a brief guide to those extension traits:
///
///
///
/// # Multiple Clients
///
/// Using multiple [`Discord`] clients is not advisable,
/// as each one of them will be tracking individual detached rate-limiting counters.
///
/// As most operations require only immutable access (`&self`) to the client,
/// courtesy of the rate-limits being held behind a [`Mutex`],
/// it is advisable to hold the client behind an [`Rc`] or [`Arc`]
///
/// [`Mutex`]: std::sync::Mutex
/// [`Rc`]: std::rc::Rc
/// [`Arc`]: std::sync::Arc
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
