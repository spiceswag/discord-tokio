use crate::{ratelimit::RateLimits, Result};

use super::Discord;

/// User agent to use when logging into a bot account.
const BOT_USER_AGENT: &'static str = concat!(
    "DiscordBot (https://github.com/spiceswag/discord-tokio, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

/// User agent to use when logging into a user account.
const USERBOT_USER_AGENT: &'static str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36";

/// Login methods for creating a [`Discord`] API client.
pub trait LoginExt {
    /// Log in as a bot account using the given bot authentication token.
    /// The token will automatically be prefixed with `Bot `.
    fn from_bot_token(token: &str) -> Result<Discord>;

    /// Log in as a user account using the given user authentication token.
    fn from_user_token(token: &str) -> Result<Discord>;
}

impl LoginExt for Discord {
    /// Log in as a bot account using the given bot authentication token.
    /// The token will automatically be prefixed with `Bot `.
    fn from_bot_token(token: &str) -> Result<Discord> {
        Ok(Discord {
            rate_limits: RateLimits::default(),
            client: reqwest::Client::builder()
                .https_only(true)
                .user_agent(BOT_USER_AGENT)
                .build()
                .expect("Couldn't build HTTPS reqwest client"),
            token: format!("Bot {}", token.trim()),
        })
    }

    /// Log in as a user account using the given user authentication token.
    fn from_user_token(token: &str) -> Result<Discord> {
        Ok(Discord {
            rate_limits: RateLimits::default(),
            client: reqwest::Client::builder()
                .https_only(true)
                .user_agent(USERBOT_USER_AGENT)
                .build()
                .expect("Couldn't build HTTPS reqwest client"),
            token: token.trim().to_string(),
        })
    }
}
