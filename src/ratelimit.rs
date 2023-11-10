use std::collections::BTreeMap;
use std::sync::Mutex;
use std::{self, time::Duration};

use chrono::prelude::*;
use tracing::{error, warn};

use crate::{Error, Result};

#[derive(Default)]
pub struct RateLimits {
    // Mutexes here are expected to have basically 0 wait time
    global: Mutex<RateLimit>,
    endpoints: Mutex<BTreeMap<String, RateLimit>>,
}

impl RateLimits {
    /// Check before issuing a request for the given URL.
    pub fn pre_check(&self, url: &str) {
        self.global
            .lock()
            .expect("Rate limits poisoned")
            .pre_check();
        if let Some(rl) = self
            .endpoints
            .lock()
            .expect("Rate limits poisoned")
            .get_mut(url)
        {
            rl.pre_check();
        }
    }

    /// Update based on rate limit headers in the response for given URL.
    /// Returns `true` if the request was rate limited and should be retried.
    pub async fn post_update(&self, url: &str, response: &reqwest::Response) -> bool {
        if response.headers().get("X-RateLimit-Global").is_some() {
            self.global
                .lock()
                .expect("Rate limits poisoned")
                .post_update(response)
                .await
        } else {
            self.endpoints
                .lock()
                .expect("Rate limits poisoned")
                .entry(url.to_owned())
                .or_insert_with(RateLimit::default)
                .post_update(response)
                .await
        }
    }
}

#[derive(Default)]
struct RateLimit {
    reset: i64,
    limit: i64,
    remaining: i64,
}

impl RateLimit {
    async fn pre_check(&mut self) {
        // break out if uninitialized
        if self.limit == 0 {
            return;
        }

        let difference = self.reset - Utc::now().timestamp();
        if difference < 0 {
            // If reset is apparently in the past, optimistically assume that
            // the reset has occurred and we're good for the next three seconds
            // or so. When the response comes back we will know for real.
            self.reset += 3;
            self.remaining = self.limit;
            return;
        }

        // if no requests remain, wait a bit
        if self.remaining <= 0 {
            // 900ms in case "difference" is off by 1
            let delay = difference as u64 * 1000 + 900;
            warn!("pre-ratelimit: sleeping for {}ms", delay);
            tokio::time::sleep(Duration::from_millis(delay)).await;
            return;
        }

        // Deduct from our remaining requests. If a lot of requests are issued
        // before any responses are received, this will mean we can still limit
        // preemptively.
        self.remaining -= 1;
    }

    async fn post_update(&mut self, response: &reqwest::Response) -> bool {
        match self.try_post_update(response).await {
            Err(e) => {
                error!("rate limit checking error: {}", e);
                false
            }
            Ok(r) => r,
        }
    }

    async fn try_post_update(&mut self, response: &reqwest::Response) -> Result<bool> {
        if let Some(reset) = read_header(&response.headers(), "X-RateLimit-Reset")? {
            self.reset = reset;
        }
        if let Some(limit) = read_header(&response.headers(), "X-RateLimit-Limit")? {
            self.limit = limit;
        }
        if let Some(remaining) = read_header(&response.headers(), "X-RateLimit-Remaining")? {
            self.remaining = remaining;
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            if let Some(delay) = read_header(&response.headers(), "Retry-After")? {
                let delay = delay as u64 + 100; // 100ms of leeway
                warn!("429: sleeping for {}ms", delay);
                tokio::time::sleep(Duration::from_millis(delay)).await;
                return Ok(true); // retry the request
            }
        }
        Ok(false)
    }
}

fn read_header(headers: &reqwest::header::HeaderMap, name: &str) -> Result<Option<i64>> {
    match headers.get(name) {
        Some(hdr) => {
            if hdr.len() == 1 {
                match std::str::from_utf8(hdr.as_bytes()) {
                    Ok(text) => match text.parse::<i64>() {
                        Ok(val) => Ok(Some(val)),
                        Err(_) => match text.parse::<f64>() {
                            Ok(val) => Ok(Some(val as i64)),
                            Err(_) => Err(Error::Other("header is not an i64 or f64")),
                        },
                    },
                    Err(_) => Err(Error::Other("header is not UTF-8")),
                }
            } else {
                Err(Error::Other("header appears multiple times"))
            }
        }
        None => Ok(None),
    }
}
